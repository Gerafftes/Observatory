# WiFi DensePose UI

A modular, modern web interface for the WiFi DensePose human tracking system. Provides real-time monitoring, WiFi sensing visualization, and pose estimation from CSI (Channel State Information).

## Architecture

The UI follows a modular architecture with clear separation of concerns:

```
ui/
├── app.js                    # Main application entry point
├── index.html                # HTML shell with tab structure
├── style.css                 # Complete CSS design system
├── config/
│   └── api.config.js         # API endpoints and configuration
├── services/
│   ├── api.service.js        # HTTP API client
│   ├── websocket.service.js  # WebSocket connection manager
│   ├── websocket-client.js   # Low-level WebSocket client
│   ├── pose.service.js       # Pose estimation API wrapper
│   ├── sensing.service.js    # WiFi sensing data service (server frames only)
│   ├── health.service.js     # Health monitoring API wrapper
│   ├── stream.service.js     # Streaming API wrapper
│   ├── server-control.service.js # Browser client for the loopback server helper
│   ├── control-helper.service.js # Browser client for local hardware control
│   └── data-processor.js     # Signal data processing utilities
├── components/
│   ├── TabManager.js         # Tab navigation component
│   ├── DashboardTab.js       # Dashboard with health and live pose statistics
│   ├── SensingTab.js         # WiFi sensing visualization (3D signal field, metrics)
│   ├── LiveDemoTab.js        # Live pose detection and session diagnostics
│   ├── HardwareTab.js        # Reported node data and local control entrypoint
│   ├── LocalControlPanel.js   # Browser discovery and serial-port controls
│   ├── ServerControlPanel.js  # Browser start/stop/status controls
│   ├── SettingsPanel.js      # Settings panel
│   ├── PoseDetectionCanvas.js # Canvas-based pose skeleton renderer
│   ├── gaussian-splats.js    # 3D Gaussian splat signal field renderer (Three.js)
│   ├── body-model.js         # 3D body model
│   ├── scene.js              # Three.js scene management
│   ├── signal-viz.js         # Signal visualization utilities
│   ├── environment.js        # Environment/room visualization
│   └── dashboard-hud.js      # Dashboard heads-up display
├── utils/
│   ├── backend-detector.js   # Auto-detect backend availability
│   ├── mock-server.js        # Mock server for testing
│   └── pose-renderer.js      # Pose rendering utilities
└── tests/
    ├── test-runner.html       # Test runner UI
    ├── test-runner.js         # Test framework and cases
    └── integration-test.html  # Integration testing page
```

## Features

### WiFi Sensing Tab
- 3D Gaussian-splat signal field visualization (Three.js)
- Real-time RSSI, variance, motion band, breathing band metrics
- Presence/motion classification with confidence scores
- **Data source banner**: green "LIVE - ESP32", yellow "RECONNECTING...", red "WIFI/CSI OFFLINE", or an explicit orange server simulation label
- Sparkline RSSI history graph
- Empty values remain `--` until a current server frame provides them

### Live Demo Tab
- WebSocket-based real-time pose skeleton rendering
- **Estimation Mode badge**: green "Signal-Derived" or blue "Model Inference"
- Debug mode with log export
- Session diagnostics (frames, uptime, errors) for the current browser session
- Only the explicit server data source is shown; the browser has no offline demo button

### Dashboard
- Live system health monitoring
- Real-time pose detection statistics
- API status indicators
- Values are populated from the active API/WebSocket; unavailable values remain empty

### Hardware
- Node rows sourced from `GET /api/v1/nodes` (status, RSSI, frame rate, packet loss, and last-seen age)
- Loopback Control Helper entrypoint for local node discovery and serial-port listing
- No illustrative antenna array or synthetic CSI amplitude/phase values

## Possible Applications

These are documented directions for the project, not validated product
capabilities. They require a measured setup, appropriate consent and privacy
review, and application-specific validation before use:

- Room-level presence and movement monitoring
- Research on privacy-preserving WiFi sensing
- Smart-building occupancy experiments
- Assistive interfaces based on coarse movement signals
- AR/VR or robotics research with a trained, validated pose model

The current browser UI does not claim medical monitoring, emergency detection,
security surveillance, full-body tracking, or production accuracy.

## Data Sources

The sensing service (`sensing.service.js`) distinguishes the following states:

