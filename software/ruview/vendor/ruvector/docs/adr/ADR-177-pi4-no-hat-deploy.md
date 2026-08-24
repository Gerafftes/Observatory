---
adr: 177
title: "ruvector-hailo on Pi 4 / Pi 5 without AI HAT+ — first-class cpu-fallback deploy"
status: accepted
date: 2026-05-03
authors: [ruvnet, claude-flow]
related: [ADR-167, ADR-175, ADR-176]
---

# ADR-177 — Pi 4 / Pi 5 without AI HAT+ deploy

## Status

**Accepted** as of iter 171 (2026-05-03). The iter-137 standalone
cpu-fallback feature (`--features cpu-fallback` without `hailo`)
already produces a hardware-agnostic aarch64 binary that runs on
any aarch64 Linux without HailoRT installed. This ADR formalises
Pi 4 + Pi 5-without-HAT as a first-class deploy target so the
operator-facing docs stop implying NPU is required.

## Context

The hailo-backend branch had been framed Pi-5-+-AI-HAT+-centric
since iter 1. ADR-176 P5 confirmed the NPU acceleration story
(67.3 / sec, 9.6× over cpu-fallback) but the cpu-fallback path
itself (ADR-175 Option E, iter 147) is genuinely hardware-agnostic:

  * No `/dev/hailo0` access required
  * No HailoRT C library required
  * No PCIe NPU required
  * Just candle BertModel + safetensors mmap

Operators with:

  * Pi 4 (1/2/4/8 GB, Cortex-A72 ×4 @ 1.5 GHz, no AI HAT+)
  * Pi 5 without AI HAT+ (Cortex-A76 ×4 @ 2.4 GHz)
  * Any other aarch64 Linux SBC (Rock 5, Orange Pi 5, Jetson Nano, ...)

can run the cpu-fallback worker. The same gRPC contract, same
fingerprint integrity (iter 143), same systemd hardening (ADR-172
§3a), same multi-worker cluster dispatch (iter-149 mixed cluster).
The only difference is the raw-throughput ceiling.

## Decision

**Ship Pi 4 (and any aarch64-without-HailoRT) as a supported
deploy target.** The work is mostly documentation alignment:

1. `cross-build-bridges.sh --with-worker` already produces the
   right binary (cpu-fallback only, no libhailort link).
2. `download-cpu-fallback-model.sh` works unchanged on the Pi.
3. `install.sh` already accepts a model dir without `model.hef`.
4. The cluster README + ADR-176 already document the dual paths;
   this ADR makes Pi 4 explicit.

## Expected performance

Estimated throughput (no measurements on Pi 4 since this lab
doesn't have one — extrapolated from the iter-149 Pi 5 measurement
scaled by relative SPECint):

| Hardware | NPU | Throughput / worker | p50 latency |
|---|---|---:|---:|
| Pi 5 + AI HAT+ | Hailo-8 | **67.3 / sec** | **57 ms** |
| Pi 5 (no HAT, pool=4) | — | 7.0 / sec | 572 ms |
| Pi 4 (Cortex-A72, pool=4) | — | ~3-4 / sec (est) | ~1 s (est) |
| x86 release (pool=4) | — | 45.0 / sec | 175 ms |

For a 4-Pi-4 cluster: ~12-16 embeds/sec aggregate. Sufficient for
low-rate ingest workloads (a few queries/min from a single user).
RAG-style traffic with the iter-168 cache enabled scales to
millions of effective embeds/sec on cache hits regardless of
hardware.

## Memory cost

At `RUVECTOR_CPU_FALLBACK_POOL_SIZE=4`:

  * 90 MB safetensors mmap (shared)
  * ~10 MB candle graph structures × 4 instances
  * ~10 MB tonic / tokio runtime
  * ~5 MB tokenizer

Total resident ~120-150 MB. Fits comfortably even on Pi 4 1 GB.
Pool=2 brings this to ~110 MB if the operator is RAM-constrained.

## Operator deploy recipe

Identical to iter 165's cpu-fallback path:

```bash
# On x86 dev box, cross-build:
bash crates/ruvector-hailo-cluster/deploy/cross-build-bridges.sh --with-worker
scp crates/ruvector-hailo-cluster/target/aarch64-unknown-linux-gnu/release/ruvector-hailo-worker pi@pi4:/tmp/

# On the Pi 4 (or any aarch64 Linux):
bash deploy/download-cpu-fallback-model.sh /var/lib/ruvector-hailo/models/all-minilm-l6-v2
sudo bash deploy/install.sh /tmp/ruvector-hailo-worker /var/lib/ruvector-hailo/models/all-minilm-l6-v2
sudo systemctl start ruvector-hailo-worker
```

iter 145/167's startup self-test will print `sim_close > sim_far`
in journald regardless of arch — same correctness gate.

## Consequences

**Positive**:

  * Lowers the hardware bar for ruvector-hailo adoption from
    "$140 Pi 5 + $99 AI HAT+ + Hailo-8 module" to "any aarch64
    Linux box you have lying around"
  * Encourages mixed clusters: a few Pi 5+HAT NPU workers + many
    cpu-fallback Pi 4s for redundancy
  * Same security/observability/test surface — no new code paths

**Negative**:

  * Operator may set up a Pi 4 deploy expecting NPU performance
    and be disappointed by 3/sec — the docs need to be honest
    about the throughput delta
  * Cluster's iter-143 fingerprint distinguishes Pi 5+HAT NPU
    workers from cpu-fallback workers (HEF + safetensors hash vs
    safetensors-only hash) so they can't be mixed in the same
    fleet without `--allow-empty-fingerprint` — operators with
    mixed deploys must either run two clusters or skip the
    integrity check

**Neutral**:

  * Pi 4 throughput is documented but not measured — first
    operator with a Pi 4 should run cluster-bench and contribute
    a row to ADR-176's measurements table

## References

  * ADR-167 — original hailo-backend design (NPU-centric framing)
  * ADR-175 — Rust-side workarounds; Option E (cpu-fallback) is
    the path this ADR formalises for non-HAT hardware
  * ADR-176 — HEF integration EPIC (NPU acceleration)
  * iter 147 commit `4edd40432` — cpu-fallback embedder pool
  * iter 137 commit `1f54c1d63` — cpu-fallback works without
    `hailo` feature
