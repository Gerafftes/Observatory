# Observatory software source

`software/ruview/` contains the complete source snapshot used by Observatory:

- the browser UI under `ui/`
- the Rust workspace and sensing backend under `v2/`
- the ESP32 CSI and mmWave firmware under `firmware/`
- the historical Python backend under `archive/v1/`
- scripts, tests, architecture documents and D4/D5/D6 implementation paths
- all eight upstream submodule source trees as ordinary vendored files

A clone of Observatory therefore does not need a second checkout from
`ruvnet/RuView`. Cargo, npm, Python and ESP-IDF may still download normal
package dependencies when they are not already cached.

## Provenance

The parent source snapshot is based on RuView commit
`bbf99e2dc94458e80121d2bacd9a8f229acb5a7c` (`feat: publish RuView calibration
workflow and Observatory docs`). The following former submodules are vendored
at their pinned commits:

| Path | Commit |
|---|---|
| `vendor/midstream` | `8f70d2bb9d64a8ddee7745bc18bc4ff9e151845e` |
| `vendor/ruvector` | `a083bd77fa2f4879595daa68686ed5b2132d981a` |
| `vendor/sublinear-time-solver` | `c25dddf163d8c413628ecdc6e979583d39270f22` |
| `vendor/rvcsi` | `72891d740f92903c78a8208a9069f6c82a4d1cc2` |
| `vendor/rufield` | `509d8ae29e654a322910bd504d325b0dd1fdd895` |
| `v2/crates/ruv-neural` | `c9638faaf8ae1d910039171be487a465a5762313` |
| `v2/crates/ruview-swarm` | `267aba5be2288aa6cbe574492062b04fa8c8a6ce` |
| `v2/crates/worldgraph` | `fdade422069d3162634292710d78cb9963c0f48d` |

The snapshot also contains the current Observatory working-tree changes in:

- `ui/components/MmwaveCalibrationAssistant.js`
- `ui/components/ObservatoryControlCenter.js`
- `ui/components/RoomGeometryEditor.js`
- `ui/components/SensingTab.js`
- `ui/index.html`
- `ui/style.css`
- `ui/tests/`
- `ui/utils/i18n.js`
- `v2/crates/wifi-densepose-sensing-server/src/experiment.rs`

Git histories, build directories, dependency caches, recordings, logs,
credentials and device-specific provisioning exports are intentionally not
part of the snapshot.

## Build and run

From the Observatory repository root:

```bash
cd software/ruview/v2
cargo check -p wifi-densepose-sensing-server --no-default-features
cargo run -p wifi-densepose-sensing-server --no-default-features -- \
  --source simulate \
  --http-port 3002 \
  --ws-port 3001
```

Then open `http://127.0.0.1:3002/ui/index.html#sensing`.

Simulation and software tests do not prove real CSI, mmWave operation or
position accuracy. Hardware results remain subject to the setup, preflight,
calibration and blind-validation gates documented in the repository root.

## Runtime and API ownership

The executable sensing server is bootstrapped in
`ruview/v2/crates/wifi-densepose-sensing-server/src/main.rs`. Browser-facing
route composition lives in `src/routes.rs`; handlers and their contract tests
are grouped by responsibility in `src/observatory_routes.rs`,
`src/mmwave_routes.rs`, `src/calibration_routes.rs`, `src/sensing_routes.rs`,
`src/system_routes.rs`, `src/model_routes.rs`, `src/recording_routes.rs`, and
`src/training_routes.rs`. Shared route validation is limited to
`src/route_support.rs`. The single runtime state contract lives in
`src/state.rs`, wire-format parsers in `src/protocol.rs`, and background task
implementations in `src/runtime_tasks.rs`. Reusable server modules are exported
by `src/lib.rs`. Files outside the declared binary or library module graphs are
retained source history, not executable API implementations.

The browser's `ui/config/api.config.js` exposes only registered active paths
through `API_CONFIG.ENDPOINTS`. Unregistered compatibility paths are kept in
the separate `LEGACY_ENDPOINTS` export so existing service methods remain
source-compatible without advertising backend support.

All `/api/v1/*` requests pass the HTTP router's bearer and Host boundaries;
state-changing browser requests also pass the Origin boundary. Live WebSocket
routes pass bearer, Host, and Origin checks. `/health/*` stays public apart from
the Host boundary. The response column below records the current executable
contract, including legacy `200` responses whose JSON body carries
`success: false`.

