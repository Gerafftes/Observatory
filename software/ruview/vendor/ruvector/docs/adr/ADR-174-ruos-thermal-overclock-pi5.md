---
id: ADR-174
title: ruOS thermal optimizer + Pi 5 over/underclocking
status: Proposed
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, ruos, thermal, pi-5, overclock, underclock, edge-ai, power]
related: [ADR-167, ADR-171, ADR-173]
---

# ADR-174 — ruOS thermal optimizer

## Status

**Partially implemented** as of iter 98 (2026-05-02). Companion to
ADRs 171 + 173. Adds the **fifth workload** to the Pi 5 + AI HAT+
edge node: an in-process thermal supervisor that adjusts CPU clock
+ workload batch sizes in response to die temperature and per-workload
thermal weight.

**What's shipped (iter 91–98):**

- ✅ `crates/ruos-thermal` Rust crate with `ThermalSensor`, 5-profile `ClockProfile`
  enum (eco/default/safe-overclock/aggressive/max), and `apply_profile()` writer
- ✅ CLI binary with `--json`, `--prom`, `--show-profiles`, `--set-profile`,
  `--allow-cpufreq-write` double-opt-in gate
- ✅ systemd Type=oneshot service + 30s timer writing atomic
  textfile-collector output to `/var/lib/node_exporter/textfile_collector/ruos-thermal.prom`
- ✅ install.sh with hardened service unit (NoNewPrivileges, ProtectSystem=strict,
  MemoryDenyWriteExecute, SystemCallFilter=@system-service ~@privileged @resources)
- ✅ 6 CLI integration tests in `tests/cli.rs`
- ✅ cargo-deny CI

**What's still planned but not built (iter 95–97 follow-up):**

- ❌ Per-workload thermal subscriber Unix-socket budget protocol.
  Subscribers in `ruvector-hailo-worker` / `ruvllm-worker` / `ruview`
  that adapt batch size / inference cadence based on a published
  thermal-headroom budget from the supervisor.

  Deferred until the HEF compile pipeline lands (ADR-167) and there's
  a real thermal load to manage — currently the worker runs
  FNV-1a content-hash placeholders that don't stress the NPU.

## Context

A Pi 5 + AI HAT+ running the full edge stack from ADRs 171/173:

```
ruvector-hailo-worker  +  mcp-brain  +  ruview  +  ruvllm-worker
```

…draws **~10-15 W sustained** with no active cooling. The BCM2712
throttles at 85°C and hardware-cuts at 90°C. Without active cooling,
real-world enclosures see 80-85°C at sustained load — right against
the throttle band.

Three failure modes today:
1. **Silent throttling**: kernel halves the clock at 85°C, throughput
   drops 40-50%, no observability except `vcgencmd measure_temp`.
2. **Workload starvation**: heavy LLM prefill (NPU-bound) saturates the
   thermal budget, forcing embed/pose/brain workloads into kernel-
   imposed throttling that's blind to per-workload priority.
3. **No power-budget option**: Pi 5 in battery / solar deployments has
   no way to opt into a lower clock for power savings; default 2.4 GHz
   is always-on.

## Decision

A new **`ruos-thermal`** crate + systemd service, integrated with the
existing four workloads via a Unix-socket coordination protocol.

### Architecture

```
┌──────────────────────────────────────────────────────────┐
│             ruos-thermal.service (root)                  │
│ ┌──────────────────────────────────────────────────────┐ │
│ │  read /sys/class/thermal/thermal_zone*/temp         │ │
│ │  read /sys/devices/system/cpu/cpufreq/policy*/*     │ │
│ │  read hailortcli sensor              (Hailo NPU °C) │ │
│ │  ──────────────── decide ──────────────────────────  │ │
│ │  budget = f(cpu_temp, npu_temp, mode, governor)     │ │
│ │  publish budget to /run/ruos-thermal/budget.sock    │ │
│ │  on overheat: lower max_freq via scaling_max_freq   │ │
│ └──────────────────────────────────────────────────────┘ │
│                                                          │
│   subscribers: ruvector-hailo-worker, ruvllm-worker,     │
│                ruview, mcp-brain (each adjusts batch     │
│                size / inference rate from budget)        │
└──────────────────────────────────────────────────────────┘
```

### Clock profiles

The supervisor exposes 5 named profiles via the Unix socket:

| Profile | CPU max | Estimated draw | Cooling required |
|---|---:|---:|---|
| `eco` | 1.4 GHz | ~3 W | passive |
| `default` | 2.4 GHz | ~5 W | passive (small heatsink) |
| `safe-overclock` | 2.6 GHz | ~7 W | passive (large heatsink) |
| `aggressive` | 2.8 GHz | ~10 W | active fan |
| `max` | 3.0 GHz | ~13 W | heatsink + fan, monitored |

Profile changes write to `/sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq`
(and policies 1-3 for the four cores). `/boot/firmware/config.txt` is
edited only when crossing the default→overclock boundary, and only
behind an explicit `--allow-boot-edit` flag (one-time at install).

Auto-revert on thermal trip: if any thermal_zone exceeds 80°C, the
supervisor drops to the next-lower profile and stays there for at
least 60 seconds before considering re-promote.

### Per-workload thermal budget

The thermal supervisor publishes a single `budget` value (0..1.0)
representing "fraction of max sustained throughput available right now":

