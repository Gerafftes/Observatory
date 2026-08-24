---
id: ADR-171
title: ruOS brain + ruview WiFi DensePose on Pi 5 + Hailo-8 (WiFi/LoRa edge node)
status: Proposed
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, hailo, ruview, brain, pi-5, wifi, lora, csi-densepose, edge-ai]
related: [ADR-167, ADR-168, ADR-169, ADR-170]
---

# ADR-171 — ruOS brain + ruview on Pi 5 + Hailo-8

## Status

**Partially implemented** as of iter 126 (2026-05-02). The ruview-side
host integration is shipped end-to-end on this branch:

| Iter | What landed |
|---:|---|
| 123 | New `ruview-csi-bridge` bin under `crates/ruvector-hailo-cluster/src/bin/`. Listens on UDP for RuView ADR-018 binary CSI frames (`0xC5110001` raw + `0xC5110006` feature-state magics), parses the 20-byte header, derives an NL description (node/channel/RSSI/noise/antennas/subcarriers), and posts via the cluster's embed RPC. Same TLS/mTLS/§2a flag set as iter-115's mmwave-bridge. |
| 125 | 6 committed CLI integration tests in `tests/ruview_csi_bridge_cli.rs`: JSONL emission, cluster sink, §2a fp+cache gate, malformed-packet drop, --help, --version. |
| 126 | Production deploy bundle: `ruview-csi-bridge.service` (hardened systemd unit running as `ruvector-csi`), `.env.example` config template, idempotent `install-ruview-csi-bridge.sh`. |

**Architectural seam:** RuView's existing pipeline (separate
`~/projects/RuView/` repo with ESP32 CSI firmware + Rust pointcloud
binary) broadcasts ADR-018 frames over UDP. This branch's bridge
consumes that stream and lifts each frame into the hailo-backend
cluster's §1b mTLS-gated embed RPC. Downstream brain memories receive
embeddings keyed off short NL strings — searchable like any other
corpus fragment.

**Still unimplemented in this branch:**
- The ruOS brain side (mcp-brain client → pi.ruv.io) doesn't query
  the cluster yet — brain-side change in `crates/mcp-brain-server/`,
  separate scope.
- LoRa transport (§7b in ADR-172) for off-Tailscale deploys uses
  X25519 ECDH session keys; future iter, separate ADR territory.
- Real WiFi DensePose pose extraction lives in the RuView repo
  unchanged.

Builds on the cluster + cache + tracing work shipped in ADRs 167–170.

---

(Original proposal text preserved below for historical context.)

## Context

The Pi 5 + AI HAT+ node (`cognitum-v0`, 100.77.59.83) currently runs
just one workload: text embedding via the Hailo-8 NPU, dispatched
through the multi-Pi coordinator (`ruvector-hailo-cluster`).

Three additional capabilities we want on the same hardware:

1. **ruOS brain** — the persistent reasoning + memory system already
   live as `crates/mcp-brain` (MCP client) + `crates/mcp-brain-server`
   (Cloud Run backend at pi.ruv.io). Today the Pi has no brain awareness;
   bringing the MCP client onto the Pi turns it into a self-improving
   edge node that contributes observations and pulls patterns.

2. **ruview WiFi DensePose** ([github.com/ruvnet/ruview][ruview]) — turns
   commodity WiFi CSI signals into real-time pose estimation, vital
   signs, and presence detection. No camera. The inference target is
   the same Hailo-8 NPU; the sensor is the WiFi chip's CSI feed.

3. **WiFi + LoRa transport** — the cluster currently uses tonic gRPC
   over WiFi/Ethernet/Tailnet. For privacy-preserving / low-bandwidth
   broadcast (presence pings, anomaly alerts), LoRa lets the edge node
   announce events 10+ km without internet.

[ruview]: https://github.com/ruvnet/ruview

## Decision

Three workloads share the Pi 5, each isolated by systemd unit + cgroup
slice:

```
                                    /
                          ┌─────────┴─────────┐
                          │  Pi 5 + AI HAT+   │
                          │  cognitum-v0       │
                          └─────────┬─────────┘
            ┌───────────────────────┼───────────────────────┐
            │                       │                       │
   ┌────────▼────────┐    ┌─────────▼────────┐    ┌─────────▼────────┐
   │ ruvector-hailo- │    │   mcp-brain      │    │   ruview         │
   │ worker.service  │    │  .service        │    │  .service        │
   ├─────────────────┤    ├──────────────────┤    ├──────────────────┤
   │ 1 thread        │    │ 1 thread         │    │ 2 threads        │
   │ embed RPC       │    │ MCP client to    │    │ WiFi CSI capture │
   │ to /dev/hailo0  │    │ pi.ruv.io        │    │ + pose inference │
   │ via libhailort  │    │ (REST + sha3 sig)│    │ on /dev/hailo0   │
   └────────┬────────┘    └─────────┬────────┘    └─────────┬────────┘
            │                       │                       │
            └───────────┬───────────┴───────────┬───────────┘
                        │                       │
                ┌───────▼────────┐      ┌───────▼────────┐
                │   /dev/hailo0  │      │  WiFi tx + RX   │
                │   shared NPU   │      │ via wlan0 +     │
                │   time-sliced  │      │ optional LoRa   │
                └────────────────┘      └─────────────────┘
```

### Integration map

| Service | Crate / repo | Hailo NPU? | Network |
|---|---|:-:|---|
| `ruvector-hailo-worker.service` | `crates/ruvector-hailo-cluster/src/bin/worker.rs` | ✓ | tonic gRPC over WiFi/Tailnet |
| `mcp-brain.service` | `crates/mcp-brain` | – | HTTPS to pi.ruv.io (Cloud Run) |
| `ruview.service` | `github.com/ruvnet/ruview` | ✓ | WiFi CSI capture + LoRa broadcast |