| Method and surface | Browser consumer | Handler and state | Response contract | Status and evidence |
|---|---|---|---|---|
| `GET /health/{health,live,ready,version,metrics}` | `health.service.js` | health handlers; `SharedState` where needed | `200` health JSON | `supported`; server health tests |
| `GET /api/v1/{info,status,metrics}` | `health.service.js` | `routes.rs` → `api_info`, `health_ready`, `health_metrics`; `SharedState` | `200` discovery/readiness JSON | `supported`; server contract tests |
| `GET /api/v1/sensing/latest`; `GET /api/v1/pose/{current,stats,zones/summary}` | sensing and pose services | `sensing_routes`; `SharedState` | `200` current WiFi-derived sensing/pose JSON | `supported`; UI service and server tests |
| `GET /api/v1/stream/status` | `streamService.getStatus`; no active component caller | `sensing_routes::stream_status`; `SharedState` | `200` stream status JSON | `supported`, currently UI-unused |
| `WS /api/v1/stream/pose`; `WS /ws/{sensing,introspection}` | pose, sensing and introspection consumers | `sensing_routes`; `SharedState` | WebSocket upgrade or guarded rejection | `supported`; WebSocket/auth tests |
| `GET /api/v1/introspection/snapshot` | introspection UI | `sensing_routes::api_introspection_snapshot`; `SharedState` | `200` bounded runtime snapshot | `supported`; server contract tests |
| `GET /api/v1/models` | `modelService`, `ModelPanel`, `LiveDemoTab`, Control Center | `model_routes::list_models`; `SharedState` | `200 {models,total}` | `supported`; model contract tests |
| `GET /api/v1/models/:id` | `modelService.getModel`, `LiveDemoTab` | `model_routes::get_model`; filesystem scan, no mutable state | `200` list-compatible metadata; `400` unsafe ID; `404` unknown ID | `supported`; model detail tests |
| `GET /api/v1/models/active`; `POST /load`, `/unload` | `modelService`, `ModelPanel`, `LiveDemoTab` | `model_routes`; `SharedState` | active metadata or mutation JSON; validation failures currently remain JSON responses | `supported`; model/UI tests |
| `DELETE /api/v1/models/:id`; `GET /lora/profiles`; `POST /lora/activate` | `modelService`, `ModelPanel`, `LiveDemoTab` | `model_routes`; filesystem and `SharedState` where needed | success/error JSON; internal delete errors use protected `500` response | `supported`; path and model tests |
| `GET /api/v1/recording/list` | `trainingService`, Control Center | `recording_routes::list_recordings`; `SharedState` plus filesystem | `200 {recordings,total}` with format and integrity fields | `supported for capture`; raw lifecycle tests |
| `POST /api/v1/recording/{start,stop}` | `TrainingPanel`, `LiveDemoTab`, Control Center | `recording_routes`; `SharedState`, raw writer lifecycle | `200` JSON with `success`; incomplete/error state never becomes completed | `supported for capture`; raw lifecycle tests |
| `DELETE /api/v1/recording/:id` | `TrainingPanel` | `recording_routes::delete_recording`; `SharedState` plus filesystem | `200` success/error JSON; active/finalizing capture is rejected | `supported`; ID and lifecycle tests |
| `/api/v1/experiments/*`, setup profiles, workflows, reports and exports | `experiment.service.js`, Control Center | `observatory_routes`; `SharedState` and experiment store | endpoint-specific `2xx` or guarded `4xx/5xx` response | `supported`; workflow and experiment tests |
| `GET /api/v1/control-center/status`, `/benchmarks/catalog` | `experiment.service.js`, Control Center | `observatory_routes`; read-only summaries | `200` status/catalog JSON | `supported`; server/UI tests |
| `/api/v1/mmwave/{status,mode,transform,session/*}` | mmWave assistant and Control Center | `mmwave_routes`; `SharedState`, calibration context and session state | endpoint-specific status or guarded error | `supported contract`; hardware transport remains separately evidenced |
| `/api/v1/adaptive/*`; `/api/v1/classification/calibration/*`; `/api/v1/calibration/*` | Control Center and calibration UI | `calibration_routes`; `SharedState` and persisted calibration bundles | endpoint-specific status or guarded error | `supported, software contract only`; calibration tests |
| `/api/v1/{nodes,mesh,vital-signs,edge-vitals,edge/registry,wasm-events}` | sensing diagnostics and Control Center | `system_routes`; read-only `SharedState` summaries | `200` diagnostics JSON or endpoint-specific guarded error | `supported`; system contract tests |
| `/api/v1/model/{info,layers,segments,sona/*}`; `/api/v1/config/*` | model/configuration UI | `system_routes`; model metadata and runtime configuration | endpoint-specific JSON with existing validation | `supported`; server contract tests |
| `GET /api/v1/train/status` | `TrainingPanel` | `training_routes::train_status`; no training state | `200`, `active:false`, `status:"unavailable"`, every capability `false` | `unavailable`; Rust and UI contract tests |
| `POST /api/v1/train/{start,stop,pretrain,lora}`; `GET /ws/train/progress` | `training.service.js`, capability-gated by `TrainingPanel` | `training_routes::train_unavailable` | `501` with the same unavailable payload; no task or WebSocket starts | `unavailable`; route and no-start UI tests |
| Legacy pose analyze, per-zone occupancy, historical, activities and pose-calibration paths | retained methods in `pose.service.js` | no active Rust handler | normal route-not-found response if called | `legacy`; `LEGACY_ENDPOINTS` catalogue test |
| Legacy stream start/stop/client/broadcast/metrics/events paths | retained `stream.service.js` methods and event-stream compatibility method | no active Rust handler | normal route-not-found response if called | `legacy`; `LEGACY_ENDPOINTS` catalogue test |
| Legacy `/api/v1/dev/{config,reset}` | no active component caller | no active Rust handler | normal route-not-found response if called | `legacy`; `LEGACY_ENDPOINTS` catalogue test |
| `MOCK_SERVER` configuration | UI tests only when explicitly enabled | no production handler | local mock behavior only | `test/mock-only` |

The current raw capture format is `raw-csi-v1-jsonl` and is not automatically
trainable. `src/training_api.rs` and `src/model_manager.rs` remain unregistered:
their state, recording, and metric assumptions differ from the active server.
Activating them requires a separately approved typed raw-CSI/label migration,
atomic RVF export, explicit progress/error contracts, and synthetic validation.
mmWave may supply independent teacher/ground-truth labels but never WiFi model
features. The separate CLI training modes are unaffected by this browser API
contract.

## Licenses

The vendored source keeps its original `LICENSE`, `LICENSE.md`, `NOTICE` and
component-level license files. Those files govern their respective source
trees; the Observatory root license does not replace third-party notices.
