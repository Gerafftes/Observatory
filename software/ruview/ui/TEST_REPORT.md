# WiFi-DensePose UI Test Report

## Scope

This report covers the browser surface and the data it is allowed to present.
The browser is not a source of sensor data: it renders frames and status
received from the sensing server and shows `--` when a value is unavailable.

## Surface review

| Surface | Current behavior | Evidence boundary |
|---|---|---|
| Dashboard | Health, source state, and pose statistics from the API | No CPU, memory, disk, zone, accuracy, body-region, sampling-rate, or cost claims |
| Hardware | Server-reported nodes from `GET /api/v1/nodes` | No illustrative antenna array or synthetic CSI amplitude/phase values |
| Live Demo | Server-backed pose stream and current-session diagnostics | No client-side offline demo, static setup claims, or visible zone placeholder |
| Sensing | Server-backed CSI features, source banner, and explicit mmWave reference view | No client-generated sensing frames; stale readouts are cleared |
| Training | Existing recording/model controls | Defaults and controls are not reported as training results |

Architecture, Performance, and Applications are no longer dashboard tabs.
Possible applications are documented in `README.md` and are explicitly not
product or accuracy claims.

## Source semantics

- `live` means a current frame was received from a real ESP32/WiFi source.
- `server-simulated` is shown only when the server was explicitly started in
  simulation mode.
- `server-offline` and `reconnecting` clear live values and do not emit local
  fallback frames.
- Unknown source values fail closed as offline.
- `auto` listens for real UDP CSI and remains `esp32:offline` until the first
  real frame; synthetic frames require explicit `--source simulated`.

## Validation commands

The current self-check loop produced:

```bash
node --test software/ruview/ui/tests/*.test.mjs
cd software/ruview/v2
cargo test -p wifi-densepose-sensing-server --no-default-features
```

Result: **134 UI tests passed**. The Rust sensing-server suite passed with
**472 library tests and 452 binary tests**; the slow mmWave diagnostics test
also passed when repeated with the required local-socket permission.

The mock server and explicit synthetic-frame fixtures remain available for
automated tests. They are test inputs, not a production fallback path.

## Remaining limits

The browser cannot cold-start a stopped native process or access USB/mDNS
directly. Those operations remain in the loopback Control Helper and are
exposed from the Hardware tab when the helper is running.
