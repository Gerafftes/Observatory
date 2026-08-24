---
id: ADR-168
title: ruvector-hailo-cluster CLI surface (3-binary split + flag conventions)
status: Accepted
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, hailo, cli, ops, ux]
related: [ADR-167, ADR-169, ADR-170]
---

# ADR-168 — Cluster CLI surface

## Context

The cluster crate (`ruvector-hailo-cluster`) is consumed in three
distinct operational modes:

1. **Ingestion** — pipe a corpus through, get vectors out
2. **Observability** — peek at fleet health/throughput from a script
3. **Benchmarking** — sustained-load measurement for CI regression

Cramming all three into one binary (`ruvector-hailo`) was the obvious
first cut. The downsides emerged immediately during iter-67-onward
operational work:

- Per-mode flag spaces conflict (`--watch` belongs to stats, `--batch`
  to embed, `--concurrency` to bench)
- Help text becomes unreadable
- Distinct exit-code semantics blur (embed: 0|1, stats: 0|1|2|3, bench: 0|1|2)
- Signal handling differs (embed exits on stdin EOF; bench exits on
  duration; stats runs forever in `--watch`)

## Decision

Three user-facing binaries plus two server binaries (5 total),
plus a sensor-bridge bin added in iter 116:

| Binary | Role | Stdin? | Long-running? |
|---|---|:-:|:-:|
| `ruvector-hailo-worker` | Server: real Hailo NPU | n/a | yes (systemd) |
| `ruvector-hailo-fakeworker` | Server: deterministic mock | n/a | yes |
| `ruvector-hailo-embed` | Client: stdin / `--text` → JSONL | yes | no (EOF exits) |
| `ruvector-hailo-stats` | Client: fleet observability | no | optional `--watch` |
| `ruvector-hailo-cluster-bench` | Client: load harness | no | bounded duration |
| `ruvector-mmwave-bridge` | Sensor: 60 GHz mmWave radar UART → cluster embed RPC (iter 116) | n/a (UART or simulator) | yes (radar event stream) |
| `ruview-csi-bridge` | Sensor: RuView ADR-018 CSI UDP → cluster embed RPC (ADR-171, iter 123) | n/a (UDP listener) | yes (CSI frame stream) |
| `ruvllm-bridge` | LLM seam: JSONL stdin → cluster embed RPC → JSONL stdout (ADR-173, iter 124) | yes (one JSON request per line) | yes (until EOF) |

### Shared flag vocabulary

All 3 user-facing binaries accept the same discovery + safety flags:

```
DISCOVERY (exactly one):
  --workers <csv>                 inline addresses
  --workers-file <path>           text manifest (one host:port or `name = host:port` per line)
  --tailscale-tag <tag> [--port N]  tag-based via `tailscale status --json`

SAFETY:
  --fingerprint <hex>             explicit expected fingerprint
  --auto-fingerprint              probe one worker, use its fp as expected
  --validate-fleet                boot-time integrity check (exit 2 on failure)
  --health-check <secs>           background runtime probe + auto-cache-clear

OUTPUT:
  --quiet                         suppress informational stderr/stdout
  --version, -V                   print pkg-name + semver, exit 0
  --help, -h                      print full help, exit 0
```

### Per-binary specifics

| Flag | embed | stats | bench |
|---|:-:|:-:|:-:|
| `--batch <N>` | ✓ | n/a | n/a (always batched) |
| `--cache <N>` / `--cache-ttl <secs>` | ✓ | n/a | ✓ |
| `--cache-keyspace <N>` | n/a | n/a | ✓ |
| `--text <s>` (repeatable) | ✓ | n/a | n/a |
| `--output head\|full\|none` | ✓ | n/a | n/a |
| `--request-id <id>` | ✓ | n/a | ✓ (suffixed `.t<tid>.c<counter>`) |
| `--validate-only` | ✓ | n/a | n/a |
| `--watch <secs>` / `--max-iters <N>` | n/a | ✓ | n/a |
| `--strict-homogeneous` | n/a | ✓ | n/a |
| `--list-workers` | n/a | ✓ | n/a |
| `--json` / `--prom` / `--prom-file` | n/a | ✓ | n/a (`--prom <path>` only) |
| `--concurrency <N>` / `--duration-secs <N>` | n/a | n/a | ✓ |

### Exit-code conventions

```
0   success
1   bad CLI args / discovery failed
2   ops-level failure (validate FAILED, stats RPC error, bench worker dead)
3   stats: --strict-homogeneous + drift detected
```

Bench shares `0|1|2`; embed shares `0|1|2`; stats adds `3`. CI gates
key off these without parsing stdout.

## Consequences

**+** Each binary's help text fits on a screen
**+** Mutually exclusive concerns (input, output, lifecycle) stay isolated
**+** Bash composition becomes natural: `embed | jq | <consumer>`,
       `stats --strict-homogeneous && bench` (precondition gate),
       `bench --quiet --prom <path>` (silent CI artifact)
**+** Test pyramid maps 1:1 onto binaries: per-binary CLI integration
       test files (`tests/embed_cli.rs`, etc.) keep test scopes tight
**−** Three binary names to `apt install` / install.sh; trade-off accepted
**−** Help-text duplication across binaries for shared flags; mitigated
       by docstrings in code that get rendered identically by `print_help()`

## Implementation

Iters 17 onwards. Pre-iter-15: single binary. By iter-72: 18 CLI tests
(6 per binary) + 7 doctests covering the entire surface.
