# wifi-densepose-sensing-server

[![Crates.io](https://img.shields.io/crates/v/wifi-densepose-sensing-server.svg)](https://crates.io/crates/wifi-densepose-sensing-server)
[![Documentation](https://docs.rs/wifi-densepose-sensing-server/badge.svg)](https://docs.rs/wifi-densepose-sensing-server)
[![License](https://img.shields.io/crates/l/wifi-densepose-sensing-server.svg)](LICENSE)

Lightweight Axum server for real-time WiFi sensing with RuVector signal processing.

## Overview

`wifi-densepose-sensing-server` is the operational backend for WiFi-DensePose. It receives raw CSI
frames from ESP32 hardware over UDP, runs them through the RuVector-powered signal processing
pipeline, and broadcasts processed sensing updates to browser clients via WebSocket. A built-in
static file server hosts the sensing UI on the same port.

The crate ships both a library (`wifi_densepose_sensing_server`) exposing the training and inference
modules, and a binary (`sensing-server`) that starts the full server stack.

Integrates [wifi-densepose-wifiscan](../wifi-densepose-wifiscan) for multi-BSSID WiFi scanning
per ADR-022 Phase 3.

## Features

- **UDP CSI ingestion** -- Receives ESP32 CSI frames on port 5005 and parses them into the internal
  `CsiFrame` representation.
- **Vital sign detection** -- Pure-Rust FFT-based breathing rate (0.1--0.5 Hz) and heart rate
  (0.67--2.0 Hz) estimation from CSI amplitude time series (ADR-021).
- **RVF container** -- Standalone binary container format for packaging model weights, metadata, and
  configuration into a single `.rvf` file with 64-byte aligned segments.
- **RVF pipeline** -- Progressive model loading with streaming segment decoding.
- **Graph Transformer** -- Cross-attention bottleneck between antenna-space CSI features and the
  COCO 17-keypoint body graph, followed by GCN message passing (ADR-023 Phase 2). Pure `std`, no ML
  dependencies.
- **SONA adaptation** -- LoRA + EWC++ online adaptation for environment drift without catastrophic
  forgetting (ADR-023 Phase 5).
- **Contrastive CSI embeddings** -- Self-supervised SimCLR-style pretraining with InfoNCE loss,
  projection head, fingerprint indexing, and cross-modal pose alignment (ADR-024).
- **Sparse inference** -- Activation profiling, sparse matrix-vector multiply, INT8/FP16
  quantization, and a full sparse inference engine for edge deployment (ADR-023 Phase 6).
- **Dataset pipeline** -- Training dataset loading and batching.
- **Multi-BSSID scanning** -- Windows `netsh` integration for BSSID discovery via
  `wifi-densepose-wifiscan` (ADR-022).
- **WebSocket broadcast** -- Real-time sensing updates pushed to all connected clients at
  `ws://localhost:8765/ws/sensing`.
- **Static file serving** -- Hosts the sensing UI on port 8080 with CORS headers.
- **Lossless ESP32 raw capture** -- Records validated UDP CSI frames as strict
  JSONL plus a setup-bound sidecar without converting or labelling the I/Q
  samples.
- **Experimental fixed-room fingerprinting** -- Builds and blindly evaluates a
  fail-closed, discrete nine-point position model from four receivers. It may
  return `unknown` or `ambiguous` and does not claim continuous coordinates.

## Modules

| Module | Description |
|--------|-------------|
| `vital_signs` | Breathing and heart rate extraction via FFT spectral analysis |
| `rvf_container` | RVF binary format builder and reader |
| `rvf_pipeline` | Progressive model loading from RVF containers |
| `graph_transformer` | Graph Transformer + GCN for CSI-to-pose estimation |
| `trainer` | Training loop orchestration |
| `dataset` | Training data loading and batching |
| `sona` | LoRA adapters and EWC++ continual learning |
| `sparse_inference` | Neuron profiling, sparse matmul, INT8/FP16 quantization |
| `embedding` | Contrastive CSI embedding model and fingerprint index |
| `raw_csi_recording` | Lossless, unlabelled ESP32 CSI recording and strict sidecar metadata |
| `raw_csi_replay` | Deterministic replay of recorded CSI frames |
| `classification_evaluation` | Truth-separated, hash-bound fixed-protocol classification verdict |
| `experiment_evaluation` | Final combined Classification-and-Position verdict for one sealed setup |
| `d6_fingerprint` | Gain-normalized CSI-shape reference and signed empty-room residuals |
| `position_capture` | Quality-gated `4 × 28` fixed-room feature extraction |
| `position_fingerprint` | Robust classifier for exactly nine discrete points |
| `position_offline` | Inspection, index building, blind prediction, and separated evaluation |
| `position_evaluation` | Truth-bound coverage, accuracy, error, and confusion reporting |
| `position_setup` | Canonical sealed setup identity for room, devices, radio grid, and software revision |
| `position_live` | Fail-closed live fingerprint runtime with D6 presence gating and temporal consensus |

## Quick Start

```bash
# The live RuField surface requires a deployment-specific signing seed.
export WDP_RUFIELD_SIGNING_SEED="$(openssl rand -hex 32)"

# Build the server
cargo build -p wifi-densepose-sensing-server

# Run with default settings (HTTP :8080, UDP :5005, WS :8765)
cargo run -p wifi-densepose-sensing-server

# Run with custom ports
cargo run -p wifi-densepose-sensing-server -- \
    --http-port 9000 \
    --udp-port 5005 \
    --static-dir ./ui
```

The server refuses to start without `WDP_RUFIELD_SIGNING_SEED`; it never uses
a built-in signing key. The default bind is loopback-only. For a routable
`--bind-addr`, also set a strong `RUVIEW_API_TOKEN`; otherwise startup is
rejected. Browser-origin checks protect live WebSockets and state-changing API
requests, while the Host allowlist remains enabled by default. By default,
only the exact local UI Origins on `--http-port` are accepted. If a browser UI
is served from another port or host, configure it explicitly:

```bash
SENSING_ALLOWED_ORIGINS="http://localhost:3000" \
  cargo run -p wifi-densepose-sensing-server -- --http-port 8080
```

Origins must be complete `http(s)://host[:port]` values without a resource path,
wildcard, or host-only entry. `--allowed-origin` can be repeated; the
comma-separated `SENSING_ALLOWED_ORIGINS` variable is equivalent. The
dedicated WebSocket port remains compatible with a UI served from the allowed
HTTP Origin, and non-browser clients without an `Origin` header remain usable.

### Experimental fixed-room position workflow

The offline position workflow is deliberately separate from normal server
startup. Every command writes a new JSON artifact and refuses to overwrite an
existing file.

Create a sealed setup from the strict setup specification before recording or
building an index:

```bash
sensing-server \
    --position-create-setup setup-spec.json \
    --position-output sealed-setup.json
```

`radio.tx_filter_identity` uses
`sha256-ruview-tx-filter-mac-v1`: SHA-256 over exactly the six binary bytes
written to the RX NVS `filter_mac` blob in network order. The textual MAC,
separators, letter case, whitespace, and a trailing NUL are not hashed.
Setup-bound CSI additionally requires the firmware's per-datagram runtime
source-binding trailer: filter enabled, actual frame source matched that
filter, identity valid, and the same digest as the sealed setup. A missing,
partial, malformed, stale, or mismatching binding fails closed before the
frame can affect classification, liveness, D4/D5/D6, position, or recording.
This is a controlled-firmware runtime assertion, not adversarial
cryptographic device authentication.

When the live server is started with `--position-setup`, the setup identity is
available at `GET /health/ready` under `position_setup`, independently of
whether a position index has already been loaded. `position_index` reports the
separate index state.

For the fixed-room protocol, use the guarded recorder instead of adding point
labels to the raw capture:

```bash
python3 scripts/capture_position_run.py \
    --kind discovery \
    --recording-id discovery-neutral-01

python3 scripts/capture_position_run.py \
    --kind preflight \
    --recording-id preflight-neutral-01

python3 scripts/capture_position_run.py \
    --kind empty \
    --recording-id empty-neutral-01 \
    --profile-id profile-... \
    --profile-revision-id profile-...-v2 \
    --confirm-empty-room

python3 scripts/capture_position_run.py \
    --kind position \
    --recording-id neutral-01
```

Run `discovery` first on the final physical setup while the server has no
position setup loaded. Use its stable per-RX grids to create the setup, restart
the server with that seal, and then run `preflight`. Setup-bound runs require
a fresh ESP32 source, the active sealed setup, exactly RX1-RX4, a fresh
matching runtime TX binding from every RX, at least 5 Hz and full duration per
RX, a stable per-RX grid, zero recorder drops, and a completed sidecar bound to
the same setup ID and hash. A server-side maximum duration also finalizes a
capture if the client loses the start/stop response. The `start` body contains
only the neutral recording ID and the watchdog duration; labels and blind
truth remain external.

For `--kind empty`, the runner also starts D5/D6 calibration before recording,
stops it after the lossless capture, verifies valid D5 and D6 references for
exactly RX1-RX4, and waits for fresh operational D6 evidence. Recording and
calibration cleanup remain independent, so a lost start/stop response cannot
silently leave calibration collecting. The profile ID and immutable revision
are mandatory: the runner rejects a contextless start and reports success only
after the persisted bundle returns matching setup/profile identities,
`calibration_id`, `calibration_context_sha256`, and at least 60 seconds of
collection.

Audit the radar transform and transport window with status snapshots from the
same server process. Both snapshots are mandatory because rejected packets
(including `room_bounds`) are not written to the accepted session JSONL:

```bash
curl -fsS http://127.0.0.1:8080/api/v1/mmwave/status \
    -o /tmp/mmwave-status-before.json
# Run the 25-second preflight or the mmWave session here.
curl -fsS http://127.0.0.1:8080/api/v1/mmwave/status \
    -o /tmp/mmwave-status-after.json
python3 scripts/audit_mmwave_runtime.py \
    --recording data/mmwave/<SESSION>.mmwave.jsonl \
    --setup /absolute/path/to/sealed-setup.json \
    --status-before /tmp/mmwave-status-before.json \
    --status-after /tmp/mmwave-status-after.json \
    --output /tmp/mmwave-runtime-audit.json
```

The audit fails on a transform or node mismatch, recomputed coordinate error,
accepted out-of-room target, radar reboot, sequence gap, clock discontinuity,
or any server-side rejection/loss/reboot increment in the measured window.

Inspect the setup-bound calibration capture and each unlabelled position
capture:

```bash
sensing-server \
    --position-inspect empty-calibration \
    --position-capture empty.raw-csi.v1.jsonl \
    --position-output empty-inspection.json

sensing-server \
    --position-inspect position \
    --position-capture capture-01.raw-csi.v1.jsonl \
    --position-output position-inspection.json
```

Build an index from a typed training manifest, predict unlabelled blind
captures, and only then compare the predictions with a separate truth
manifest:

```bash
sensing-server \
    --position-build-index training-manifest.json \
    --position-output position-index.json

sensing-server \
    --position-predict position-index.json \
    --position-capture blind-01.raw-csi.v1.jsonl \
    --position-output blind-predictions.json

sensing-server \
    --position-evaluate blind-predictions.json \
    --position-truth blind-truth.json \
    --position-output evaluation.json
```

The mmWave-guided blind path additionally freezes four diagnostic WiFi-only
receiver ablations (`RX1` through `RX4`) inside every prediction artifact
before radar truth is attached. Its evaluation reports global fused metrics
and per-RX nearest-prototype accuracy/error separately. Per-RX ablations are
diagnostics only; the four-receiver fused decision remains the deployment gate.
From the repository root, `bash scripts/test_mmwave_synthetic_pipeline.sh`
replays this boundary through raw RX1-RX4 CSI ingestion, a persisted WiFi-only
index, stable radar gating, prediction freeze, and the later truth attachment.

Create Classification predictions without truth from the same 65-second
calibration, three empty checks, and all 18 occupied blind captures:

```bash
sensing-server \
    --replay-calibration empty.raw-csi.v1.jsonl \
    --replay-measurement empty-check-01.raw-csi.v1.jsonl \
    --replay-measurement empty-check-02.raw-csi.v1.jsonl \
    --replay-measurement empty-check-03.raw-csi.v1.jsonl \
    --replay-measurement neutral-01.raw-csi.v1.jsonl \
    --replay-measurement neutral-02.raw-csi.v1.jsonl \
    --replay-report classification-predictions.json
```

Supply all remaining neutral captures the same way. The report contains
capture identities and per-second predictions but no truth. Generate the
private mode-`0600` truth template, fill its 3 empty and 18 occupied rows, and
evaluate only after prediction is frozen:

```bash
python3 scripts/build_classification_truth_template.py \
    classification-predictions.json classification-truth.json

sensing-server \
    --classification-evaluate classification-predictions.json \
    --classification-truth classification-truth.json \
    --classification-output classification-report.json

sensing-server \
    --experiment-classification-report classification-report.json \
    --experiment-position-report evaluation.json \
    --experiment-output experiment-report.json
```

A capture has confirmed presence when at least one post-settling D6 evaluation
second is positive; each positive second is already temporally confirmed by
D6. This capture-level gate cannot hide intermittent detection because the
separate occupied-recall gate still requires at least 80 percent positive
seconds across all occupied runs. The final experiment verdict is `PASS` only
when both component reports pass and name the same sealed setup.

Protocol invariants:

- one empty-room calibration capture covering at least 65 seconds
- one capture per training/blind run covering at least 35 seconds
- the first five seconds are settling time
- four receivers, at least 5 Hz, and a stable CSI grid are required
- raw capture files must not contain point labels or expected positions
- training, calibration, and blind captures must have distinct raw,
  metadata, and protocol-signal identities
- prediction cannot receive or read the truth manifest

After a real blind evaluation passes the predeclared acceptance gates, start
the live server with the exact sealed setup, index bytes, and index SHA-256:

```bash
sensing-server \
    --position-setup sealed-setup.json \
    --position-index position-index.json \
    --position-index-sha256 <64-lowercase-hex>
```

The index and hash flags are an inseparable pair and require the sealed setup.
An index/setup mismatch, malformed hash, invalid receiver/grid input, stale
raw CSI, or insufficient D6 evidence fails closed without coordinates.
With a sealed position setup active, Classification also stays fail-closed as
`uncalibrated` or `calibrating` until the setup-bound D6 reference is ready.
Legacy D4 fallback remains available only for ordinary sessions without a
position setup. Both the server and UI require the exact point IDs P01-P09.

Observatory treats transport and evidence separately. It shows `LIVE ESP32`
only for a fresh explicit `source: "esp32"` frame, becomes stale after three
seconds without another frame, requires valid room/TX/exact RX1-RX4 geometry,
and renders hardware only as a neutral marker after the same presence and
discrete-position gates. Procedural skeletons and the fixed demo room remain
simulation-only; the CSI field is labelled diagnostic rather than a measured
person position.

Automated status (2026-08-01): a file-based synthetic test passes the complete
pipeline with one 65-second empty capture, nine 35-second training captures,
and nine distinct 35-second blind captures (`9/9` correct). Targeted regression
suites for source binding, rejection paths, raw parsing, setup, capture, live
position, offline inspect/build/predict, the guarded runner, and the Sensing and
Observatory UI pass. The earlier four-package Rust matrix passed 1,118 tests
with 0 failures and 3 intentionally ignored: sensing server 885/2, hardware
177/1, CLI 33/0, and pointcloud 23/0 (passed/ignored). Separate firmware host,
provisioning, runner, and UI contract tests also pass and are not added to that
Rust total. The debug binary, actual CLI help/error paths, targeted formatting
checks for the edited Rust modules, source diff checks, JavaScript syntax
checks, and Sensing localization UI regression test also pass. After the final
classification/position protocol gaps were closed, the complete server binary
suite passed `394/394`, the guarded runner plus private truth generator passed
`18/18`, and the public setup/training templates passed their real Rust schema
tests.

ESP-IDF 5.4 is installed locally. Current firmware 0.7.0 target builds passed
for ESP32-S3 8 MB, ESP32-S3 4 MB, and ESP32-C6. The four physical RX were each
inventoried, flashed with the verified S3 image, hash-checked, and booted with
their preserved node IDs and TX filter. The separate TX firmware was left
unchanged; read-only inventory, stable SoftAP boot, DHCP/Gateway, and observed
32-byte broadcast traffic passed. Common 1TX/4RX discovery and the sealed
preflight remain required before real captures.

This proves the software path, not fixed-room RF accuracy. The live
WebSocket/Sensing path can load a setup-bound validated index, but no real
nine-point index has passed blind validation or been enabled. Until then the
runtime remains uncalibrated and emits no person coordinates. The existing
geometry heatmap is explicitly diagnostic and must not be interpreted as this
fingerprint model's measured person position.

### mmWave-assisted D6 position calibration

The optional HLK-LD2450 node is a temporary teacher and held-back reference.
It never becomes an input to live position prediction. The server listens for
strict `ruview.mmwave.ld2450.v1` packets on UDP 5010 by default, aligns each
accepted radar sample with fresh CSI from RX1-RX4, and guides this sequence in
the Sensing tab:

1. 65 seconds of empty-room CSI with no radar target.
2. One walk through all accessible room areas to select nine reproducible
   25 cm coverage zones automatically.
3. Six independent five-second still blocks per zone.
4. Two new blind visits per zone, with the WiFi prediction frozen before radar
   truth is attached. Metric error uses the median measured radar position from
   each five-second visit, not the nominal zone centre.

Start the server with the node control URL and keep the bearer token only in an
environment variable:

```bash
export MMWAVE_NODE_TOKEN='<node bearer token>'
cargo run -p wifi-densepose-sensing-server --bin sensing-server -- \
  --mmwave-node-url http://192.0.2.60 \
  --mmwave-token-env MMWAVE_NODE_TOKEN \
  --position-setup ../data/position-setup-v2.json
```

The setup must use schema version 2 and seal the node ID, firmware artifact,
mounting revision, and room-coordinate transform. Transform changes are
rejected after sealing or while a session is active. The generated position
index stores the setup hash, full radar-recording hash, CSI-grid identities,
54 training-block hashes, empty reference, and the WiFi-only fingerprint
model. Artifacts are created without overwriting existing files.

The blind run passes only with exactly 18 visits, at least 16 decided, at least
15 correct, at least 90% decided accuracy, no more than two abstentions, median
error at most 0.75 m, and maximum error at most 1.30 m. Trajectory coverage is
reported as a diagnostic and is not an acceptance gate. A model that fails any
gate is not approved for live use.

The freshly trained index is installed only as a private blind-test candidate.
Public Sensing output remains `uncalibrated` until the blind report is `PASS`;
after `FAIL` it remains locked. Radar coordinates are never passed into the
WiFi predictor.

### Using as a library

```rust
use wifi_densepose_sensing_server::vital_signs::VitalSignDetector;

// Create a detector with 20 Hz sample rate
let mut detector = VitalSignDetector::new(20.0);

// Feed CSI amplitude samples
for amplitude in csi_amplitudes.iter() {
    detector.push_sample(*amplitude);
}

// Extract vital signs
if let Some(vitals) = detector.detect() {
    println!("Breathing: {:.1} BPM", vitals.breathing_rate_bpm);
    println!("Heart rate: {:.0} BPM", vitals.heart_rate_bpm);
}
```

## Architecture

```text
ESP32 ──UDP:5005──> [ CSI Receiver ]
                          |
                    [ Signal Pipeline ]
                    (vital_signs, graph_transformer, sona)
                          |
                    [ WebSocket Broadcast ]
                          |
Browser <──WS:8765── [ Axum Server :8080 ] ──> Static UI files
```

## Related Crates

| Crate | Role |
|-------|------|
| [`wifi-densepose-wifiscan`](../wifi-densepose-wifiscan) | Multi-BSSID WiFi scanning (ADR-022) |
| [`wifi-densepose-core`](../wifi-densepose-core) | Shared types and traits |
| [`wifi-densepose-signal`](../wifi-densepose-signal) | CSI signal processing algorithms |
| [`wifi-densepose-hardware`](../wifi-densepose-hardware) | ESP32 hardware interfaces |
| [`wifi-densepose-wasm`](../wifi-densepose-wasm) | Browser WASM bindings for the sensing UI |
| [`wifi-densepose-train`](../wifi-densepose-train) | Full training pipeline with ruvector |
| [`wifi-densepose-mat`](../wifi-densepose-mat) | Disaster detection module |

## License

MIT OR Apache-2.0