| State | Budget | Action |
|---|---:|---|
| `cpu < 60°C, npu < 60°C` | 1.0 | Workloads run at max batch size |
| `cpu 60-75°C` or `npu 60-75°C` | 0.8 | Workloads halve batch size |
| `cpu 75-80°C` or `npu 75-80°C` | 0.5 | Embed pauses; LLM only at p99>5s |
| `cpu > 80°C` or `npu > 80°C` | 0.2 | All non-essential workloads pause |
| `cpu > 85°C` or `npu > 85°C` | 0.0 | Emergency: mcp-brain only; rest stops |

Workloads subscribe by connecting to `/run/ruos-thermal/budget.sock`
(SOCK_DGRAM, JSON line per update). They self-throttle by adjusting
their internal batch / concurrency / RPC ack rate. The supervisor never
kills processes — it advises; workloads cooperate.

### Hailo NPU thermal awareness

The Hailo-8 has its own thermal sensor exposed via:

```bash
$ hailortcli sensor temperature show
0001:01:00.0  temperature: 47.3°C
```

The supervisor polls this every 5s, factors into the budget. NPU has a
narrower thermal envelope than CPU (Hailo throttles ~75°C, hard-cuts
80°C); the budget table treats NPU temp with stricter thresholds.

### Power-saving modes

For battery / solar / fan-less deployments, the operator selects a
profile at `ruos-thermal.service` start:

```bash
# Eco mode — lowest power, narrow workload scope
systemd-run --setenv=RUOS_THERMAL_PROFILE=eco ruos-thermal

# Aggressive — assumes active cooling, max throughput
systemd-run --setenv=RUOS_THERMAL_PROFILE=aggressive ruos-thermal
```

Eco mode also writes:
- `cpu_governor = powersave` (vs `performance`)
- `disable_pcie_l1_aspm = 0` (allow ASPM, reduces idle PCIe draw)

### Observability

Three Prometheus metrics on `/var/lib/node_exporter/textfile_collector/ruos-thermal.prom`:

```
ruos_thermal_cpu_temp_celsius{policy="0"} 62.3
ruos_thermal_cpu_temp_celsius{policy="1"} 61.8
ruos_thermal_cpu_temp_celsius{policy="2"} 63.1
ruos_thermal_cpu_temp_celsius{policy="3"} 62.5
ruos_thermal_npu_temp_celsius 47.3
ruos_thermal_cpu_freq_hz{policy="0"} 2400000000
ruos_thermal_budget 0.8
ruos_thermal_profile_active{profile="default"} 1
ruos_thermal_throttle_events_total 0
```

Pair with the existing `ruvector-hailo-fleet.prom` (ADR-168 §stats
binary) for unified fleet thermal observability.

## Consequences

**+** Throttle events become rare and visible; pre-iter-91 they were
       silent kernel events
**+** Each Pi can be tuned to its enclosure (passive vs fanned vs
       battery) without per-deploy code changes
**+** Workload coordination eliminates the "LLM steals all NPU + CPU,
       embeds queue forever" failure mode by giving embed a thermal-
       budget-derived priority knob
**+** Power-saving modes unlock battery / solar deployments; an `eco`
       Pi 5 + Hailo-8 draws ~3 W and runs ruview pose detection for
       ~50 hours on a 10 Ah USB-PD battery
**−** Adds a fifth long-running daemon to each Pi; ~5 MB RSS
**−** Supervisor needs root for cpufreq writes; isolated by
       CapabilityBoundingSet=CAP_SYS_NICE in the systemd unit
**−** Overclock profiles void warranty + can degrade silicon over time
       at 3.0 GHz; documented prominently in the install path

## Implementation roadmap (post-merge)

Parallel to ADR-172 security work and ADR-173 ruvllm work:

| Iter | Item | Notes |
|---|---|---|
| 91 | `crates/ruos-thermal` skeleton + sysfs reader | Pure-read first; ~200 LOC |
| 92 | Profile switching + safe overclock validated | Requires write access; behind `--allow-cpufreq-write` |
| 93 | Hailo NPU sensor integration + npu temp in budget | shells `hailortcli sensor temperature show` |
| 94 | `ruos-thermal.service` systemd unit + install.sh | Drops into `deploy/` next to ruvector-hailo-worker |
| 95 | Per-workload subscriber stub for ruvector-hailo-worker | Worker reads budget, scales `--cache-keyspace` accordingly |
| 96 | Subscriber for ruvllm-worker (LLM-bound, biggest thermal win) | Pauses prefill when budget < 0.3 |
| 97 | Subscriber for ruview (continuous workload, hardest scheduling) | Drops CSI sample rate from 30Hz → 10Hz under thermal pressure |

## Out of scope

* Fan PWM control — Pi 5 active cooler uses kernel-managed PWM via
  `/sys/devices/platform/cooling_fan/hwmon`; supervisor only reads
  fan RPM, doesn't override
* GPU clock — Pi 5 GPU is unused by this stack; default 910 MHz fine
* DDR overclock — risk/reward poor; default LPDDR4X timings stay

## Combined edge-node thermal envelope

With ADR-174 + 171 + 173 + 167:

| Profile | Workloads enabled | Sustained watts | Throughput (mixed) |
|---|---|---:|---:|
| eco | embed + brain (no LLM, no pose) | ~3 W | ~50 embed/s |
| default | all four | ~10 W | ~150 mixed inferences/s |
| safe-overclock | all four + 10% boost | ~12 W | ~165 mixed/s |
| aggressive | all four + 25% boost | ~15 W | ~180 mixed/s |
| max | all four + 35% boost | ~18 W | ~200 mixed/s |

The thermal supervisor turns "deploy a Pi 5 + AI HAT+" from a single
fixed configuration into a tunable platform — choose your watts, get
your throughput, never silent-throttle.
