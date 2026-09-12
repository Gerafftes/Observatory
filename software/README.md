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

The executable sensing server is composed in
`ruview/v2/crates/wifi-densepose-sensing-server/src/main.rs`. Its small
read-only observability route group lives in `src/routes.rs`; reusable server
modules are exported by `src/lib.rs`. Files that are not declared by either
module graph are retained source history, not executable API implementations.

The browser's `ui/config/api.config.js` is a compatibility catalogue. A path
listed there is not automatically a supported backend capability. The active
UI/server contract is:

| Surface | Runtime status | Contract |
|---|---|---|
| `/api/v1/info`, `/status`, `/metrics` | supported | Read-only discovery and health routes composed in `src/routes.rs`. |
| `/api/v1/models` and `/api/v1/models/:id` | supported | Lists models and returns one model's discovered RVF metadata. |
| Recording routes | supported for capture | Current files use `raw-csi-v1-jsonl`; they remain offline evidence unless a compatible training pipeline is validated. |
| `/api/v1/train/*` and `/ws/train/progress` | explicit unavailable contract | Status returns capability flags; HTTP start, stop, pretrain, LoRA, and progress-stream requests do not claim work that is not executed. The separate CLI training modes are unaffected. |
| Other unused pose, stream, and dev catalogue paths | compatibility only | They have no active browser call site and are not registered by the Rust server. |

`src/training_api.rs` and `src/model_manager.rs` are not registered wholesale:
their state and recording assumptions differ from the active server. Reusing
them requires an explicit contract migration and validation, not only adding a
route declaration.

## Licenses

The vendored source keeps its original `LICENSE`, `LICENSE.md`, `NOTICE` and
component-level license files. Those files govern their respective source
trees; the Observatory root license does not replace third-party notices.