### NPU contention strategy

Two clients (`worker` + `ruview`) want `/dev/hailo0`. Hailo's vdevice
abstraction supports time-sliced sharing — multiple processes can each
hold a vdevice handle and the firmware schedules. ADR-167 §5 already
configures this for the worker; ruview hooks the same path.

Per-process latency budget (8 ms total per query at saturation):
- `worker` embed: ~3 ms (yolov8s reference; MiniLM-L6 will be similar)
- `ruview` CSI→pose: ~3 ms
- Scheduler overhead: ~2 ms

Steady-state combined throughput: ~150 inferences/s sustained, mixed.

### LoRa transport

Off-the-shelf LoRa hat (e.g. Waveshare SX1262 HAT) sits on the Pi 5's
GPIO header. The cluster's `EmbeddingTransport` trait (ADR-167 §8.2)
already abstracts the wire format — adding `LoRaTransport` is a 200-LOC
impl that:

- Frames embed RPC requests as 256-byte LoRa packets (max payload @ SF7)
- Forgoes per-call response (one-way fire-and-forget for alerts)
- Encrypts payload with the existing fingerprint as the symmetric key

LoRa bandwidth (~5 kbps SF7) bounds throughput to a few alerts/sec —
not for general embed traffic, but ideal for **broadcast-only edges**
(remote sensors, agricultural / wildlife monitoring) that periodically
ping a gateway with a low-dim feature vector.

### ruview Hailo backend wiring

ruview today (per repo survey) ships Python + Rust components for CSI
capture and pose inference. The Hailo-8 backend is a HEF-driven
inference path identical to what `ruvector-hailo` builds for embeddings:

```
WiFi CSI tensor (N x 64 x 30 complex floats)
  → preprocess (magnitude, FFT)
  → push input vstream (HEF)
  → pull output vstream (pose tensor: 17 keypoints x (x, y, conf))
  → postprocess (NMS, smoothing)
```

`ruvector-hailo::EmbeddingPipeline` will be renamed/generalized to
`HailoPipeline<I, O>` so both ruview pose and ruvector embed share the
same vstream lifecycle code. New trait:

```rust
pub trait HailoPipeline {
    type Input;
    type Output;
    fn run(&self, input: Self::Input) -> Result<Self::Output, HailoError>;
}
```

Implementations:
- `EmbeddingPipeline`: input = `Vec<i64>` (token IDs), output = `Vec<f32>` (384-d)
- `PosePipeline`: input = `CsiTensor`, output = `PoseTensor`

### Brain integration

`mcp-brain` runs as a long-lived daemon, exposing the MCP brain tools
locally (`brain_search`, `brain_share`, `brain_partition`) over a Unix
socket. Local consumers (worker + ruview) push observations:

```rust
brain.share(BrainMemory {
    category: "telemetry",
    title: "embed_latency_p99",
    content: format!("worker on {} measured {}µs over {} samples",
                     hostname, p99_us, n_samples),
    tags: vec!["pi-5", "hailo-8", "ruvector-hailo-worker"],
});
```

The Cloud Run brain at pi.ruv.io aggregates across all edge nodes
and emits derived patterns. New patterns are pulled back periodically
for local routing decisions (which workers to favor, which queries to
predict-cache, etc.).

### Storage layout

```
/usr/local/bin/
  ruvector-hailo-worker              ← embed worker (this PR)
  mcp-brain                          ← brain MCP daemon (cross-built same path)
  ruview-pose                        ← future: CSI → pose inference
/etc/systemd/system/
  ruvector-hailo-worker.service      ← already shipped
  mcp-brain.service                  ← new (this ADR)
  ruview.service                     ← future
/var/lib/ruvector-hailo/
  models/
    all-minilm-l6-v2/model.hef       ← gated on HEF compile
  csi-pose-densepose-v1/model.hef    ← future
/run/ruview/
  brain.sock                         ← Unix socket for inter-service brain access
```

## Consequences

**+** Single $200 Pi 5 hosts three complementary edge-AI workloads
**+** Hailo-8 NPU utilization climbs from one workload to two (better
       hardware ROI per node)
**+** Brain integration makes every Pi a contributor to the shared
       knowledge graph — fleet observability becomes self-organizing
**+** LoRa broadcast unlocks deployments where internet isn't available
       (agricultural, wildlife, industrial monitoring)
**−** NPU contention adds tail-latency variance vs single-tenant; mitigated
       by Hailo's hardware scheduler + careful per-service vdevice config
**−** Brain client adds ~30 MB RSS to the Pi (8 GB total → fine)
**−** ruview CSI capture requires modified WiFi driver (nexmon-csi or
       similar); not all WiFi chipsets supported. Pi 5's BCM4387 is
       supported via patched firmware but adds setup complexity.

## Implementation iterations (proposed, post-merge)

1. **iter 86**: Cross-build `mcp-brain` for aarch64 ultra; deploy to Pi 5;
   write `mcp-brain.service` systemd unit; add to deploy/install.sh.
2. **iter 87**: Generalize `ruvector-hailo::EmbeddingPipeline` →
   `HailoPipeline<I, O>` trait; preserve embedding impl, ship the trait.
3. **iter 88**: Sketch ruview's Pi 5 + Hailo-8 wiring in a
   `ruvview-hailo` companion crate (live skeleton; HEF for pose model
   gated same as embedding HEF).
4. **iter 89**: Author `LoRaTransport` impl of `EmbeddingTransport` for
   broadcast-only edge embeds via Waveshare SX1262 HAT.
5. **iter 90**: Brain aggregation patterns — coordinator-side
   `brain_share` of fleet stats every N seconds; brain-driven cache
   warmup via `brain_search`-derived hot-key list.