| State | Banner Color | Description |
|-------|-------------|-------------|
| **LIVE - ESP32** | Green | Connected to the Rust sensing server receiving real CSI data |
| **RECONNECTING** | Yellow (pulsing) | WebSocket disconnected, retrying (up to 20 attempts) |
| **WIFI/CSI OFFLINE** | Red | Server is reachable, but no fresh ESP32 frame is available |
| **SERVER SIMULATION** | Orange | The server was explicitly started with `--source simulated` |

The browser never generates fallback frames. If the server is unavailable or
the source is unknown, the UI clears live readouts and keeps retrying. Synthetic
frames are available only when the server is explicitly run in simulation mode.

## Backends

### Rust Sensing Server (primary)
The Rust-based `wifi-densepose-sensing-server` serves the UI and provides:
- `GET /health` — server health
- `GET /api/v1/sensing/latest` — latest sensing features
- `GET /api/v1/vital-signs` — vital sign estimates (HR/RR)
- `GET /api/v1/model/info` — RVF model container info
- `WS /ws/sensing` — real-time sensing data stream
- `WS /api/v1/stream/pose` — real-time pose keypoint stream

### Python FastAPI (legacy)
The original Python backend on port 8000 is still supported. The UI auto-detects which backend is available via `backend-detector.js`.

## Quick Start

### Observatory Control Center

Der vollständige Ablauf für Setup, Software-only-Demo, mmWave-Kalibrierung,
Blindtest und Hardware-Abnahme steht in
[`observatory/EXPERIMENT_SETUP_GUIDE.md`](observatory/EXPERIMENT_SETUP_GUIDE.md).

### With Docker (recommended)
```bash
cd docker/

# Default: auto-detects the available source; with no live source it stays offline
docker-compose up

# Force real ESP32 data
CSI_SOURCE=esp32 docker-compose up

# Force simulation (no hardware needed)
CSI_SOURCE=simulated docker-compose up
```
Open http://localhost:3000/ui/index.html

### With local Rust binary
```bash
cd v2
cargo build -p wifi-densepose-sensing-server --no-default-features

# Run with simulated data
target/debug/sensing-server --source simulated --tick-ms 100 --ui-path ../ui --http-port 3000

# Run with real ESP32
target/debug/sensing-server --source esp32 --tick-ms 100 --ui-path ../ui --http-port 3000
```
Open http://localhost:3000/ui/index.html

### Browser plus local Control Helper

The active application surface is the browser. Native operations that a normal
webpage cannot perform (mDNS/UDP discovery and USB serial access) live in the
small loopback-only `ruview-control` helper:

```bash
cd v2
cargo build -p wifi-densepose-control --no-default-features
```

When the helper binary is placed beside `sensing-server`, the sensing server
starts it automatically for the browser control surface. It listens only on
`127.0.0.1:8090` and accepts mutating requests only from the local UI origin.
The browser's Hardware tab then exposes `Nodes suchen` and `Ports laden`.

The helper cannot be cold-started by a webpage when both processes are stopped;
that is an operating-system permission boundary. Start the sensing server once
(or register `ruview-control` as a user service) and subsequent control stays
in the browser.

### With Python HTTP server (legacy)
```bash
# Start FastAPI backend on port 8000
wifi-densepose start

# Serve the UI on port 3000
cd ui/
python -m http.server 3000
```
Open http://localhost:3000

## Pose Estimation Modes

| Mode | Badge | Condition | What the UI can verify |
|------|-------|-------------|----------|
| **Signal-Derived** | Green | Current CSI frame with aggregate features | Signal-derived output; no accuracy claim |
| **Model Inference** | Blue | Loaded `.rvf` model with model output in the frame | Model-produced keypoints are present |

To use model inference, start the server with a trained model:
```bash
sensing-server --source esp32 --model path/to/model.rvf --ui-path ./ui
```

## Configuration

### API Configuration
Edit `config/api.config.js`:

```javascript
export const API_CONFIG = {
  BASE_URL: window.location.origin,
  API_VERSION: '/api/v1',
  WS_CONFIG: {
    RECONNECT_DELAY: 5000,
    MAX_RECONNECT_ATTEMPTS: 20,
    PING_INTERVAL: 30000
  }
};
```

## Testing

Open `tests/test-runner.html` to run the test suite:

```bash
cd ui/
python -m http.server 3000
# Open http://localhost:3000/tests/test-runner.html
```

Test categories: API configuration, API service, WebSocket, pose service, health service, UI components, integration.

## Styling

Uses a CSS design system with custom properties, dark/light mode, responsive layout, and component-based styling. Key variables in `:root` of `style.css`.

## License

Part of the WiFi-DensePose system. See the main project LICENSE file.
