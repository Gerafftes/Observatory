//! WiFi-DensePose Sensing Server
//!
//! Lightweight Axum server that:
//! - Receives ESP32 CSI frames via UDP (port 5005)
//! - Processes signals using RuVector-powered wifi-densepose-signal crate
//! - Broadcasts sensing updates via WebSocket (ws://localhost:8765/ws/sensing)
//! - Serves the static UI files (port 8080)
//!
//! Replaces both ws_server.py and the Python HTTP server.
#![allow(dead_code)]

mod adaptive_classifier;
mod benchmark;
mod calibration_dataset;
mod calibration_persistence;
mod calibration_routes;
mod classification_evaluation;
mod coarse_localization;
#[cfg(test)]
pub mod csi;
mod d5_presence;
mod d6_fingerprint;
mod engine_bridge;
mod experiment;
mod experiment_evaluation;
mod field_bridge;
mod field_localize;
mod mmwave_calibration;
mod mmwave_connection;
mod mmwave_discovery;
mod mmwave_position_index;
mod mmwave_routes;
mod model_routes;
mod multistatic_bridge;
mod observatory_routes;
#[cfg(test)]
pub mod pose;
mod position_artifact;
mod position_capture;
mod position_evaluation;
mod position_fingerprint;
mod position_live;
mod position_offline;
mod position_setup;
mod protocol;
mod raw_csi_recording;
mod raw_csi_replay;
mod recording_routes;
mod route_support;
mod routes;
mod runtime_tasks;
mod sensing_routes;
mod server_clock;
mod server_control;
mod state;
mod system_routes;
mod tracker_bridge;
mod training_routes;
#[cfg(test)]
pub mod types;

// Training pipeline modules (exposed via lib.rs)
#[cfg(test)]
use wifi_densepose_sensing_server::rvf_container;
use wifi_densepose_sensing_server::{
    dataset, embedding, error_response, graph_transformer, model_format, rufield_surface,
    rvf_container::{RvfBuilder, RvfContainerInfo, RvfReader, VitalSignConfig},
    rvf_pipeline::{self, ProgressiveLoader},
    torso, trainer,
    vital_signs::{self, VitalSignDetector, VitalSigns},
};

use ruvector_mincut::{DynamicMinCut, MinCutBuilder};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::{Path as FilePath, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post, put},
    Extension, Router,
};
use clap::{Parser, ValueEnum};

use axum::http::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc, Mutex, Notify, RwLock};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{debug, error, info, warn};

#[cfg(test)]
use calibration_routes::classification_decision_status;
use protocol::{
    esp32_csi_header_rx_id, has_esp32_csi_magic, parse_edge_fused_vitals, parse_esp32_frame,
    parse_esp32_vitals, parse_wasm_output, Esp32Frame, Esp32VitalsPacket, WasmOutputPacket,
};
use runtime_tasks::{
    broadcast_tick_task, mmwave_receiver_task, probe_esp32, probe_windows_wifi,
    settle_recording_on_shutdown, simulated_data_task, spawn_mmwave_node_diagnostics_poller,
    spawn_mmwave_session_ticker, udp_receiver_task, windows_wifi_task,
};
use state::*;
#[cfg(test)]
use system_routes::{
    bool_metric, fleet_role_counts, public_node_summaries, source_binding_consistent_across_nodes,
};

// ADR-022 Phase 3: Multi-BSSID pipeline integration
use wifi_densepose_wifiscan::parse_netsh_output as parse_netsh_bssid_output;
use wifi_densepose_wifiscan::{BssidRegistry, WindowsWifiPipeline};

// Accuracy sprint: Kalman tracker, multistatic fusion, field model
use wifi_densepose_signal::ruvsense::field_model::{CalibrationStatus, FieldModel};
use wifi_densepose_signal::ruvsense::multistatic::{MultistaticConfig, MultistaticFuser};
use wifi_densepose_signal::ruvsense::pose_tracker::PoseTracker;

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PositionInspectionProtocolArg {
    EmptyCalibration,
    Position,
}

#[derive(Parser, Debug)]
#[command(name = "sensing-server", about = "WiFi-DensePose sensing server")]
struct Args {
    /// HTTP port for UI and REST API
    #[arg(long, default_value = "8080")]
    http_port: u16,

    /// WebSocket port for sensing stream
    #[arg(long, default_value = "8765")]
    ws_port: u16,

    /// UDP port for ESP32 CSI frames
    #[arg(long, default_value = "5005")]
    udp_port: u16,

    /// UDP port for HLK-LD2450 packets from the ESP32-C3 node.
    #[arg(long, default_value_t = mmwave_calibration::DEFAULT_UDP_PORT)]
    mmwave_udp_port: u16,

    /// Kernel receive buffer requested for the mmWave UDP socket.
    #[arg(long, default_value_t = runtime_tasks::DEFAULT_MMWAVE_RECEIVE_BUFFER_BYTES, env = "MMWAVE_UDP_RECEIVE_BUFFER_BYTES")]
    mmwave_receive_buffer_bytes: usize,

    /// Maximum hold time used to repair short UDP reordering bursts.
    #[arg(long, default_value_t = runtime_tasks::DEFAULT_MMWAVE_REORDER_HOLD_MS, env = "MMWAVE_REORDER_HOLD_MS")]
    mmwave_reorder_hold_ms: u64,

    /// Base URL of the ESP32-C3 control server, for example http://192.0.2.60:8032.
    #[arg(long, env = "MMWAVE_NODE_URL")]
    mmwave_node_url: Option<String>,

    /// Environment variable that contains the ESP32-C3 bearer token.
    #[arg(long, default_value = "MMWAVE_NODE_TOKEN")]
    mmwave_token_env: String,

    /// Path to UI static files (repo `ui/`; from `v2/` use `../ui` or rely on auto-detect)
    #[arg(long, default_value = "../ui")]
    ui_path: PathBuf,

    /// Tick interval in milliseconds (default 100 ms = 10 fps for smooth pose animation)
    #[arg(long, default_value = "100")]
    tick_ms: u64,

    /// Bind address (default 127.0.0.1; set to 0.0.0.0 for network access)
    #[arg(long, default_value = "127.0.0.1", env = "SENSING_BIND_ADDR")]
    bind_addr: String,

    /// Additional hostname (with or without `:PORT`) to permit in the `Host`
    /// header — defends loopback-bound deployments against DNS rebinding.
    /// Loopback names (`localhost`, `127.0.0.1`, `[::1]`) are always permitted
    /// implicitly. Pass multiple times to add several entries. Comma-separated
    /// values are also accepted via the `SENSING_ALLOWED_HOSTS` env var.
    #[arg(long = "allowed-host", value_name = "HOST")]
    allowed_hosts: Vec<String>,

    /// Explicit browser Origin (scheme://host[:port]) allowed to open live
    /// WebSockets or submit state-changing API requests. Pass multiple times;
    /// comma-separated values are also accepted via `SENSING_ALLOWED_ORIGINS`.
    /// When omitted, only the local UI origins on `--http-port` are allowed.
    #[arg(long = "allowed-origin", value_name = "ORIGIN")]
    allowed_origins: Vec<String>,

    /// Disable Host-header validation entirely. Use only when the server sits
    /// behind a reverse proxy that already canonicalises `Host` (e.g. nginx
    /// `proxy_set_header Host`) — bare deployments stay vulnerable to DNS
    /// rebinding without it.
    #[arg(long)]
    disable_host_validation: bool,

    /// MQTT publisher (HA auto-discovery) + privacy-mode flags (ADR-115).
    /// Flattened so `--mqtt*` reach the binary's parser and the publisher
    /// in `mqtt::` is actually started (fixes #872). Uses the *lib* crate's
    /// `MqttArgs` type so it's compatible with `mqtt::config::from_args`.
    #[command(flatten)]
    mqtt_opts: wifi_densepose_sensing_server::cli::MqttArgs,

    /// Data source: auto, wifi, esp32, simulate
    #[arg(long, default_value = "auto")]
    source: String,

    /// Run vital sign detection benchmark (1000 frames) and exit
    #[arg(long)]
    benchmark: bool,

    /// Load model config from an RVF container at startup
    #[arg(long, value_name = "PATH")]
    load_rvf: Option<PathBuf>,

    /// Save current model state as an RVF container on shutdown
    #[arg(long, value_name = "PATH")]
    save_rvf: Option<PathBuf>,

    /// Load a trained .rvf model for inference
    #[arg(long, value_name = "PATH")]
    model: Option<PathBuf>,

    /// Enable progressive loading (Layer A instant start)
    #[arg(long)]
    progressive: bool,

    /// Export an RVF container package and exit (no server)
    #[arg(long, value_name = "PATH")]
    export_rvf: Option<PathBuf>,

    /// Convert a published model file (model.safetensors / model.rvf.jsonl) to
    /// the RVF binary container the --model loader expects, then exit (#894).
    /// Pair with --convert-out for the destination path.
    #[arg(long, value_name = "PATH")]
    convert_model: Option<PathBuf>,

    /// Output path for --convert-model (defaults to <input>.rvf).
    #[arg(long, value_name = "PATH")]
    convert_out: Option<PathBuf>,

    /// Run training mode (train a model and exit)
    #[arg(long)]
    train: bool,

    /// Path to dataset directory (MM-Fi or Wi-Pose)
    #[arg(long, value_name = "PATH")]
    dataset: Option<PathBuf>,

    /// Dataset type: "mmfi" or "wipose"
    #[arg(long, value_name = "TYPE", default_value = "mmfi")]
    dataset_type: String,

    /// Number of training epochs
    #[arg(long, default_value = "100")]
    epochs: usize,

    /// Directory for training checkpoints
    #[arg(long, value_name = "DIR")]
    checkpoint_dir: Option<PathBuf>,

    /// Run self-supervised contrastive pretraining (ADR-024)
    #[arg(long)]
    pretrain: bool,

    /// Number of pretraining epochs (default 50)
    #[arg(long, default_value = "50")]
    pretrain_epochs: usize,

    /// Extract embeddings mode: load model and extract CSI embeddings
    #[arg(long)]
    embed: bool,

    /// Build fingerprint index from embeddings (env|activity|temporal|person)
    #[arg(long, value_name = "TYPE")]
    build_index: Option<String>,

    /// Node positions for multistatic fusion (format: "x,y,z;x,y,z;...")
    #[arg(long, env = "SENSING_NODE_POSITIONS")]
    node_positions: Option<String>,

    /// Transmitter position for visualization (format: "x,y,z")
    #[arg(long, env = "SENSING_TX_POSITION")]
    tx_position: Option<String>,

    /// Room dimensions for visualization (format: "length,height,width")
    #[arg(long, env = "SENSING_ROOM_DIMENSIONS")]
    room_dimensions: Option<String>,

    /// Completed empty-room raw CSI capture used to calibrate an offline replay.
    #[arg(long, value_name = "PATH")]
    replay_calibration: Option<PathBuf>,

    /// Completed labelled raw CSI capture to evaluate. May be supplied repeatedly.
    #[arg(long, value_name = "PATH")]
    replay_measurement: Vec<PathBuf>,

    /// Write the deterministic replay report to this JSON file instead of stdout.
    #[arg(long, value_name = "PATH")]
    replay_report: Option<PathBuf>,

    /// Evaluate an unlabelled classification replay against separately held truth.
    #[arg(long, value_name = "PREDICTIONS")]
    classification_evaluate: Option<PathBuf>,

    /// Separate fixed-protocol truth manifest for --classification-evaluate.
    #[arg(long, value_name = "TRUTH_MANIFEST")]
    classification_truth: Option<PathBuf>,

    /// New no-clobber classification evaluation report.
    #[arg(long, value_name = "OUTPUT")]
    classification_output: Option<PathBuf>,

    /// Combine final classification and position reports into one verdict.
    #[arg(long, value_name = "CLASSIFICATION_REPORT")]
    experiment_classification_report: Option<PathBuf>,

    /// Final position report paired with --experiment-classification-report.
    #[arg(long, value_name = "POSITION_REPORT")]
    experiment_position_report: Option<PathBuf>,

    /// New no-clobber combined fixed-room experiment report.
    #[arg(long, value_name = "OUTPUT")]
    experiment_output: Option<PathBuf>,

    /// Build a deterministic nine-point index from this training manifest.
    #[arg(long, value_name = "TRAINING_MANIFEST")]
    position_build_index: Option<PathBuf>,

    /// Inspect captures and emit manifest-ready hashes for `empty-calibration` or `position`.
    #[arg(long, value_enum, value_name = "PROTOCOL")]
    position_inspect: Option<PositionInspectionProtocolArg>,

    /// Create and seal a canonical fixed-room setup from this strict JSON specification.
    #[arg(long, value_name = "SETUP_SPEC")]
    position_create_setup: Option<PathBuf>,

    /// Load and validate this sealed setup for a normal sensing-server start.
    #[arg(long, value_name = "SEALED_SETUP")]
    position_setup: Option<PathBuf>,

    /// Pin the pre-setup sensing path to one exact CSI grid.
    /// Format: center_frequency_mhz,antenna_count,subcarrier_count,ppdu_type,layout_flags.
    /// This discovery aid is mutually exclusive with --position-setup, whose sealed grids
    /// become authoritative instead.
    #[arg(
        long,
        env = "SENSING_CSI_GRID_PIN",
        value_name = "MHZ,ANTENNAS,SUBCARRIERS,PPDU,FLAGS"
    )]
    csi_grid_pin: Option<CsiGridPin>,

    /// Activate live fingerprint positioning with this validated position index.
    #[arg(long, value_name = "POSITION_INDEX")]
    position_index: Option<PathBuf>,

    /// Exact SHA-256 of the live position-index bytes.
    #[arg(long, value_name = "HEX")]
    position_index_sha256: Option<String>,

    /// Predict unlabelled captures with this previously built position index.
    #[arg(long, value_name = "POSITION_INDEX")]
    position_predict: Option<PathBuf>,

    /// Evaluate this prediction artifact against a separately supplied truth manifest.
    #[arg(long, value_name = "PREDICTIONS")]
    position_evaluate: Option<PathBuf>,

    /// Unlabelled raw CSI capture for --position-predict. May be repeated.
    #[arg(long, value_name = "RAW_CAPTURE")]
    position_capture: Vec<PathBuf>,

    /// Separate truth manifest required only by --position-evaluate.
    #[arg(long, value_name = "TRUTH_MANIFEST")]
    position_truth: Option<PathBuf>,

    /// New no-clobber JSON artifact written by a position offline mode.
    #[arg(long, value_name = "OUTPUT")]
    position_output: Option<PathBuf>,

    /// Start field model calibration on boot (empty room required)
    #[arg(long)]
    calibrate: bool,

    // ---------------------------------------------------------------
    // ADR-102: Edge Module Registry — surface the canonical Cognitum
    // cog catalog via `GET /api/v1/edge/registry`.
    // ---------------------------------------------------------------
    /// Override the upstream URL for the edge module registry. Set to a
    /// mirror or local file://... URL for air-gapped deployments. Empty
    /// string or --no-edge-registry disables the endpoint entirely.
    #[arg(
        long,
        value_name = "URL",
        env = "RUVIEW_EDGE_REGISTRY_URL",
        default_value = "https://storage.googleapis.com/cognitum-apps/app-registry.json"
    )]
    edge_registry_url: String,

    /// Cache TTL for the edge module registry, in seconds.
    #[arg(
        long,
        value_name = "SECS",
        env = "RUVIEW_EDGE_REGISTRY_TTL_SECS",
        default_value = "3600"
    )]
    edge_registry_ttl_secs: u64,

    /// Disable the edge module registry endpoint entirely. Returns 404 on
    /// `GET /api/v1/edge/registry`. Use for air-gapped deployments.
    #[arg(long, env = "RUVIEW_NO_EDGE_REGISTRY")]
    no_edge_registry: bool,

    /// Internal loopback helper for browser start/stop/restart controls.
    #[arg(long, hide = true)]
    server_control_daemon: bool,

    /// Loopback port used by the browser server-control helper.
    #[arg(long, hide = true, default_value_t = server_control::DEFAULT_PORT)]
    server_control_port: u16,

    /// Internal launch profile used by the browser server-control helper.
    #[arg(long, hide = true)]
    server_control_spec: Option<PathBuf>,

    /// PID of the server process that initially launched the helper.
    #[arg(long, hide = true)]
    server_control_adopt_pid: Option<u32>,

    /// HTTP port of the browser UI allowed to control the helper.
    #[arg(long, hide = true, default_value_t = 8080)]
    server_control_ui_port: u16,
}

// ── Data types ───────────────────────────────────────────────────────────────

/// CSI fingerprint identity. Equal bin counts alone are not comparable across
/// channels, antenna layouts, or PPDU training fields.
type CsiGridKey = (u16, u8, u16, wifi_densepose_hardware::PpduType);

/// Exact operator-selected grid used only before a sealed setup exists.
/// The transient time-sync flag is ignored when matching live frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct CsiGridPin {
    center_frequency_mhz: u32,
    antenna_count: u8,
    subcarrier_count: u16,
    ppdu_type: u8,
    layout_flags: u8,
}

impl CsiGridPin {
    fn matches(self, frame: &raw_csi_recording::RawCsiFrame) -> bool {
        frame.center_frequency_mhz == self.center_frequency_mhz
            && frame.antenna_count == self.antenna_count
            && frame.subcarrier_count == self.subcarrier_count
            && frame.ppdu_type == self.ppdu_type
            && frame.flags & !raw_csi_recording::TRANSIENT_SYNC_FLAG == self.layout_flags
    }
}

impl std::str::FromStr for CsiGridPin {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let fields = value.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 5 {
            return Err("CSI grid pin must be MHZ,ANTENNAS,SUBCARRIERS,PPDU,FLAGS".to_string());
        }
        let center_frequency_mhz = fields[0]
            .parse::<u32>()
            .map_err(|_| "CSI grid frequency must be an unsigned integer".to_string())?;
        let antenna_count = fields[1]
            .parse::<u8>()
            .map_err(|_| "CSI grid antenna count must be an unsigned integer".to_string())?;
        let subcarrier_count = fields[2]
            .parse::<u16>()
            .map_err(|_| "CSI grid subcarrier count must be an unsigned integer".to_string())?;
        let ppdu_type = fields[3]
            .parse::<u8>()
            .map_err(|_| "CSI grid PPDU type must be an unsigned integer".to_string())?;
        let layout_flags = fields[4]
            .strip_prefix("0x")
            .or_else(|| fields[4].strip_prefix("0X"))
            .map(|hex| u8::from_str_radix(hex, 16))
            .unwrap_or_else(|| fields[4].parse::<u8>())
            .map_err(|_| "CSI grid flags must be a byte in decimal or 0xNN form".to_string())?;

        if center_frequency_mhz == 0 || antenna_count == 0 || subcarrier_count == 0 {
            return Err("CSI grid frequency and dimensions must be greater than zero".to_string());
        }
        if !matches!(ppdu_type, 0..=3 | 0xff) {
            return Err(format!("CSI grid PPDU type {ppdu_type} is unsupported"));
        }
        if layout_flags & raw_csi_recording::TRANSIENT_SYNC_FLAG != 0 {
            return Err(
                "CSI grid flags must not contain transient time-sync flag 0x10".to_string(),
            );
        }

        Ok(Self {
            center_frequency_mhz,
            antenna_count,
            subcarrier_count,
            ppdu_type,
            layout_flags,
        })
    }
}

impl Esp32Frame {
    /// The `(frequency, antennas, subcarriers, PPDU)` identity of this frame.
    fn grid(&self) -> CsiGridKey {
        (
            self.freq_mhz,
            self.n_antennas,
            self.n_subcarriers,
            self.ppdu_type,
        )
    }
}

/// Sensing update broadcast to WebSocket clients
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SensingUpdate {
    #[serde(rename = "type")]
    msg_type: String,
    timestamp: f64,
    source: String,
    tick: u64,
    nodes: Vec<NodeInfo>,
    /// Configured transmitter position in the visualization's `[x, y, z]`
    /// room coordinate system.
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_position: Option<[f64; 3]>,
    /// Physical room dimensions `[length, height, width]` in meters.
    #[serde(skip_serializing_if = "Option::is_none")]
    room_dimensions: Option<[f64; 3]>,
    features: FeatureInfo,
    classification: ClassificationInfo,
    signal_field: SignalField,
    /// Optional metric floor estimate from calibrated D6 link anomalies and
    /// the configured TX/RX geometry. `None` is used by non-ESP32 sources;
    /// ESP32 updates always include a status and only include a position when
    /// the evidence gates pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    localization: Option<coarse_localization::CoarseLocalizationEstimate>,
    /// Discrete fingerprint position. ESP32 updates always carry an explicit
    /// fail-closed state; non-ESP32 sources omit this additive property.
    #[serde(skip_serializing_if = "Option::is_none")]
    position_estimate: Option<position_live::LivePositionState>,
    /// Vital sign estimates (breathing rate, heart rate, confidence).
    #[serde(skip_serializing_if = "Option::is_none")]
    vital_signs: Option<VitalSigns>,
    // ── ADR-022 Phase 3: Enhanced multi-BSSID pipeline fields ──
    /// Enhanced motion estimate from multi-BSSID pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    enhanced_motion: Option<serde_json::Value>,
    /// Enhanced breathing estimate from multi-BSSID pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    enhanced_breathing: Option<serde_json::Value>,
    /// Posture classification from BSSID fingerprint matching.
    #[serde(skip_serializing_if = "Option::is_none")]
    posture: Option<String>,
    /// Signal quality score from multi-BSSID quality gate [0.0, 1.0].
    #[serde(skip_serializing_if = "Option::is_none")]
    signal_quality_score: Option<f64>,
    /// Quality gate verdict: "Permit", "Warn", or "Deny".
    #[serde(skip_serializing_if = "Option::is_none")]
    quality_verdict: Option<String>,
    /// Number of BSSIDs used in the enhanced sensing cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    bssid_count: Option<usize>,
    // ── ADR-023 Phase 7-8: Model inference fields ──
    /// Pose keypoints when a trained model is loaded (x, y, z, confidence).
    #[serde(skip_serializing_if = "Option::is_none")]
    pose_keypoints: Option<Vec<[f64; 4]>>,
    /// Model status when a trained model is loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    model_status: Option<serde_json::Value>,
    // ── Multi-person detection (issue #97) ──
    /// Detected persons from WiFi sensing (multi-person support).
    #[serde(skip_serializing_if = "Option::is_none")]
    persons: Option<Vec<PersonDetection>>,
    /// Estimated person count from CSI feature heuristics (1-3 for single ESP32).
    #[serde(skip_serializing_if = "Option::is_none")]
    estimated_persons: Option<usize>,
    /// Per-node feature breakdown for multi-node deployments.
    #[serde(skip_serializing_if = "Option::is_none")]
    node_features: Option<Vec<PerNodeFeatureInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeInfo {
    node_id: u8,
    rssi_dbm: f64,
    position: [f64; 3],
    amplitude: Vec<f64>,
    subcarrier_count: usize,
    /// ADR-110 iter 23 — cross-board sync snapshot for this node.
    /// `None` when no fresh sync packet has been observed (no mesh peer
    /// reachable, or this node is a singleton). Populated from
    /// `NodeState::latest_sync` and the iter 18 fps EMA.
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<NodeSyncSnapshot>,
}

const DEFAULT_NODE_POSITION: [f64; 3] = [2.0, 0.0, 1.5];

/// Resolve the deployment position for a firmware node ID.
///
/// ESP32 node IDs are provisioned from 1, while `--node-positions` is an
/// ordered list. Node 1 therefore uses entry 0, node 2 entry 1, and so on.
/// Node ID 0 remains compatible with the first entry for legacy senders.
fn configured_node_position(node_id: u8, positions: &[[f32; 3]]) -> [f64; 3] {
    let index = usize::from(node_id.saturating_sub(1));
    positions
        .get(index)
        .map(|position| position.map(f64::from))
        .unwrap_or(DEFAULT_NODE_POSITION)
}

fn parse_tx_position(value: Option<&str>) -> Option<[f64; 3]> {
    value
        .and_then(|position| {
            field_bridge::parse_node_positions(position)
                .into_iter()
                .next()
        })
        .map(|position| position.map(f64::from))
}

fn parse_room_dimensions(value: Option<&str>) -> Option<[f64; 3]> {
    parse_tx_position(value).filter(|dimensions| dimensions.iter().all(|value| *value > 0.0))
}

#[derive(Debug, Clone, PartialEq)]
struct RuntimePositionGeometry {
    tx_position: Option<[f64; 3]>,
    room_dimensions: Option<[f64; 3]>,
    node_positions: Option<Vec<[f32; 3]>>,
}

fn resolve_runtime_position_geometry(
    args: &Args,
    setup: Option<&position_setup::SealedPositionSetup>,
) -> Result<RuntimePositionGeometry, String> {
    let Some(setup) = setup else {
        return Ok(RuntimePositionGeometry {
            tx_position: parse_tx_position(args.tx_position.as_deref()),
            room_dimensions: parse_room_dimensions(args.room_dimensions.as_deref()),
            node_positions: args
                .node_positions
                .as_deref()
                .map(field_bridge::parse_node_positions)
                .filter(|positions| !positions.is_empty()),
        });
    };

    let explicit_room = args
        .room_dimensions
        .as_deref()
        .map(|value| parse_millimetre_triplet("--room-dimensions", value))
        .transpose()?;
    let explicit_tx = args
        .tx_position
        .as_deref()
        .map(|value| parse_millimetre_triplet("--tx-position", value))
        .transpose()?;
    let explicit_receivers = args
        .node_positions
        .as_deref()
        .map(parse_receiver_positions_mm)
        .transpose()?;
    setup.validate_explicit_geometry_mm(explicit_room, explicit_tx, explicit_receivers)?;

    Ok(RuntimePositionGeometry {
        tx_position: Some(setup.transmitter_position_m()),
        room_dimensions: Some(setup.room_dimensions_m()),
        node_positions: Some(
            setup
                .receiver_positions_m()
                .into_iter()
                .map(|position| position.map(|coordinate| coordinate as f32))
                .collect(),
        ),
    })
}

fn parse_receiver_positions_mm(value: &str) -> Result<[[u32; 3]; 4], String> {
    let positions: Vec<[u32; 3]> = value
        .split(';')
        .enumerate()
        .map(|(index, position)| {
            parse_millimetre_triplet(
                &format!("--node-positions RX{}", index.saturating_add(1)),
                position,
            )
        })
        .collect::<Result<_, _>>()?;
    positions.try_into().map_err(|positions: Vec<[u32; 3]>| {
        format!(
            "--node-positions must repeat exactly four RX positions when --position-setup is used, got {}",
            positions.len()
        )
    })
}

fn parse_millimetre_triplet(field: &str, value: &str) -> Result<[u32; 3], String> {
    let coordinates: Vec<u32> = value
        .split(',')
        .enumerate()
        .map(|(index, coordinate)| {
            parse_millimetre_coordinate(&format!("{field} coordinate {}", index + 1), coordinate)
        })
        .collect::<Result<_, _>>()?;
    coordinates.try_into().map_err(|coordinates: Vec<u32>| {
        format!(
            "{field} must contain exactly three comma-separated metre coordinates, got {}",
            coordinates.len()
        )
    })
}

fn parse_millimetre_coordinate(field: &str, value: &str) -> Result<u32, String> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || value.matches('.').count() > 1
    {
        return Err(format!(
            "{field} must be a non-negative decimal metre value"
        ));
    }

    let significant_fraction = fraction.trim_end_matches('0');
    if significant_fraction.len() > 3 {
        return Err(format!(
            "{field} must resolve to an exact whole number of millimetres"
        ));
    }
    let whole_mm = whole
        .parse::<u64>()
        .map_err(|_| format!("{field} is outside the supported range"))?
        .checked_mul(1_000)
        .ok_or_else(|| format!("{field} is outside the supported range"))?;
    let mut fraction_mm = 0_u64;
    for byte in fraction.bytes().take(3) {
        fraction_mm = fraction_mm * 10 + u64::from(byte - b'0');
    }
    for _ in fraction.len().min(3)..3 {
        fraction_mm *= 10;
    }
    u32::try_from(
        whole_mm
            .checked_add(fraction_mm)
            .ok_or_else(|| format!("{field} is outside the supported range"))?,
    )
    .map_err(|_| format!("{field} is outside the supported range"))
}

#[cfg(test)]
mod configured_node_position_tests {
    use super::*;

    #[test]
    fn maps_one_based_node_ids_to_ordered_positions() {
        let positions = [[0.0, 0.5, 0.28], [4.02, 0.87, 0.97]];

        for (actual, expected) in configured_node_position(1, &positions)
            .into_iter()
            .zip([0.0, 0.5, 0.28])
        {
            assert!((actual - expected).abs() < 1e-6);
        }
        for (actual, expected) in configured_node_position(2, &positions)
            .into_iter()
            .zip([4.02, 0.87, 0.97])
        {
            assert!((actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn falls_back_when_no_position_was_configured_for_node() {
        assert_eq!(
            configured_node_position(4, &[[0.0, 0.5, 0.28]]),
            DEFAULT_NODE_POSITION
        );
    }

    #[test]
    fn parses_the_configured_tx_position() {
        let position = parse_tx_position(Some("1.51,1.19,0.39")).unwrap();

        for (actual, expected) in position.into_iter().zip([1.51, 1.19, 0.39]) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert_eq!(parse_tx_position(None), None);
    }

    #[test]
    fn parses_positive_room_dimensions() {
        let dimensions = parse_room_dimensions(Some("4.02,2.59,3.44")).unwrap();

        for (actual, expected) in dimensions.into_iter().zip([4.02, 2.59, 3.44]) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert_eq!(parse_room_dimensions(Some("4.02,0,3.44")), None);
    }

    #[test]
    fn parses_explicit_setup_geometry_as_exact_millimetres() {
        assert_eq!(
            parse_millimetre_triplet("--tx-position", "1.510, 1.19, 0.3900").unwrap(),
            [1510, 1190, 390]
        );
        assert_eq!(
            parse_receiver_positions_mm("0,0.5,0.28;4.02,0.87,0.97;0,0.74,2.11;4.02,0.87,2.46")
                .unwrap(),
            [
                [0, 500, 280],
                [4020, 870, 970],
                [0, 740, 2110],
                [4020, 870, 2460],
            ]
        );
    }

    #[test]
    fn rejects_submillimetre_or_incomplete_setup_geometry() {
        assert!(parse_millimetre_triplet("--tx-position", "1.5101,1.19,0.39").is_err());
        assert!(parse_millimetre_triplet("--tx-position", "1.51,1.19").is_err());
        assert!(parse_receiver_positions_mm("0,0,0;1,1,1").is_err());
    }
}

/// ADR-110 iter 23 — per-node mesh-sync snapshot embedded in NodeInfo.
/// Surfaces what was previously only visible in the debug log so UI clients
/// can render leader / follower / offset / measured-fps live.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeSyncSnapshot {
    /// Smoothed local-vs-mesh offset in µs (negative when this node's clock
    /// is behind the leader's — see §A0.10's measured -1.16 s on the bench).
    offset_us: i64,
    /// True when this node is the elected mesh leader.
    is_leader: bool,
    /// True when this node has heard a fresh leader beacon within the
    /// firmware's VALID_WINDOW_MS gate (3 s).
    is_valid: bool,
    /// True once the EMA-smoothed offset has seeded (one full beacon round-trip).
    smoothed: bool,
    /// Sync packet's sequence high-water — used by the host to pair CSI
    /// frames against this snapshot for §A0.12 mesh-time recovery.
    sequence: u32,
    /// Per-node CSI sequence-clock rate, derived from sequence/time deltas
    /// between valid sync packets and smoothed with an EMA.
    csi_fps_ema: f64,
    /// How many valid sync-to-sync intervals have contributed to the FPS EMA.
    /// Zero means mesh extrapolation is deliberately unavailable.
    csi_fps_samples: u32,
    /// ADR-110 iter 34 — milliseconds since the host last received a sync
    /// packet from this node. Lets UI dashboards render sync-age decay
    /// (badge fades after 5 s, drops off after the 9 s mesh_aligned_us
    /// staleness gate). `None` only when the host never had Instant data
    /// for this node, which shouldn't happen in normal flow but is
    /// modeled defensively.
    #[serde(skip_serializing_if = "Option::is_none")]
    staleness_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MeshTimeStatus {
    NoSync,
    AwaitingCsiFrame,
    InvalidSync,
    StaleSync,
    FpsWarmingUp,
    ImplausibleFps,
    SequenceBeforeSync,
    SequenceJump,
    SequenceRegression,
    NodeReboot,
    Valid,
}

#[derive(Debug, Clone, Copy)]
struct SyncRateAnchor {
    sequence: u32,
    local_us: u64,
    observed_at: std::time::Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeatureInfo {
    mean_rssi: f64,
    variance: f64,
    motion_band_power: f64,
    breathing_band_power: f64,
    dominant_freq_hz: f64,
    change_points: usize,
    spectral_power: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClassificationInfo {
    motion_level: String,
    presence: bool,
    confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalField {
    grid_size: [usize; 3],
    values: Vec<f64>,
}

/// WiFi-derived pose keypoint (17 COCO keypoints)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PoseKeypoint {
    name: String,
    x: f64,
    y: f64,
    z: f64,
    confidence: f64,
}

/// Person detection from WiFi sensing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersonDetection {
    id: u32,
    confidence: f64,
    keypoints: Vec<PoseKeypoint>,
    bbox: BoundingBox,
    zone: String,
    /// Room-world position `[x, y, z]` (meters). Live ESP32 updates only attach
    /// this when the setup-bound discrete fingerprint model emits P01-P09.
    /// Synthetic/non-ESP32 sources retain the legacy signal-field peak mapping.
    #[serde(default)]
    position: [f64; 3],
    /// Motion magnitude on the Observatory's `0..100` scale, passed through
    /// from the measured `motion_band_power` (issue #1050).
    #[serde(default)]
    motion_score: f64,
    /// Coarse posture label (`"standing"`/`"lying"`/…) when a **real** aggregate
    /// posture estimate exists, else `None`. ESP32 discrete-position markers
    /// never receive a synthetic posture or skeleton.
    #[serde(skip_serializing_if = "Option::is_none")]
    pose: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoundingBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Per-node sensing state for multi-node deployments (issue #249).
/// Each ESP32 node gets its own frame history, smoothing buffers, and vital
/// sign detector so that data from different nodes is never mixed.
struct SourceBindingObservation {
    observed_at: std::time::Instant,
    complete: bool,
    matches_setup: bool,
    /// Private comparison value only. It is never serialized or logged.
    tx_filter_identity: String,
}

impl SourceBindingObservation {
    fn validated(
        binding: &raw_csi_recording::SourceBinding,
        observed_at: std::time::Instant,
        matches_setup: bool,
    ) -> Result<Self, String> {
        binding
            .validate()
            .map_err(|error| format!("invalid TX-source binding: {error}"))?;
        Ok(Self {
            observed_at,
            complete: binding.has_required_flags(),
            matches_setup,
            tx_filter_identity: binding.tx_filter_sha256.clone(),
        })
    }

    fn is_fresh(&self, now: std::time::Instant) -> bool {
        now.saturating_duration_since(self.observed_at) <= SOURCE_BINDING_FRESHNESS_TIMEOUT
    }
}

fn validated_complete_source_binding(
    binding: Option<&raw_csi_recording::SourceBinding>,
    observed_at: std::time::Instant,
    matches_setup: bool,
) -> Result<SourceBindingObservation, String> {
    let binding =
        binding.ok_or_else(|| "raw CSI frame has no TX-source binding trailer".to_string())?;
    let observation = SourceBindingObservation::validated(binding, observed_at, matches_setup)?;
    if !observation.complete {
        return Err("TX-source binding is incomplete; exactly 0x07 is required".to_string());
    }
    Ok(observation)
}

#[derive(Debug, Clone)]
pub(crate) struct FusionFrameSample {
    pub(crate) amplitude: Vec<f64>,
    pub(crate) host_monotonic_us: u64,
    pub(crate) mesh_timestamp_us: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerNodeFeatureInfo {
    node_id: u8,
    features: FeatureInfo,
    /// Effective node classification. Once D5 is calibrated this reflects the
    /// accepted D5 observation for still/absent, while obvious D4 movement
    /// remains authoritative.
    classification: ClassificationInfo,
    /// Raw D4 result retained for diagnostics while D5 is active.
    d4_classification: ClassificationInfo,
    /// Diagnostic motion values used to tune this physical installation.
    raw_motion_score: f64,
    smoothed_motion_score: f64,
    quiet_motion_baseline: f64,
    /// Experimental D5 empty-room reference and current vote diagnostics.
    #[serde(default)]
    d5_presence: d5_presence::NodePresenceSnapshot,
    /// Static D6 CSI-shape reference and current per-link anomaly diagnostics.
    #[serde(default)]
    d6_fingerprint: d6_fingerprint::NodeFingerprintSnapshot,
    rssi_dbm: f64,
    last_seen_ms: u64,
    frame_rate_hz: f64,
    stale: bool,
    /// ADR-084 Pass 3 cluster-Pi novelty score in `[0.0, 1.0]`.
    /// `0.0` = exact-match-in-bank, `1.0` = no overlap with recent
    /// per-node frame history. `None` until the first
    /// `update_novelty()` call. Consumers (model-wake gate, anomaly
    /// emit, UI heatmap) read this to decide whether to escalate.
    #[serde(skip_serializing_if = "Option::is_none")]
    novelty_score: Option<f32>,
}

/// Build a per-node feature snapshot for the WebSocket envelope.
///
/// ADR-084 Pass 3.6 — exposes `last_novelty_score` from each
/// `NodeState` to the WebSocket consumer. Returns `None` when the
/// node map is empty (no live ESP32 frames have been ingested yet),
/// so the existing `node_features: None` semantics on cold-start are
/// preserved.
///
/// Stale flag uses 5-second threshold matching `ESP32_OFFLINE_TIMEOUT`.
fn build_node_features(
    node_states: &std::collections::HashMap<u8, NodeState>,
    now: std::time::Instant,
    d5_phase: d5_presence::CalibrationPhase,
    position_setup_active: bool,
) -> Option<Vec<PerNodeFeatureInfo>> {
    if node_states.is_empty() {
        return None;
    }
    let mut entries: Vec<PerNodeFeatureInfo> = node_states
        .iter()
        .map(|(&node_id, ns)| {
            let last_seen_ms = ns
                .last_frame_time
                .map(|t| now.saturating_duration_since(t).as_millis() as u64)
                .unwrap_or(u64::MAX);
            let stale = ns
                .last_frame_time
                .map(|t| now.saturating_duration_since(t) > ESP32_OFFLINE_TIMEOUT)
                .unwrap_or(true);
            let features = ns.latest_features.clone().unwrap_or(FeatureInfo {
                mean_rssi: 0.0,
                variance: 0.0,
                motion_band_power: 0.0,
                breathing_band_power: 0.0,
                dominant_freq_hz: 0.0,
                change_points: 0,
                spectral_power: 0.0,
            });
            let d4_classification = ClassificationInfo {
                motion_level: ns.current_motion_level.clone(),
                presence: !matches!(ns.current_motion_level.as_str(), "absent"),
                confidence: ns.motion_confidence,
            };
            let d4_reports_motion = matches!(
                ns.current_motion_level.as_str(),
                "active" | "present_moving"
            );
            let classification = match d5_phase {
                d5_presence::CalibrationPhase::Uncalibrated => d4_classification.clone(),
                d5_presence::CalibrationPhase::Collecting if d4_reports_motion => {
                    d4_classification.clone()
                }
                d5_presence::CalibrationPhase::Collecting => ClassificationInfo {
                    motion_level: "calibrating".to_string(),
                    presence: false,
                    confidence: 0.0,
                },
                d5_presence::CalibrationPhase::Ready if ns.d6_fingerprint.evidence_ready(now) => {
                    let vote = ns.d6_fingerprint.vote();
                    let anomaly_ratio = ns.d6_fingerprint.anomaly_ratio().unwrap_or(0.0);
                    let confidence = if vote {
                        (anomaly_ratio / 2.0).clamp(0.0, 1.0)
                    } else {
                        (1.0 - anomaly_ratio).clamp(0.0, 1.0)
                    };
                    ClassificationInfo {
                        motion_level: if vote {
                            if d4_reports_motion {
                                ns.current_motion_level.clone()
                            } else {
                                "present_still".to_string()
                            }
                        } else {
                            "absent".to_string()
                        },
                        presence: vote,
                        confidence,
                    }
                }
                d5_presence::CalibrationPhase::Ready => ClassificationInfo {
                    motion_level: "unknown".to_string(),
                    presence: false,
                    confidence: 0.0,
                },
            };
            let classification = apply_position_setup_classification_gate(
                position_setup_active,
                d5_phase,
                classification,
            );
            PerNodeFeatureInfo {
                node_id,
                features,
                classification,
                d4_classification,
                raw_motion_score: ns.latest_raw_motion,
                smoothed_motion_score: ns.smoothed_motion,
                quiet_motion_baseline: ns.baseline_motion,
                d5_presence: ns.d5_presence.snapshot(now),
                d6_fingerprint: ns.d6_fingerprint.snapshot(now),
                rssi_dbm: ns.rssi_history.back().copied().unwrap_or(0.0),
                last_seen_ms,
                frame_rate_hz: if ns.csi_fps_samples > 0 {
                    ns.csi_fps_ema
                } else {
                    0.0
                },
                stale,
                novelty_score: ns.last_novelty_score,
            }
        })
        .collect();
    entries.sort_by_key(|entry| entry.node_id);
    Some(entries)
}

// ── ADR-044 §5.2: Rolling P95 adaptive feature normalizer ────────────────────

/// Streaming P95 estimator over a fixed-size sliding window.
///
/// Self-calibrates feature normalization to whatever distribution the deployment
/// produces — no hardcoded scale values that can saturate in large rooms or
/// degrade in high-interference environments.
///
/// O(n log n) per query via sorted copy — acceptable at 20 Hz with window=600.
/// Cold-start (len < min_samples) returns `None` so the caller uses the legacy
/// fixed denominator, preserving day-0 behaviour.
pub struct RollingP95 {
    buf: std::collections::VecDeque<f64>,
    window: usize,
    min_samples: usize,
}

impl RollingP95 {
    pub fn new(window: usize, min_samples: usize) -> Self {
        Self {
            buf: std::collections::VecDeque::with_capacity(window),
            window,
            min_samples,
        }
    }

    pub fn push(&mut self, v: f64) {
        if self.buf.len() == self.window {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }

    /// Returns `Some(p95)` once enough samples have accumulated, else `None`.
    pub fn current(&self) -> Option<f64> {
        if self.buf.len() < self.min_samples {
            return None;
        }
        let mut sorted: Vec<f64> = self.buf.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((sorted.len() as f64) * 0.95).ceil() as usize;
        Some(sorted[idx.saturating_sub(1).min(sorted.len() - 1)])
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

// ── ADR-044 §5.3: Runtime config persistence ─────────────────────────────────

/// Runtime configuration that persists across server restarts via `data/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeConfig {
    /// Divisor for multi-node person-count deduplication (sum / factor).
    pub dedup_factor: f64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self { dedup_factor: 3.0 }
    }
}

/// Load persisted runtime config from `<data_dir>/config.json`.
/// Falls back to [`RuntimeConfig::default`] if the file is absent or malformed.
pub(crate) fn load_runtime_config(data_dir: &std::path::Path) -> RuntimeConfig {
    let path = data_dir.join("config.json");
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => RuntimeConfig::default(),
    }
}

/// Persist runtime config to `<data_dir>/config.json`.
pub(crate) fn save_runtime_config(data_dir: &std::path::Path, config: &RuntimeConfig) {
    let path = data_dir.join("config.json");
    if let Ok(json) = serde_json::to_string_pretty(config) {
        if let Err(e) = std::fs::write(&path, json) {
            warn!("Failed to save runtime config to {}: {e}", path.display());
        } else {
            info!("Runtime config saved to {}", path.display());
        }
    }
}

/// Shared application state
#[derive(Debug, Default)]
struct RecordingWriterResult {
    frames_written: u64,
    dropped_frames: u64,
    error: Option<String>,
    rx_summaries: BTreeMap<u8, raw_csi_recording::RawCsiRxSummary>,
}

#[derive(Debug, Clone)]
enum RawCsiIngress {
    Frame(raw_csi_recording::RawCsiFrame),
    Rejected { rx_id: Option<u8>, reason: String },
}

impl RecordingWriterResult {
    fn incomplete(&self) -> bool {
        self.frames_written == 0 || self.dropped_frames > 0 || self.error.is_some()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum RecordingLifecyclePhase {
    #[default]
    Idle,
    Recording,
    Finalizing,
}

fn edge_vitals_classification(vitals: &Esp32VitalsPacket) -> ClassificationInfo {
    ClassificationInfo {
        motion_level: if vitals.motion {
            "present_moving"
        } else if vitals.presence {
            "present_still"
        } else {
            "absent"
        }
        .to_string(),
        presence: vitals.presence,
        confidence: vitals.presence_score as f64,
    }
}

/// Edge-vitals packets contain only already-derived classifications and no
/// setup-bound raw CSI or TX-source trailer. They remain a useful fallback for
/// ordinary RuView sessions, but cannot be measurement input for a sealed
/// position experiment.
fn edge_vitals_measurement_input_allowed(position_setup_active: bool) -> bool {
    !position_setup_active
}

/// Convert internal edge measurements into the public contract after the
/// setup-bound classification decision has been made.
fn public_edge_vitals_packet(
    vitals: &Esp32VitalsPacket,
    classification: &ClassificationInfo,
) -> Esp32VitalsPacket {
    let mut public = vitals.clone();
    public.presence = classification.presence;
    public.motion = classification.presence
        && matches!(
            classification.motion_level.as_str(),
            "active" | "present_moving"
        );
    public.n_persons = if classification.presence {
        vitals.n_persons.max(1)
    } else {
        0
    };
    public.presence_score = if classification.presence {
        classification.confidence.clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    if !classification.presence {
        public.fall_detected = false;
        public.breathing_rate_bpm = 0.0;
        public.heartrate_bpm = 0.0;
    }
    public
}

// ── Signal field generation ──────────────────────────────────────────────────

/// Generate a signal field that reflects where motion and signal changes are occurring.
///
/// Instead of a fixed-animation circle, this function uses the actual sensing data:
/// - `subcarrier_variances`: per-subcarrier variance computed from the frame history.
///   High-variance subcarriers indicate spatial directions where the signal is disrupted.
/// - `motion_score`: overall motion intensity [0, 1].
/// - `breathing_rate_hz`: estimated breathing rate in Hz; if > 0, adds a breathing ring.
/// - `signal_quality`: overall quality metric [0, 1] modulates field brightness.
///
/// The field grid is 20×20 cells representing a top-down view of the room.
/// Hotspots are derived from the subcarrier index (treated as an angular bin) so that
/// subcarriers with the highest variance produce peaks at the corresponding directions.
fn generate_signal_field(
    _mean_rssi: f64,
    motion_score: f64,
    breathing_rate_hz: f64,
    signal_quality: f64,
    subcarrier_variances: &[f64],
) -> SignalField {
    let grid = 20usize;
    let mut values = vec![0.0f64; grid * grid];
    let center = (grid as f64 - 1.0) / 2.0;

    // Normalise subcarrier variances to [0, 1].
    let max_var = subcarrier_variances.iter().cloned().fold(0.0f64, f64::max);
    let norm_factor = if max_var > 1e-9 { max_var } else { 1.0 };

    // For each cell, accumulate contributions from all subcarriers.
    // Each subcarrier k is assigned an angular direction proportional to its index
    // so that different subcarriers illuminate different regions of the room.
    let n_sub = subcarrier_variances.len().max(1);
    for (k, &var) in subcarrier_variances.iter().enumerate() {
        let weight = (var / norm_factor) * motion_score;
        if weight < 1e-6 {
            continue;
        }
        // Map subcarrier index to an angle across the full 2π sweep.
        let angle = (k as f64 / n_sub as f64) * 2.0 * std::f64::consts::PI;
        // Place the hotspot at a distance proportional to the weight, capped at 40% of
        // the grid radius so it stays within the room model.
        let radius = center * 0.8 * weight.sqrt();
        let hx = center + radius * angle.cos();
        let hz = center + radius * angle.sin();

        for z in 0..grid {
            for x in 0..grid {
                let dx = x as f64 - hx;
                let dz = z as f64 - hz;
                let dist2 = dx * dx + dz * dz;
                // Gaussian blob centred on the hotspot; spread scales with weight.
                let spread = (0.5 + weight * 2.0).max(0.5);
                values[z * grid + x] += weight * (-dist2 / (2.0 * spread * spread)).exp();
            }
        }
    }

    // Base radial attenuation from the router assumed at grid centre.
    for z in 0..grid {
        for x in 0..grid {
            let dx = x as f64 - center;
            let dz = z as f64 - center;
            let dist = (dx * dx + dz * dz).sqrt();
            let base = signal_quality * (-dist * 0.12).exp();
            values[z * grid + x] += base * 0.3;
        }
    }

    // Breathing ring: if a breathing rate was estimated add a faint annular highlight
    // at a radius corresponding to typical chest-wall displacement range.
    if breathing_rate_hz > 0.05 {
        let ring_r = center * 0.55;
        let ring_width = 1.8f64;
        for z in 0..grid {
            for x in 0..grid {
                let dx = x as f64 - center;
                let dz = z as f64 - center;
                let dist = (dx * dx + dz * dz).sqrt();
                let ring_val =
                    0.08 * (-(dist - ring_r).powi(2) / (2.0 * ring_width * ring_width)).exp();
                values[z * grid + x] += ring_val;
            }
        }
    }

    // Clamp and normalise to [0, 1].
    let field_max = values.iter().cloned().fold(0.0f64, f64::max);
    let scale = if field_max > 1e-9 {
        1.0 / field_max
    } else {
        1.0
    };
    for v in &mut values {
        *v = (*v * scale).clamp(0.0, 1.0);
    }

    SignalField {
        grid_size: [grid, 1, grid],
        values,
    }
}

/// Build the calibrated D6 link-likelihood estimate for the fixed room.
///
/// This is deliberately a coarse geometry prior. It uses no invented
/// subcarrier directions and emits no position unless presence, reference,
/// freshness, and multi-link gates all pass.
fn estimate_live_localization(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
    classification: &ClassificationInfo,
    tx_position: Option<[f64; 3]>,
    room_dimensions: Option<[f64; 3]>,
    node_positions: &[[f32; 3]],
) -> coarse_localization::CoarseLocalizationEstimate {
    let transmitter = tx_position
        .map(|position| coarse_localization::FloorPoint {
            x: position[0],
            z: position[2],
        })
        .unwrap_or(coarse_localization::FloorPoint {
            x: f64::NAN,
            z: f64::NAN,
        });
    let bounds = room_dimensions
        .map(|dimensions| coarse_localization::FloorBounds {
            min_x: 0.0,
            max_x: dimensions[0],
            min_z: 0.0,
            max_z: dimensions[2],
        })
        .unwrap_or(coarse_localization::FloorBounds {
            min_x: 0.0,
            max_x: 0.0,
            min_z: 0.0,
            max_z: 0.0,
        });
    // HashMap iteration order is randomized. Stable numeric link ordering keeps
    // live and replay likelihood calculations byte-for-byte reproducible.
    let mut nodes: Vec<(&u8, &NodeState)> = node_states.iter().collect();
    nodes.sort_by_key(|(node_id, _)| **node_id);
    let observations: Vec<coarse_localization::CoarseLinkObservation> = nodes
        .into_iter()
        .map(|(&node_id, node)| {
            let receiver = configured_node_position(node_id, node_positions);
            coarse_localization::CoarseLinkObservation {
                node_id: node_id.to_string(),
                receiver: coarse_localization::FloorPoint {
                    x: receiver[0],
                    z: receiver[2],
                },
                anomaly_strength: node.d6_fingerprint.anomaly_strength(),
                reference_ready: node.d6_fingerprint.reference_ready(),
                evidence_ready: node.d6_fingerprint.evidence_ready(now),
            }
        })
        .collect();
    let config = coarse_localization::CoarseLocalizationConfig {
        grid_columns: 20,
        grid_rows: 20,
        ..Default::default()
    };

    coarse_localization::estimate_coarse_location(
        bounds,
        transmitter,
        &observations,
        classification.presence,
        config,
    )
}

fn signal_field_from_localization(
    localization: &coarse_localization::CoarseLocalizationEstimate,
) -> SignalField {
    match localization.probability_map.as_ref() {
        Some(map) => SignalField {
            grid_size: [map.columns, 1, map.rows],
            values: map.values.clone(),
        },
        None => SignalField {
            grid_size: [20, 1, 20],
            values: vec![0.0; 20 * 20],
        },
    }
}

/// Clone a sensing update for public delivery and fail closed when the source
/// has gone offline. Fixed deployment geometry remains configured in
/// `tx_position` / `room_dimensions`, but stale measurements and detections are
/// never rebroadcast as current evidence.
fn public_sensing_update(update: &SensingUpdate, effective_source: &str) -> SensingUpdate {
    let mut public = update.clone();
    public.source = effective_source.to_string();
    if !effective_source.ends_with(":offline") {
        return public;
    }

    let localization = coarse_localization::CoarseLocalizationEstimate::unavailable();
    public.nodes.clear();
    public.features = FeatureInfo {
        mean_rssi: 0.0,
        variance: 0.0,
        motion_band_power: 0.0,
        breathing_band_power: 0.0,
        dominant_freq_hz: 0.0,
        change_points: 0,
        spectral_power: 0.0,
    };
    public.classification = ClassificationInfo {
        motion_level: "unknown".to_string(),
        presence: false,
        confidence: 0.0,
    };
    public.signal_field = signal_field_from_localization(&localization);
    public.localization = Some(localization);
    public.position_estimate = Some(position_live::LivePositionState::Stale);
    public.vital_signs = None;
    public.enhanced_motion = None;
    public.enhanced_breathing = None;
    public.posture = None;
    public.signal_quality_score = None;
    public.quality_verdict = None;
    public.bssid_count = None;
    public.pose_keypoints = None;
    public.persons = None;
    public.estimated_persons = None;
    public.node_features = None;
    public
}

// ── Feature extraction from ESP32 frame ──────────────────────────────────────

/// Estimate breathing rate in Hz from the amplitude time series stored in `frame_history`.
///
/// Approach:
/// 1. Build a scalar time series by computing the mean amplitude of each historical frame.
/// 2. Run a peak-detection pass: count rising-edge zero-crossings of the de-meaned signal.
/// 3. Convert the crossing rate to Hz, clipped to the physiological range 0.1–0.5 Hz
///    (12–30 breaths/min).
///
/// For accuracy the function additionally applies a simple 3-tap Goertzel-style power
/// estimate at evenly-spaced candidate frequencies in the breathing band and returns
/// the candidate with the highest energy.
fn estimate_breathing_rate_hz(frame_history: &VecDeque<Vec<f64>>, sample_rate_hz: f64) -> f64 {
    let n = frame_history.len();
    if n < 6 {
        return 0.0;
    }

    // Build scalar time series: mean amplitude per frame.
    let series: Vec<f64> = frame_history
        .iter()
        .map(|amps| {
            if amps.is_empty() {
                0.0
            } else {
                amps.iter().sum::<f64>() / amps.len() as f64
            }
        })
        .collect();

    let mean_s = series.iter().sum::<f64>() / n as f64;
    // De-mean.
    let detrended: Vec<f64> = series.iter().map(|x| x - mean_s).collect();

    // Goertzel power at candidate frequencies in the breathing band [0.1, 0.5] Hz.
    // We evaluate 9 candidate frequencies uniformly spaced in that band.
    let n_candidates = 9usize;
    let f_low = 0.1f64;
    let f_high = 0.5f64;
    let mut best_freq = 0.0f64;
    let mut best_power = 0.0f64;

    for i in 0..n_candidates {
        let freq = f_low + (f_high - f_low) * i as f64 / (n_candidates - 1).max(1) as f64;
        let omega = 2.0 * std::f64::consts::PI * freq / sample_rate_hz;
        let coeff = 2.0 * omega.cos();
        let mut s_prev2 = 0.0f64;
        let mut s_prev1 = 0.0f64;
        for &x in &detrended {
            let s = x + coeff * s_prev1 - s_prev2;
            s_prev2 = s_prev1;
            s_prev1 = s;
        }
        // Goertzel magnitude squared.
        let power = s_prev2 * s_prev2 + s_prev1 * s_prev1 - coeff * s_prev1 * s_prev2;
        if power > best_power {
            best_power = power;
            best_freq = freq;
        }
    }

    // Only report a breathing rate if the Goertzel energy is meaningfully above noise.
    // Threshold: power must exceed 10× the average power across all candidates.
    let avg_power = {
        let mut total = 0.0f64;
        for i in 0..n_candidates {
            let freq = f_low + (f_high - f_low) * i as f64 / (n_candidates - 1).max(1) as f64;
            let omega = 2.0 * std::f64::consts::PI * freq / sample_rate_hz;
            let coeff = 2.0 * omega.cos();
            let mut s_prev2 = 0.0f64;
            let mut s_prev1 = 0.0f64;
            for &x in &detrended {
                let s = x + coeff * s_prev1 - s_prev2;
                s_prev2 = s_prev1;
                s_prev1 = s;
            }
            total += s_prev2 * s_prev2 + s_prev1 * s_prev1 - coeff * s_prev1 * s_prev2;
        }
        total / n_candidates as f64
    };

    if best_power > avg_power * 3.0 {
        best_freq.clamp(f_low, f_high)
    } else {
        0.0
    }
}

/// Compute per-subcarrier variance across the sliding window of `frame_history`.
///
/// For each subcarrier index `k`, returns `Var[A_k]` over all stored frames.
/// This captures spatial signal variation; subcarriers whose amplitude fluctuates
/// heavily across time correspond to directions with motion.
/// Compute per-subcarrier importance weights using a simple sensitivity split.
///
/// Subcarriers whose sensitivity (amplitude magnitude) is above the median are
/// considered "sensitive" and receive weight `1.0 + (sens / max_sens)` (range 1.0–2.0).
/// The rest receive a baseline weight of 0.5. This mirrors the RuVector mincut
/// partition logic without requiring the graph dependency.
fn compute_subcarrier_importance_weights(sensitivity: &[f64]) -> Vec<f64> {
    let n = sensitivity.len();
    if n == 0 {
        return vec![];
    }
    let max_sens = sensitivity
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(1e-9);

    // Compute median via a sorted copy.
    let mut sorted = sensitivity.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if n % 2 == 0 {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    };

    sensitivity
        .iter()
        .map(|&s| {
            if s >= median {
                1.0 + (s / max_sens).min(1.0)
            } else {
                0.5
            }
        })
        .collect()
}

fn compute_subcarrier_variances(frame_history: &VecDeque<Vec<f64>>, n_sub: usize) -> Vec<f64> {
    if frame_history.is_empty() || n_sub == 0 {
        return vec![0.0; n_sub];
    }

    let n_frames = frame_history.len() as f64;
    let mut means = vec![0.0f64; n_sub];
    let mut sq_means = vec![0.0f64; n_sub];

    for frame in frame_history.iter() {
        for k in 0..n_sub {
            let a = if k < frame.len() { frame[k] } else { 0.0 };
            means[k] += a;
            sq_means[k] += a * a;
        }
    }

    (0..n_sub)
        .map(|k| {
            let mean = means[k] / n_frames;
            let sq_mean = sq_means[k] / n_frames;
            (sq_mean - mean * mean).max(0.0)
        })
        .collect()
}

const MIN_CSI_FRAME_RMS: f64 = 1e-9;

fn frame_rms(amplitudes: &[f64]) -> f64 {
    if amplitudes.is_empty() {
        return 0.0;
    }

    (amplitudes.iter().map(|a| a * a).sum::<f64>() / amplitudes.len() as f64).sqrt()
}

/// Compare two CSI amplitude shapes without treating a uniform gain change as motion.
///
/// ESP-IDF may automatically scale consecutive CSI frames by different uniform factors
/// when manual CSI scaling is disabled. Normalising each frame by its own RMS preserves
/// changes between subcarriers while cancelling that frame-wide gain factor.
fn scale_invariant_frame_difference(current: &[f64], previous: &[f64]) -> f64 {
    let n_cmp = current.len().min(previous.len());
    if n_cmp == 0 {
        return 0.0;
    }

    let current = &current[..n_cmp];
    let previous = &previous[..n_cmp];
    let current_rms = frame_rms(current);
    let previous_rms = frame_rms(previous);

    match (
        current_rms > MIN_CSI_FRAME_RMS,
        previous_rms > MIN_CSI_FRAME_RMS,
    ) {
        (false, false) => 0.0,
        (false, true) | (true, false) => 1.0,
        (true, true) => {
            let diff_energy = current
                .iter()
                .zip(previous.iter())
                .map(|(current_amp, previous_amp)| {
                    (current_amp / current_rms - previous_amp / previous_rms).powi(2)
                })
                .sum::<f64>()
                / n_cmp as f64;
            diff_energy.sqrt().clamp(0.0, 1.0)
        }
    }
}

/// Compute temporal subcarrier variance after normalising every frame by its own RMS.
///
/// Only frames with the active grid length participate. The per-node grid gate normally
/// guarantees this already; the explicit check prevents an incomplete frame from being
/// padded with zeros and misclassified as motion.
fn compute_scale_invariant_subcarrier_variances(
    frame_history: &VecDeque<Vec<f64>>,
    n_sub: usize,
) -> Vec<f64> {
    if frame_history.is_empty() || n_sub == 0 {
        return vec![0.0; n_sub];
    }

    let mut means = vec![0.0f64; n_sub];
    let mut sq_means = vec![0.0f64; n_sub];
    let mut n_frames = 0usize;

    for frame in frame_history.iter().filter(|frame| frame.len() >= n_sub) {
        let amplitudes = &frame[..n_sub];
        let rms = frame_rms(amplitudes);
        for (k, amplitude) in amplitudes.iter().enumerate() {
            let normalised = if rms > MIN_CSI_FRAME_RMS {
                amplitude / rms
            } else {
                0.0
            };
            means[k] += normalised;
            sq_means[k] += normalised * normalised;
        }
        n_frames += 1;
    }

    if n_frames == 0 {
        return vec![0.0; n_sub];
    }

    let n_frames = n_frames as f64;
    (0..n_sub)
        .map(|k| {
            let mean = means[k] / n_frames;
            let sq_mean = sq_means[k] / n_frames;
            (sq_mean - mean * mean).max(0.0)
        })
        .collect()
}

/// Extract features from the current ESP32 frame, enhanced with temporal context from
/// `frame_history`.
///
/// Improvements over the previous single-frame approach:
///
/// - **Variance**: computed as the mean of per-subcarrier temporal variance across the
///   sliding window, not just the intra-frame spatial variance.
/// - **Motion detection**: uses frame-to-frame temporal difference (mean L2 change
///   between the current frame and the previous frame) normalised by signal amplitude,
///   so that actual changes are detected rather than just a threshold on the current frame.
/// - **Breathing rate**: estimated via Goertzel filter bank on the 0.1–0.5 Hz band of
///   the amplitude time series.
/// - **Signal quality**: based on SNR estimate (RSSI – noise floor) and subcarrier
///   variance stability.
///
/// Returns (features, raw_classification, breathing_rate_hz, sub_variances, raw_motion_score).
fn extract_features_from_frame(
    frame: &Esp32Frame,
    frame_history: &VecDeque<Vec<f64>>,
    sample_rate_hz: f64,
) -> (FeatureInfo, ClassificationInfo, f64, Vec<f64>, f64) {
    let n_sub = frame.amplitudes.len().max(1);
    let n = n_sub as f64;
    let mean_rssi = frame.rssi as f64;

    // ── RuVector Phase 1: subcarrier importance weighting ──
    // Compute per-subcarrier sensitivity from amplitude magnitude, then weight
    // sensitive subcarriers higher (>1.0) and insensitive ones lower (0.5).
    // This emphasises body-motion-correlated subcarriers in all downstream metrics.
    let sub_sensitivity: Vec<f64> = frame.amplitudes.iter().map(|a| a.abs()).collect();
    let importance_weights = compute_subcarrier_importance_weights(&sub_sensitivity);

    let weight_sum: f64 = importance_weights.iter().sum::<f64>();
    let mean_amp: f64 = if weight_sum > 0.0 {
        frame
            .amplitudes
            .iter()
            .zip(importance_weights.iter())
            .map(|(a, w)| a * w)
            .sum::<f64>()
            / weight_sum
    } else {
        frame.amplitudes.iter().sum::<f64>() / n
    };

    // ── Intra-frame subcarrier variance (weighted by importance) ──
    let intra_variance: f64 = if weight_sum > 0.0 {
        frame
            .amplitudes
            .iter()
            .zip(importance_weights.iter())
            .map(|(a, w)| w * (a - mean_amp).powi(2))
            .sum::<f64>()
            / weight_sum
    } else {
        frame
            .amplitudes
            .iter()
            .map(|a| (a - mean_amp).powi(2))
            .sum::<f64>()
            / n
    };

    // ── Temporal (sliding-window) per-subcarrier variance ──
    let sub_variances = compute_subcarrier_variances(frame_history, n_sub);
    let temporal_variance: f64 = if sub_variances.is_empty() {
        intra_variance
    } else {
        sub_variances.iter().sum::<f64>() / sub_variances.len() as f64
    };

    // Use the larger of intra-frame and temporal variance as the reported variance.
    let variance = intra_variance.max(temporal_variance);

    // ── Spectral power ──
    let spectral_power: f64 = frame.amplitudes.iter().map(|a| a * a).sum::<f64>() / n;

    // ── Motion band power (upper half of subcarriers, high spatial frequency) ──
    let half = frame.amplitudes.len() / 2;
    let motion_band_power = if half > 0 {
        frame.amplitudes[half..]
            .iter()
            .map(|a| (a - mean_amp).powi(2))
            .sum::<f64>()
            / (frame.amplitudes.len() - half) as f64
    } else {
        0.0
    };

    // ── Breathing band power (lower half of subcarriers, low spatial frequency) ──
    let breathing_band_power = if half > 0 {
        frame.amplitudes[..half]
            .iter()
            .map(|a| (a - mean_amp).powi(2))
            .sum::<f64>()
            / half as f64
    } else {
        0.0
    };

    // ── Dominant frequency via peak subcarrier index ──
    let peak_idx = frame
        .amplitudes
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let dominant_freq_hz = peak_idx as f64 * 0.05;

    // ── Change point detection (threshold-crossing count in current frame) ──
    let threshold = mean_amp * 1.2;
    let change_points = frame
        .amplitudes
        .windows(2)
        .filter(|w| (w[0] < threshold) != (w[1] < threshold))
        .count();

    // ── Motion score: sliding-window temporal difference ──
    // The caller has already appended the current frame, so compare it with
    // the second-to-last entry. Comparing against `back()` compares the frame
    // with itself and permanently yields zero temporal motion.
    // Each frame is normalised independently so ESP-IDF's automatic frame-wide
    // CSI gain changes do not become false movement.
    let temporal_motion_score = if let Some(prev_frame) = frame_history.iter().rev().nth(1) {
        scale_invariant_frame_difference(&frame.amplitudes, prev_frame)
    } else {
        0.0
    };

    // Motion must come from change over time. Intra-frame spatial variance,
    // motion-band power and change-point count describe the static multipath
    // shape too, so using their absolute values made a quiet room look active.
    // The windowed variance uses the same per-frame normalisation. Otherwise a
    // gain jump would leak back into the score through this secondary term.
    let scale_invariant_sub_variances =
        compute_scale_invariant_subcarrier_variances(frame_history, n_sub);
    let scale_invariant_temporal_variance = if scale_invariant_sub_variances.is_empty() {
        0.0
    } else {
        scale_invariant_sub_variances.iter().sum::<f64>()
            / scale_invariant_sub_variances.len() as f64
    };
    let variance_motion = scale_invariant_temporal_variance.sqrt().clamp(0.0, 1.0);
    let motion_score = (temporal_motion_score * 0.8 + variance_motion * 0.2).clamp(0.0, 1.0);

    // ── Signal quality metric ──
    // Based on estimated SNR (RSSI relative to noise floor) and subcarrier consistency.
    let snr_db = (frame.rssi as f64 - frame.noise_floor as f64).max(0.0);
    let snr_quality = (snr_db / 40.0).clamp(0.0, 1.0); // 40 dB → quality = 1.0
                                                       // Penalise quality when temporal variance is very high (unstable signal).
    let stability = (1.0 - scale_invariant_temporal_variance.clamp(0.0, 1.0)).max(0.0);
    let signal_quality = (snr_quality * 0.6 + stability * 0.4).clamp(0.0, 1.0);

    // ── Breathing rate estimation ──
    let breathing_rate_hz = estimate_breathing_rate_hz(frame_history, sample_rate_hz);

    let features = FeatureInfo {
        mean_rssi,
        variance,
        motion_band_power,
        breathing_band_power,
        dominant_freq_hz,
        change_points,
        spectral_power,
    };

    // Return raw motion_score and signal_quality — classification is done by
    // `smooth_and_classify()` which has access to EMA state and hysteresis.
    let raw_classification = ClassificationInfo {
        motion_level: raw_classify(motion_score),
        presence: motion_score > 0.04,
        confidence: (0.4 + signal_quality * 0.3 + motion_score * 0.3).clamp(0.0, 1.0),
    };

    (
        features,
        raw_classification,
        breathing_rate_hz,
        sub_variances,
        motion_score,
    )
}

/// Simple threshold classification (no smoothing) — used as the "raw" input.
fn raw_classify(score: f64) -> String {
    if score > 0.25 {
        "active".into()
    } else if score > 0.12 {
        "present_moving".into()
    } else if score > 0.04 {
        "present_still".into()
    } else {
        "absent".into()
    }
}

fn motion_score_for_level(level: &str) -> f64 {
    match level {
        "active" => 0.8,
        "present_moving" => 0.55,
        "present_still" => 0.3,
        _ => 0.05,
    }
}

fn empty_room_calibration_frame_is_usable(motion_level: &str) -> bool {
    !matches!(motion_level, "active" | "present_moving")
}

fn live_position_presence_gate(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
    d5_fusion: &d5_presence::PresenceFusionState,
) -> position_live::PresenceGate {
    let usable_fingerprints = node_states
        .values()
        .filter(|node| node.d6_fingerprint.evidence_ready(now))
        .count();
    let present_votes = node_states
        .values()
        .filter(|node| node.d6_fingerprint.evidence_ready(now) && node.d6_fingerprint.vote())
        .count();
    map_d6_position_presence_gate(
        d5_fusion.phase(),
        usable_fingerprints,
        present_votes,
        d5_fusion.present(),
    )
}

fn map_d6_position_presence_gate(
    phase: d5_presence::CalibrationPhase,
    usable_fingerprints: usize,
    present_votes: usize,
    persisted_present: bool,
) -> position_live::PresenceGate {
    if phase != d5_presence::CalibrationPhase::Ready {
        return position_live::PresenceGate::Uncalibrated;
    }
    if usable_fingerprints < d5_presence::MIN_FRESH_REFERENCES {
        return position_live::PresenceGate::Insufficient;
    }
    if persisted_present {
        return position_live::PresenceGate::ReadyPresent;
    }
    if present_votes >= d5_presence::REQUIRED_VOTES {
        // A raw D6 quorum still inside the persistence interval is neither a
        // persisted presence nor a safe absence.
        return position_live::PresenceGate::Insufficient;
    }
    position_live::PresenceGate::ReadyAbsent
}

/// Fuse debounced per-node classifications instead of exposing whichever RX
/// happened to deliver the newest UDP frame. Motion needs agreement from at
/// least half of the live links; one noisy receiver cannot flip the room state.
fn aggregate_node_classification(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
    d5_fusion: &mut d5_presence::PresenceFusionState,
) -> ClassificationInfo {
    let live_nodes: Vec<&NodeState> = node_states
        .values()
        .filter(|ns| {
            ns.last_frame_time
                .is_some_and(|seen| now.saturating_duration_since(seen) <= ESP32_OFFLINE_TIMEOUT)
        })
        .collect();

    if live_nodes.is_empty() {
        d5_fusion.update(false, false, now);
        let motion_level = match d5_fusion.phase() {
            d5_presence::CalibrationPhase::Uncalibrated => "absent",
            d5_presence::CalibrationPhase::Collecting => "calibrating",
            d5_presence::CalibrationPhase::Ready => "unknown",
        };
        return ClassificationInfo {
            motion_level: motion_level.to_string(),
            presence: false,
            confidence: 0.0,
        };
    }

    let active = live_nodes
        .iter()
        .filter(|ns| ns.current_motion_level == "active")
        .count();
    let moving = live_nodes
        .iter()
        .filter(|ns| ns.current_motion_level == "present_moving")
        .count();
    let motion_quorum = (live_nodes.len() + 1) / 2;

    let (motion_level, supporters, confidence_denominator) = match d5_fusion.phase() {
        d5_presence::CalibrationPhase::Ready => {
            let usable_nodes: Vec<&NodeState> = live_nodes
                .iter()
                .copied()
                .filter(|ns| ns.d6_fingerprint.evidence_ready(now))
                .collect();
            let votes = usable_nodes
                .iter()
                .filter(|ns| ns.d6_fingerprint.vote())
                .count();
            let evidence_ready = usable_nodes.len() >= d5_presence::MIN_FRESH_REFERENCES;
            let raw_present = votes >= d5_presence::REQUIRED_VOTES;
            let present = d5_fusion.update(raw_present, evidence_ready, now);

            if !evidence_ready {
                ("unknown", 0, 0)
            } else if raw_present && !present {
                // D6 quorum is real but has not yet passed the persistence gate.
                ("unknown", 0, 0)
            } else if present && active >= motion_quorum {
                ("active", active, live_nodes.len())
            } else if present && active + moving >= motion_quorum {
                ("present_moving", active + moving, live_nodes.len())
            } else if present {
                ("present_still", votes, usable_nodes.len())
            } else {
                (
                    "absent",
                    usable_nodes.len().saturating_sub(votes),
                    usable_nodes.len(),
                )
            }
        }
        d5_presence::CalibrationPhase::Collecting => {
            d5_fusion.update(false, false, now);
            if active >= motion_quorum {
                ("active", active, live_nodes.len())
            } else if active + moving >= motion_quorum {
                ("present_moving", active + moving, live_nodes.len())
            } else {
                ("calibrating", 0, 0)
            }
        }
        d5_presence::CalibrationPhase::Uncalibrated => {
            // Backward compatibility before the explicit D6 calibration.
            let still = live_nodes
                .iter()
                .filter(|ns| ns.current_motion_level == "present_still")
                .count();
            if active >= motion_quorum {
                ("active", active, live_nodes.len())
            } else if active + moving >= motion_quorum {
                ("present_moving", active + moving, live_nodes.len())
            } else if active + moving + still > 0 {
                ("present_still", still, live_nodes.len())
            } else {
                ("absent", live_nodes.len(), live_nodes.len())
            }
        }
    };

    ClassificationInfo {
        motion_level: motion_level.to_string(),
        presence: matches!(motion_level, "active" | "present_moving" | "present_still"),
        confidence: if confidence_denominator > 0 {
            supporters as f64 / confidence_denominator as f64
        } else {
            0.0
        },
    }
}

/// A sealed position experiment must not expose legacy D4 presence while its
/// setup-bound D6 empty-room reference is absent or still being collected.
/// Ordinary RuView sessions without a position setup retain the legacy fallback.
fn apply_position_setup_classification_gate(
    position_setup_active: bool,
    phase: d5_presence::CalibrationPhase,
    classification: ClassificationInfo,
) -> ClassificationInfo {
    if !position_setup_active || phase == d5_presence::CalibrationPhase::Ready {
        return classification;
    }

    ClassificationInfo {
        motion_level: match phase {
            d5_presence::CalibrationPhase::Uncalibrated => "uncalibrated",
            d5_presence::CalibrationPhase::Collecting => "calibrating",
            d5_presence::CalibrationPhase::Ready => unreachable!("handled above"),
        }
        .to_string(),
        presence: false,
        confidence: 0.0,
    }
}

/// Debounce frames required before state transition (at ~10 FPS = ~0.4s).
const DEBOUNCE_FRAMES: u32 = 4;
/// EMA alpha for motion smoothing (~1s time constant at 10 FPS).
const MOTION_EMA_ALPHA: f64 = 0.15;
/// Number of warm-up frames before baseline subtraction kicks in.
const BASELINE_WARMUP: u64 = 50;
/// Ignore small frame-to-frame deviations around the learned quiet baseline.
const MOTION_BASELINE_MARGIN: f64 = 0.02;
/// Follow a falling noise floor quickly, but only absorb upward drift slowly so
/// real movement is not learned into the baseline.
const BASELINE_FALL_ALPHA: f64 = 0.05;
const BASELINE_RISE_ALPHA: f64 = 0.001;

fn update_motion_estimate(
    baseline_motion: &mut f64,
    baseline_frames: &mut u64,
    smoothed_motion: &mut f64,
    raw_motion: f64,
) -> f64 {
    *baseline_frames += 1;
    if *baseline_frames < BASELINE_WARMUP {
        *baseline_motion = *baseline_motion * 0.9 + raw_motion * 0.1;
    } else {
        let alpha = if raw_motion < *baseline_motion {
            BASELINE_FALL_ALPHA
        } else {
            BASELINE_RISE_ALPHA
        };
        *baseline_motion += (raw_motion - *baseline_motion) * alpha;
    }

    let available_range = (1.0 - *baseline_motion).max(0.2);
    let adjusted = ((raw_motion - *baseline_motion - MOTION_BASELINE_MARGIN) / available_range)
        .clamp(0.0, 1.0);
    *smoothed_motion = *smoothed_motion * (1.0 - MOTION_EMA_ALPHA) + adjusted * MOTION_EMA_ALPHA;
    *smoothed_motion
}

/// Apply EMA smoothing, adaptive baseline subtraction, and hysteresis debounce
/// to the raw classification.  Mutates the smoothing state in `AppStateInner`.
fn smooth_and_classify(state: &mut AppStateInner, raw: &mut ClassificationInfo, raw_motion: f64) {
    let sm = update_motion_estimate(
        &mut state.baseline_motion,
        &mut state.baseline_frames,
        &mut state.smoothed_motion,
        raw_motion,
    );

    // 4. Classify from smoothed score.
    let candidate = raw_classify(sm);

    // 5. Hysteresis debounce: require N consecutive frames agreeing on a new state.
    if candidate == state.current_motion_level {
        // Already in this state — reset debounce.
        state.debounce_counter = 0;
        state.debounce_candidate = candidate;
    } else if candidate == state.debounce_candidate {
        state.debounce_counter += 1;
        if state.debounce_counter >= DEBOUNCE_FRAMES {
            // Transition accepted.
            state.current_motion_level = candidate;
            state.debounce_counter = 0;
        }
    } else {
        // New candidate — restart counter.
        state.debounce_candidate = candidate;
        state.debounce_counter = 1;
    }

    // 6. Write the smoothed result back into the classification.
    raw.motion_level = state.current_motion_level.clone();
    raw.presence = sm > 0.03;
    raw.confidence = (0.4 + sm * 0.6).clamp(0.0, 1.0);
}

/// Per-node variant of `smooth_and_classify` that operates on a `NodeState`
/// instead of `AppStateInner` (issue #249).
fn smooth_and_classify_node(ns: &mut NodeState, raw: &mut ClassificationInfo, raw_motion: f64) {
    ns.latest_raw_motion = raw_motion;
    let sm = update_motion_estimate(
        &mut ns.baseline_motion,
        &mut ns.baseline_frames,
        &mut ns.smoothed_motion,
        raw_motion,
    );

    let candidate = raw_classify(sm);

    if candidate == ns.current_motion_level {
        ns.debounce_counter = 0;
        ns.debounce_candidate = candidate;
    } else if candidate == ns.debounce_candidate {
        ns.debounce_counter += 1;
        if ns.debounce_counter >= DEBOUNCE_FRAMES {
            ns.current_motion_level = candidate;
            ns.debounce_counter = 0;
        }
    } else {
        ns.debounce_candidate = candidate;
        ns.debounce_counter = 1;
    }

    raw.motion_level = ns.current_motion_level.clone();
    raw.presence = sm > 0.03;
    raw.confidence = (0.4 + sm * 0.6).clamp(0.0, 1.0);
    ns.motion_confidence = raw.confidence;
}

/// Run the shared D4/D5/D6 presence path for one grid-accepted ESP32 frame.
///
/// Keeping this state transition in one function lets offline raw-CSI replay
/// exercise the same motion filtering, calibration rejection, fingerprint
/// windows, and node timing as the UDP live path.
fn observe_frame_for_presence(
    ns: &mut NodeState,
    frame: &Esp32Frame,
    frame_now: std::time::Instant,
    phase: d5_presence::CalibrationPhase,
) -> (FeatureInfo, ClassificationInfo) {
    ns.observe_accepted_csi_frame(frame.sequence, frame_now);
    ns.update_novelty(&frame.amplitudes);

    ns.frame_history.push_back(frame.amplitudes.clone());
    if ns.frame_history.len() > FRAME_HISTORY_CAPACITY {
        ns.frame_history.pop_front();
    }

    let sample_rate_hz = 1000.0 / 500.0_f64;
    let (features, mut classification, _, _, raw_motion) =
        extract_features_from_frame(frame, &ns.frame_history, sample_rate_hz);
    smooth_and_classify_node(ns, &mut classification, raw_motion);

    match phase {
        d5_presence::CalibrationPhase::Collecting => {
            if empty_room_calibration_frame_is_usable(&ns.current_motion_level) {
                ns.d5_presence
                    .observe_calibration(frame_now, ns.smoothed_motion);
                ns.d6_fingerprint
                    .observe_calibration(frame_now, &frame.amplitudes);
            } else {
                ns.calibration_motion_rejected_frames =
                    ns.calibration_motion_rejected_frames.saturating_add(1);
            }
        }
        d5_presence::CalibrationPhase::Ready => {
            ns.d5_presence.observe_live(frame_now, ns.smoothed_motion);
            ns.d6_fingerprint.observe_live(frame_now, &frame.amplitudes);
        }
        d5_presence::CalibrationPhase::Uncalibrated => {}
    }

    (features, classification)
}

const MIN_ADAPTIVE_MODEL_ACCURACY: f64 = 0.70;

fn adaptive_model_is_trusted(training_accuracy: f64) -> bool {
    training_accuracy.is_finite() && training_accuracy >= MIN_ADAPTIVE_MODEL_ACCURACY
}

#[cfg(test)]
mod motion_classification_tests {
    use super::*;

    fn frame(amplitudes: Vec<f64>) -> Esp32Frame {
        Esp32Frame {
            magic: 0xC511_0001,
            node_id: 1,
            n_antennas: 1,
            n_subcarriers: amplitudes.len() as u16,
            freq_mhz: 2437,
            sequence: 1,
            rssi: -50,
            noise_floor: -90,
            ppdu_type: wifi_densepose_hardware::PpduType::HtLegacy,
            phases: vec![0.0; amplitudes.len()],
            amplitudes,
        }
    }

    #[test]
    fn static_spatial_pattern_is_not_motion() {
        let amplitudes = vec![10.0, 30.0, 10.0, 30.0];
        let history = VecDeque::from([amplitudes.clone(), amplitudes.clone()]);

        let (_, _, _, _, motion) = extract_features_from_frame(&frame(amplitudes), &history, 10.0);

        assert!(motion < 1e-9, "static frame scored as motion: {motion}");
    }

    #[test]
    fn uniform_frame_gain_change_is_not_motion() {
        let previous = vec![10.0, 30.0, 15.0, 25.0];
        let current: Vec<f64> = previous.iter().map(|amplitude| amplitude * 1.689).collect();
        let history = VecDeque::from([previous, current.clone()]);

        let (_, _, _, _, motion) = extract_features_from_frame(&frame(current), &history, 10.0);

        assert!(
            motion < 1e-8,
            "uniform frame gain change scored as motion: {motion}"
        );
    }

    #[test]
    fn temporal_change_produces_motion() {
        let previous = vec![10.0, 30.0, 10.0, 30.0];
        let current = vec![30.0, 10.0, 30.0, 10.0];
        let history = VecDeque::from([previous, current.clone()]);

        let (_, _, _, _, motion) = extract_features_from_frame(&frame(current), &history, 10.0);

        assert!(motion > 0.6, "temporal change scored too low: {motion}");
    }

    #[test]
    fn subcarrier_shape_change_survives_gain_normalisation() {
        let previous = vec![10.0, 30.0, 10.0, 30.0];
        let current: Vec<f64> = vec![30.0, 10.0, 30.0, 10.0]
            .into_iter()
            .map(|amplitude| amplitude * 1.689)
            .collect();
        let history = VecDeque::from([previous, current.clone()]);

        let (_, _, _, _, motion) = extract_features_from_frame(&frame(current), &history, 10.0);

        assert!(
            motion > 0.6,
            "subcarrier shape change was removed with frame gain: {motion}"
        );
    }

    #[test]
    fn learned_quiet_baseline_suppresses_noise_but_not_motion_spike() {
        let mut baseline = 0.0;
        let mut frames = 0;
        let mut smoothed = 0.0;

        for _ in 0..100 {
            update_motion_estimate(&mut baseline, &mut frames, &mut smoothed, 0.5);
        }
        assert!(smoothed < 0.04, "quiet baseline leaked motion: {smoothed}");

        for _ in 0..6 {
            update_motion_estimate(&mut baseline, &mut frames, &mut smoothed, 0.9);
        }
        assert!(smoothed > 0.25, "motion spike was absorbed: {smoothed}");
    }

    fn classified_node(level: &str, now: std::time::Instant) -> NodeState {
        let mut node = NodeState::new();
        node.current_motion_level = level.to_string();
        node.last_frame_time = Some(now);
        node
    }

    fn complete_source_binding(hex: char) -> raw_csi_recording::SourceBinding {
        raw_csi_recording::SourceBinding {
            trailer_version: raw_csi_recording::TX_SOURCE_BINDING_VERSION,
            flags: raw_csi_recording::SOURCE_BINDING_REQUIRED_FLAGS,
            scheme: raw_csi_recording::TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: hex.to_string().repeat(64),
        }
    }

    fn observe_complete_binding(
        node: &mut NodeState,
        now: std::time::Instant,
        matches_setup: bool,
        hex: char,
    ) {
        node.observe_source_binding(Some(
            SourceBindingObservation::validated(&complete_source_binding(hex), now, matches_setup)
                .unwrap(),
        ));
    }

    fn d5_voting_node(vote: bool, now: std::time::Instant) -> NodeState {
        d5_voting_node_with_interval(vote, now, std::time::Duration::from_millis(100))
    }

    fn d5_voting_node_with_interval(
        vote: bool,
        now: std::time::Instant,
        interval: std::time::Duration,
    ) -> NodeState {
        let mut node = classified_node("absent", now);
        node.d5_presence.install_reference_for_test(0.01, 0.005);
        let empty_shape = [10.0, 30.0, 10.0, 30.0];
        let occupied_shape = [30.0, 10.0, 30.0, 10.0];
        node.d6_fingerprint
            .install_reference_for_test(&empty_shape)
            .unwrap();
        let sample_count = (d5_presence::LIVE_WINDOW.as_nanos() / interval.as_nanos()) as u64;
        let started = now - d5_presence::LIVE_WINDOW;
        for sample in 0..=sample_count {
            let timestamp = started + interval.mul_f64(sample as f64);
            node.d5_presence
                .observe_live(timestamp, if vote { 0.03 } else { 0.01 });
            node.d6_fingerprint
                .observe_live(timestamp, if vote { &occupied_shape } else { &empty_shape });
        }
        node
    }

    #[test]
    fn one_noisy_receiver_cannot_flip_room_to_moving() {
        let now = std::time::Instant::now();
        let nodes = HashMap::from([
            (1, classified_node("present_moving", now)),
            (2, classified_node("present_still", now)),
            (3, classified_node("present_still", now)),
            (4, classified_node("present_still", now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();

        let result = aggregate_node_classification(&nodes, now, &mut d5);

        assert_eq!(result.motion_level, "present_still");
        assert_eq!(result.confidence, 0.75);
    }

    #[test]
    fn two_receivers_are_motion_quorum_for_four_links() {
        let now = std::time::Instant::now();
        let nodes = HashMap::from([
            (1, classified_node("present_moving", now)),
            (2, classified_node("present_moving", now)),
            (3, classified_node("present_still", now)),
            (4, classified_node("present_still", now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();

        let result = aggregate_node_classification(&nodes, now, &mut d5);

        assert_eq!(result.motion_level, "present_moving");
        assert_eq!(result.confidence, 0.5);
    }

    #[test]
    fn d5_requires_two_fresh_rx_votes_and_time_persistence() {
        let now = std::time::Instant::now();
        let nodes = HashMap::from([
            (1, d5_voting_node(true, now)),
            (2, d5_voting_node(true, now)),
            (3, d5_voting_node(false, now)),
            (4, d5_voting_node(false, now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);

        let initial = aggregate_node_classification(&nodes, now, &mut d5);
        let persisted =
            aggregate_node_classification(&nodes, now + d5_presence::STATE_PERSISTENCE, &mut d5);

        assert_eq!(initial.motion_level, "unknown");
        assert_eq!(persisted.motion_level, "present_still");
        assert_eq!(persisted.confidence, 0.5);
    }

    #[test]
    fn live_position_gate_uses_only_ready_persisted_d6_evidence() {
        use position_live::PresenceGate;

        assert_eq!(
            map_d6_position_presence_gate(d5_presence::CalibrationPhase::Uncalibrated, 4, 4, true,),
            PresenceGate::Uncalibrated
        );
        assert_eq!(
            map_d6_position_presence_gate(d5_presence::CalibrationPhase::Collecting, 4, 4, true,),
            PresenceGate::Uncalibrated
        );
        assert_eq!(
            map_d6_position_presence_gate(d5_presence::CalibrationPhase::Ready, 1, 1, false),
            PresenceGate::Insufficient
        );
        assert_eq!(
            map_d6_position_presence_gate(d5_presence::CalibrationPhase::Ready, 4, 2, false),
            PresenceGate::Insufficient,
            "a raw quorum inside the persistence interval is not safe presence or absence"
        );
        assert_eq!(
            map_d6_position_presence_gate(d5_presence::CalibrationPhase::Ready, 4, 2, true),
            PresenceGate::ReadyPresent
        );
        assert_eq!(
            map_d6_position_presence_gate(d5_presence::CalibrationPhase::Ready, 4, 1, false),
            PresenceGate::ReadyAbsent
        );
    }

    #[test]
    fn ready_d6_empty_room_cannot_be_overridden_by_two_noisy_d4_receivers() {
        let now = std::time::Instant::now();
        let mut nodes = HashMap::from([
            (1, d5_voting_node(false, now)),
            (2, d5_voting_node(false, now)),
            (3, d5_voting_node(false, now)),
            (4, d5_voting_node(false, now)),
        ]);
        nodes.get_mut(&1).unwrap().current_motion_level = "active".to_string();
        nodes.get_mut(&2).unwrap().current_motion_level = "present_moving".to_string();
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);

        let result = aggregate_node_classification(&nodes, now, &mut d5);

        assert_eq!(result.motion_level, "absent");
        assert!(!result.presence);
    }

    #[test]
    fn d4_refines_a_persisted_d6_presence_to_moving() {
        let now = std::time::Instant::now();
        let mut nodes = HashMap::from([
            (1, d5_voting_node(true, now)),
            (2, d5_voting_node(true, now)),
            (3, d5_voting_node(false, now)),
            (4, d5_voting_node(false, now)),
        ]);
        nodes.get_mut(&1).unwrap().current_motion_level = "present_moving".to_string();
        nodes.get_mut(&2).unwrap().current_motion_level = "present_moving".to_string();
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);

        assert_eq!(
            aggregate_node_classification(&nodes, now, &mut d5).motion_level,
            "unknown"
        );
        let result =
            aggregate_node_classification(&nodes, now + d5_presence::STATE_PERSISTENCE, &mut d5);

        assert_eq!(result.motion_level, "present_moving");
        assert!(result.presence);
    }

    #[test]
    fn one_drifting_rx_cannot_trigger_d5_presence() {
        let now = std::time::Instant::now();
        let nodes = HashMap::from([
            (1, d5_voting_node(true, now)),
            (2, d5_voting_node(false, now)),
            (3, d5_voting_node(false, now)),
            (4, d5_voting_node(false, now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);

        aggregate_node_classification(&nodes, now, &mut d5);
        let result =
            aggregate_node_classification(&nodes, now + d5_presence::STATE_PERSISTENCE, &mut d5);

        assert_eq!(result.motion_level, "absent");
        assert!(!result.presence);
    }

    #[test]
    fn low_rate_rx_vote_is_excluded_from_d5_quorum() {
        let now = std::time::Instant::now();
        let low_rate_voter =
            d5_voting_node_with_interval(true, now, std::time::Duration::from_secs(1));
        let nodes = HashMap::from([
            (1, d5_voting_node(true, now)),
            (2, low_rate_voter),
            (3, d5_voting_node(false, now)),
            (4, d5_voting_node(false, now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);

        aggregate_node_classification(&nodes, now, &mut d5);
        let result =
            aggregate_node_classification(&nodes, now + d5_presence::STATE_PERSISTENCE, &mut d5);

        assert_eq!(result.motion_level, "absent");
        assert!(!result.presence);
    }

    #[test]
    fn stale_d5_vote_is_excluded_even_when_general_node_liveness_is_fresh() {
        let now = std::time::Instant::now();
        let stale_at =
            now - d5_presence::OBSERVATION_FRESHNESS - std::time::Duration::from_millis(1);
        let mut stale_voter = d5_voting_node(true, stale_at);
        stale_voter.last_frame_time = Some(now);
        stale_voter.csi_fps_samples = 10;
        stale_voter.csi_fps_ema = 30.0;
        let nodes = HashMap::from([
            (1, d5_voting_node(true, now)),
            (2, stale_voter),
            (3, d5_voting_node(false, now)),
            (4, d5_voting_node(false, now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);

        aggregate_node_classification(&nodes, now, &mut d5);
        let result =
            aggregate_node_classification(&nodes, now + d5_presence::STATE_PERSISTENCE, &mut d5);

        assert_eq!(result.motion_level, "absent");
        assert!(!result.presence);
    }

    #[test]
    fn per_node_classification_uses_d5_after_calibration() {
        let now = std::time::Instant::now();
        let mut node = d5_voting_node(false, now);
        node.current_motion_level = "active".to_string();
        let nodes = HashMap::from([(1, node)]);

        let entries =
            build_node_features(&nodes, now, d5_presence::CalibrationPhase::Ready, true).unwrap();

        assert_eq!(entries[0].classification.motion_level, "absent");
        assert!(!entries[0].classification.presence);
        assert_eq!(entries[0].d4_classification.motion_level, "active");
        assert!(entries[0].d4_classification.presence);
    }

    #[test]
    fn active_position_setup_blocks_d4_presence_until_d6_is_ready() {
        let legacy_presence = ClassificationInfo {
            motion_level: "present_still".to_string(),
            presence: true,
            confidence: 0.9,
        };

        let uncalibrated = apply_position_setup_classification_gate(
            true,
            d5_presence::CalibrationPhase::Uncalibrated,
            legacy_presence.clone(),
        );
        assert_eq!(uncalibrated.motion_level, "uncalibrated");
        assert!(!uncalibrated.presence);
        assert_eq!(uncalibrated.confidence, 0.0);

        let collecting = apply_position_setup_classification_gate(
            true,
            d5_presence::CalibrationPhase::Collecting,
            legacy_presence.clone(),
        );
        assert_eq!(collecting.motion_level, "calibrating");
        assert!(!collecting.presence);
        assert_eq!(collecting.confidence, 0.0);

        let ready = apply_position_setup_classification_gate(
            true,
            d5_presence::CalibrationPhase::Ready,
            legacy_presence.clone(),
        );
        assert_eq!(ready.motion_level, "present_still");
        assert!(ready.presence);

        let ordinary_ruview = apply_position_setup_classification_gate(
            false,
            d5_presence::CalibrationPhase::Uncalibrated,
            legacy_presence,
        );
        assert_eq!(ordinary_ruview.motion_level, "present_still");
        assert!(ordinary_ruview.presence);
    }

    #[test]
    fn public_nodes_fail_closed_until_position_setup_is_calibrated() {
        let now = std::time::Instant::now();
        let mut node = classified_node("active", now);
        node.motion_confidence = 0.9;
        node.prev_person_count = 2;
        let nodes = HashMap::from([(1, node)]);

        let setup_nodes = public_node_summaries(
            &nodes,
            now,
            d5_presence::CalibrationPhase::Uncalibrated,
            true,
        );
        assert_eq!(setup_nodes[0]["motion_level"], "uncalibrated");
        assert_eq!(setup_nodes[0]["person_count"], 0);

        let legacy_nodes = public_node_summaries(
            &nodes,
            now,
            d5_presence::CalibrationPhase::Uncalibrated,
            false,
        );
        assert_eq!(legacy_nodes[0]["motion_level"], "active");
        assert_eq!(legacy_nodes[0]["person_count"], 2);
    }

    #[test]
    fn public_nodes_expose_only_fresh_setup_binding_booleans() {
        let now = std::time::Instant::now();
        let mut node = classified_node("absent", now);
        observe_complete_binding(&mut node, now, true, 'a');
        node.skipped_grid_frames = 3;
        let nodes = HashMap::from([(1, node)]);

        let fresh = public_node_summaries(&nodes, now, d5_presence::CalibrationPhase::Ready, true);
        for field in [
            "source_binding_attested",
            "filter_enforced",
            "source_matched_filter",
            "identity_valid",
            "identity_matches_setup",
        ] {
            assert_eq!(fresh[0][field], true, "fresh field {field}");
        }
        assert_eq!(fresh[0]["binding_last_seen_ms"], 0);
        assert_eq!(fresh[0]["skipped_grid_frames"], 3);
        assert!(fresh[0].get("tx_filter_sha256").is_none());
        assert!(fresh[0].get("filter_mac").is_none());
        assert!(fresh[0].get("source_mac").is_none());

        let stale = public_node_summaries(
            &nodes,
            now + SOURCE_BINDING_FRESHNESS_TIMEOUT + std::time::Duration::from_millis(1),
            d5_presence::CalibrationPhase::Ready,
            true,
        );
        for field in [
            "source_binding_attested",
            "filter_enforced",
            "source_matched_filter",
            "identity_valid",
            "identity_matches_setup",
        ] {
            assert_eq!(stale[0][field], false, "stale field {field}");
        }
        assert_eq!(
            stale[0]["binding_last_seen_ms"],
            SOURCE_BINDING_FRESHNESS_TIMEOUT.as_millis() as u64 + 1
        );

        let no_setup =
            public_node_summaries(&nodes, now, d5_presence::CalibrationPhase::Ready, false);
        assert_eq!(no_setup[0]["source_binding_attested"], true);
        assert_eq!(no_setup[0]["identity_matches_setup"], false);
        assert_eq!(no_setup[0]["binding_last_seen_ms"], 0);
    }

    #[test]
    fn rejecting_source_binding_does_not_mutate_presence_or_liveness_state() {
        let now = std::time::Instant::now();
        let mut node = classified_node("present_still", now);
        node.frame_history.push_back(vec![1.0, 2.0, 3.0]);
        node.baseline_motion = 0.125;
        observe_complete_binding(&mut node, now, true, 'a');

        node.invalidate_source_binding_attestation();

        assert_eq!(node.last_frame_time, Some(now));
        assert_eq!(node.current_motion_level, "present_still");
        assert_eq!(node.frame_history, VecDeque::from([vec![1.0, 2.0, 3.0]]));
        assert_eq!(node.baseline_motion, 0.125);
        assert!(node.source_binding_observation.is_none());
    }

    #[test]
    fn discovery_binding_consistency_requires_fresh_matching_rx1_through_rx4() {
        let now = std::time::Instant::now();
        let mut nodes = HashMap::new();
        for rx_id in 1..=4 {
            let mut node = classified_node("absent", now);
            observe_complete_binding(&mut node, now, false, 'a');
            nodes.insert(rx_id, node);
        }
        assert!(source_binding_consistent_across_nodes(&nodes, now));

        observe_complete_binding(nodes.get_mut(&4).unwrap(), now, false, 'b');
        assert!(!source_binding_consistent_across_nodes(&nodes, now));

        observe_complete_binding(nodes.get_mut(&4).unwrap(), now, false, 'a');
        let stale_now = now + SOURCE_BINDING_FRESHNESS_TIMEOUT + Duration::from_millis(1);
        assert!(!source_binding_consistent_across_nodes(&nodes, stale_now));
    }

    #[test]
    fn public_edge_vitals_fail_closed_until_position_setup_is_calibrated() {
        let vitals = Esp32VitalsPacket {
            node_id: 4,
            presence: true,
            fall_detected: true,
            motion: true,
            breathing_rate_bpm: 16.0,
            heartrate_bpm: 72.0,
            rssi: -48,
            n_persons: 2,
            motion_energy: 0.8,
            presence_score: 0.9,
            timestamp_ms: 123,
        };
        let classification = apply_position_setup_classification_gate(
            true,
            d5_presence::CalibrationPhase::Uncalibrated,
            edge_vitals_classification(&vitals),
        );

        let public = public_edge_vitals_packet(&vitals, &classification);

        assert!(!public.presence);
        assert!(!public.motion);
        assert!(!public.fall_detected);
        assert_eq!(public.n_persons, 0);
        assert_eq!(public.presence_score, 0.0);
        assert_eq!(public.breathing_rate_bpm, 0.0);
        assert_eq!(public.heartrate_bpm, 0.0);
    }

    #[test]
    fn sealed_position_setup_ignores_edge_vitals_measurement_input() {
        assert!(!edge_vitals_measurement_input_allowed(true));
    }

    #[test]
    fn ordinary_session_keeps_edge_vitals_fallback() {
        assert!(edge_vitals_measurement_input_allowed(false));
    }

    #[test]
    fn calibration_status_never_claims_legacy_d4_for_a_position_setup() {
        assert_eq!(
            classification_decision_status(d5_presence::CalibrationPhase::Uncalibrated, true, 0,),
            "uncalibrated"
        );
        assert_eq!(
            classification_decision_status(d5_presence::CalibrationPhase::Uncalibrated, false, 0,),
            "legacy_d4"
        );
        assert_eq!(
            classification_decision_status(d5_presence::CalibrationPhase::Collecting, true, 0,),
            "calibrating"
        );
        assert_eq!(
            classification_decision_status(
                d5_presence::CalibrationPhase::Ready,
                true,
                d5_presence::MIN_FRESH_REFERENCES,
            ),
            "operational"
        );
    }

    #[test]
    fn losing_all_nodes_clears_a_latched_d5_presence() {
        let now = std::time::Instant::now();
        let nodes = HashMap::from([
            (1, d5_voting_node(true, now)),
            (2, d5_voting_node(true, now)),
            (3, d5_voting_node(false, now)),
        ]);
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.mark_ready_for_test(now);
        aggregate_node_classification(&nodes, now, &mut d5);
        assert_eq!(
            aggregate_node_classification(&nodes, now + d5_presence::STATE_PERSISTENCE, &mut d5,)
                .motion_level,
            "present_still"
        );

        let result = aggregate_node_classification(
            &HashMap::new(),
            now + d5_presence::STATE_PERSISTENCE + std::time::Duration::from_secs(1),
            &mut d5,
        );

        assert_eq!(result.motion_level, "unknown");
        assert!(!result.presence);
        assert!(!d5.present());
    }

    #[test]
    fn d5_calibration_collecting_is_fail_closed_for_still_presence() {
        let now = std::time::Instant::now();
        let nodes = HashMap::from([(1, classified_node("present_still", now))]);
        let mut d5 = d5_presence::PresenceFusionState::default();
        d5.start_calibration(now).unwrap();

        let result = aggregate_node_classification(&nodes, now, &mut d5);

        assert_eq!(result.motion_level, "calibrating");
        assert!(!result.presence);
    }

    #[test]
    fn moving_frames_are_rejected_from_empty_room_calibration() {
        assert!(empty_room_calibration_frame_is_usable("absent"));
        assert!(empty_room_calibration_frame_is_usable("present_still"));
        assert!(!empty_room_calibration_frame_is_usable("present_moving"));
        assert!(!empty_room_calibration_frame_is_usable("active"));
    }

    #[test]
    fn moving_level_has_more_field_energy_than_still() {
        assert!(motion_score_for_level("present_moving") > motion_score_for_level("present_still"));
    }

    #[test]
    fn low_accuracy_adaptive_model_is_rejected() {
        assert!(!adaptive_model_is_trusted(0.415));
        assert!(adaptive_model_is_trusted(0.70));
    }
}

/// If an adaptive model is loaded, override the classification with the
/// model's prediction.  Uses the full 15-feature vector for higher accuracy.
fn adaptive_override(
    state: &AppStateInner,
    features: &FeatureInfo,
    classification: &mut ClassificationInfo,
) {
    if let Some(ref model) = state.adaptive_model {
        // Get current frame amplitudes from the latest history entry.
        let amps = state
            .frame_history
            .back()
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let feat_arr = adaptive_classifier::features_from_runtime(
            &serde_json::json!({
                "variance": features.variance,
                "motion_band_power": features.motion_band_power,
                "breathing_band_power": features.breathing_band_power,
                "spectral_power": features.spectral_power,
                "dominant_freq_hz": features.dominant_freq_hz,
                "change_points": features.change_points,
                "mean_rssi": features.mean_rssi,
            }),
            amps,
        );
        let (label, conf) = model.classify(&feat_arr);
        classification.motion_level = label.to_string();
        classification.presence = label != "absent";
        // Blend model confidence with existing smoothed confidence.
        classification.confidence = (conf * 0.7 + classification.confidence * 0.3).clamp(0.0, 1.0);
    }
}

/// Size of the median filter window for vital signs outlier rejection.
const VITAL_MEDIAN_WINDOW: usize = 21;
/// EMA alpha for vital signs (~5s time constant at 10 FPS).
const VITAL_EMA_ALPHA: f64 = 0.02;
/// Maximum BPM jump per frame before a value is rejected as an outlier.
const HR_MAX_JUMP: f64 = 8.0;
const BR_MAX_JUMP: f64 = 2.0;
/// Minimum change from current smoothed value before EMA updates (dead-band).
/// Prevents micro-drift from creeping in.
const HR_DEAD_BAND: f64 = 2.0;
const BR_DEAD_BAND: f64 = 0.5;

/// Smooth vital signs using median-filter outlier rejection + EMA.
/// Mutates `state.smoothed_hr`, `state.smoothed_br`, etc.
/// Returns the smoothed VitalSigns to broadcast.
fn smooth_vitals(state: &mut AppStateInner, raw: &VitalSigns) -> VitalSigns {
    let raw_hr = raw.heart_rate_bpm.unwrap_or(0.0);
    let raw_br = raw.breathing_rate_bpm.unwrap_or(0.0);

    // -- Outlier rejection: skip values that jump too far from current EMA --
    let hr_ok = state.smoothed_hr < 1.0 || (raw_hr - state.smoothed_hr).abs() < HR_MAX_JUMP;
    let br_ok = state.smoothed_br < 1.0 || (raw_br - state.smoothed_br).abs() < BR_MAX_JUMP;

    // Push into buffer (only non-outlier values)
    if hr_ok && raw_hr > 0.0 {
        state.hr_buffer.push_back(raw_hr);
        if state.hr_buffer.len() > VITAL_MEDIAN_WINDOW {
            state.hr_buffer.pop_front();
        }
    }
    if br_ok && raw_br > 0.0 {
        state.br_buffer.push_back(raw_br);
        if state.br_buffer.len() > VITAL_MEDIAN_WINDOW {
            state.br_buffer.pop_front();
        }
    }

    // Compute trimmed mean: drop top/bottom 25% then average the middle 50%.
    // This is more stable than pure median and less noisy than raw mean.
    let trimmed_hr = trimmed_mean(&state.hr_buffer);
    let trimmed_br = trimmed_mean(&state.br_buffer);

    // EMA smooth with dead-band: only update if the trimmed mean differs
    // from the current smoothed value by more than the dead-band.
    // This prevents the display from constantly creeping by tiny amounts.
    if trimmed_hr > 0.0 {
        if state.smoothed_hr < 1.0 {
            state.smoothed_hr = trimmed_hr;
        } else if (trimmed_hr - state.smoothed_hr).abs() > HR_DEAD_BAND {
            state.smoothed_hr =
                state.smoothed_hr * (1.0 - VITAL_EMA_ALPHA) + trimmed_hr * VITAL_EMA_ALPHA;
        }
        // else: within dead-band, hold current value
    }
    if trimmed_br > 0.0 {
        if state.smoothed_br < 1.0 {
            state.smoothed_br = trimmed_br;
        } else if (trimmed_br - state.smoothed_br).abs() > BR_DEAD_BAND {
            state.smoothed_br =
                state.smoothed_br * (1.0 - VITAL_EMA_ALPHA) + trimmed_br * VITAL_EMA_ALPHA;
        }
    }

    // Smooth confidence
    state.smoothed_hr_conf = state.smoothed_hr_conf * 0.92 + raw.heartbeat_confidence * 0.08;
    state.smoothed_br_conf = state.smoothed_br_conf * 0.92 + raw.breathing_confidence * 0.08;

    VitalSigns {
        breathing_rate_bpm: if state.smoothed_br > 1.0 {
            Some(state.smoothed_br)
        } else {
            None
        },
        heart_rate_bpm: if state.smoothed_hr > 1.0 {
            Some(state.smoothed_hr)
        } else {
            None
        },
        breathing_confidence: state.smoothed_br_conf,
        heartbeat_confidence: state.smoothed_hr_conf,
        signal_quality: raw.signal_quality,
    }
}

/// Per-node variant of `smooth_vitals` that operates on a `NodeState` (issue #249).
fn smooth_vitals_node(ns: &mut NodeState, raw: &VitalSigns) -> VitalSigns {
    let raw_hr = raw.heart_rate_bpm.unwrap_or(0.0);
    let raw_br = raw.breathing_rate_bpm.unwrap_or(0.0);

    let hr_ok = ns.smoothed_hr < 1.0 || (raw_hr - ns.smoothed_hr).abs() < HR_MAX_JUMP;
    let br_ok = ns.smoothed_br < 1.0 || (raw_br - ns.smoothed_br).abs() < BR_MAX_JUMP;

    if hr_ok && raw_hr > 0.0 {
        ns.hr_buffer.push_back(raw_hr);
        if ns.hr_buffer.len() > VITAL_MEDIAN_WINDOW {
            ns.hr_buffer.pop_front();
        }
    }
    if br_ok && raw_br > 0.0 {
        ns.br_buffer.push_back(raw_br);
        if ns.br_buffer.len() > VITAL_MEDIAN_WINDOW {
            ns.br_buffer.pop_front();
        }
    }

    let trimmed_hr = trimmed_mean(&ns.hr_buffer);
    let trimmed_br = trimmed_mean(&ns.br_buffer);

    if trimmed_hr > 0.0 {
        if ns.smoothed_hr < 1.0 {
            ns.smoothed_hr = trimmed_hr;
        } else if (trimmed_hr - ns.smoothed_hr).abs() > HR_DEAD_BAND {
            ns.smoothed_hr =
                ns.smoothed_hr * (1.0 - VITAL_EMA_ALPHA) + trimmed_hr * VITAL_EMA_ALPHA;
        }
    }
    if trimmed_br > 0.0 {
        if ns.smoothed_br < 1.0 {
            ns.smoothed_br = trimmed_br;
        } else if (trimmed_br - ns.smoothed_br).abs() > BR_DEAD_BAND {
            ns.smoothed_br =
                ns.smoothed_br * (1.0 - VITAL_EMA_ALPHA) + trimmed_br * VITAL_EMA_ALPHA;
        }
    }

    ns.smoothed_hr_conf = ns.smoothed_hr_conf * 0.92 + raw.heartbeat_confidence * 0.08;
    ns.smoothed_br_conf = ns.smoothed_br_conf * 0.92 + raw.breathing_confidence * 0.08;

    VitalSigns {
        breathing_rate_bpm: if ns.smoothed_br > 1.0 {
            Some(ns.smoothed_br)
        } else {
            None
        },
        heart_rate_bpm: if ns.smoothed_hr > 1.0 {
            Some(ns.smoothed_hr)
        } else {
            None
        },
        breathing_confidence: ns.smoothed_br_conf,
        heartbeat_confidence: ns.smoothed_hr_conf,
        signal_quality: raw.signal_quality,
    }
}

/// Trimmed mean: sort, drop top/bottom 25%, average the middle 50%.
/// More robust than median (uses more data) and less noisy than raw mean.
fn trimmed_mean(buf: &VecDeque<f64>) -> f64 {
    if buf.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = buf.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let trim = n / 4; // drop 25% from each end
    let middle = &sorted[trim..n - trim.max(0)];
    if middle.is_empty() {
        sorted[n / 2] // fallback to median if too few samples
    } else {
        middle.iter().sum::<f64>() / middle.len() as f64
    }
}

// ── Windows WiFi RSSI collector ──────────────────────────────────────────────

/// Parse `netsh wlan show interfaces` output for RSSI and signal quality
fn parse_netsh_interfaces_output(output: &str) -> Option<(f64, f64, String)> {
    let mut rssi = None;
    let mut signal = None;
    let mut ssid = None;

    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("Signal") {
            // "Signal                 : 89%"
            if let Some(pct) = line.split(':').nth(1) {
                let pct = pct.trim().trim_end_matches('%');
                if let Ok(v) = pct.parse::<f64>() {
                    signal = Some(v);
                    // Convert signal% to approximate dBm: -100 + (signal% * 0.6)
                    rssi = Some(-100.0 + v * 0.6);
                }
            }
        }
        if line.starts_with("SSID") && !line.starts_with("BSSID") {
            if let Some(s) = line.split(':').nth(1) {
                ssid = Some(s.trim().to_string());
            }
        }
    }

    match (rssi, signal, ssid) {
        (Some(r), Some(_s), Some(name)) => Some((r, _s, name)),
        (Some(r), Some(_s), None) => Some((r, _s, "Unknown".into())),
        _ => None,
    }
}

// ── Source resolution state machine (issue #1004) ────────────────────────────

/// What background tasks to start, derived from `--source` and the boot probes.
///
/// Issue #1004: a one-shot startup probe latched `auto` to `simulate` forever
/// when no CSI happened to be flowing at boot (the normal case — the firmware
/// and the server race to come up). The UDP :5005 receiver was then never
/// bound, so real CSI arriving seconds later was silently ignored and the
/// server served simulated poses for the rest of the process. The UI looked
/// live; the data was fake. This is the exact "where's the real data?" failure
/// class the project fights.
///
/// The robust resolution: in `auto` mode **always bind the UDP receiver**
/// regardless of the boot probe. If no real source is up yet, keep the server
/// explicitly offline while listening; the receiver promotes `source` →
/// `esp32` the instant the first real frame lands (see `udp_receiver_task`,
/// which sets `s.source = "esp32"`), mirroring the inverse `esp32 →
/// esp32:offline` reversion already in `effective_source()`.
///
/// Explicit `--source simulated` is a hard override for offline demos: it does
/// NOT bind UDP, so no promotion ever happens.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourcePlan {
    /// The `AppStateInner.source` value to start with.
    initial_source: String,
    /// Bind the UDP :5005 receiver (and thus allow offline→esp32 promotion).
    bind_udp: bool,
    /// Run the simulated-data generator for an explicit offline demo.
    run_simulator: bool,
    /// Run the Windows WiFi capture task.
    run_wifi: bool,
}

/// Pure decision function — fully unit-testable without binding sockets.
///
/// `requested` is the normalized `--source` value. `esp32_detected` /
/// `wifi_detected` are the boot-probe results (only consulted in `auto` mode).
/// Returns `None` for an unknown source that names neither a real source nor a
/// simulate alias (the caller maps that to its own pass-through/exit policy).
fn plan_source(requested: &str, esp32_detected: bool, wifi_detected: bool) -> SourcePlan {
    match requested {
        "auto" => {
            if esp32_detected {
                // Real CSI already flowing — bind UDP, no simulator.
                SourcePlan {
                    initial_source: "esp32".to_string(),
                    bind_udp: true,
                    run_simulator: false,
                    run_wifi: false,
                }
            } else if wifi_detected {
                SourcePlan {
                    initial_source: "wifi".to_string(),
                    bind_udp: false,
                    run_simulator: false,
                    run_wifi: true,
                }
            } else {
                // No real source *yet*. Stay offline, but bind UDP so the
                // receiver can promote to esp32 when the first real frame
                // arrives (issue #1004). Never show synthetic frames in auto.
                SourcePlan {
                    initial_source: "esp32".to_string(),
                    bind_udp: true,
                    run_simulator: false,
                    run_wifi: false,
                }
            }
        }
        // Explicit overrides. "simulate" is a back-compat alias for "simulated".
        "simulate" | "simulated" => SourcePlan {
            initial_source: "simulated".to_string(),
            bind_udp: false, // hard override: offline demo, no live promotion
            run_simulator: true,
            run_wifi: false,
        },
        "esp32" => SourcePlan {
            initial_source: "esp32".to_string(),
            bind_udp: true,
            run_simulator: false,
            run_wifi: false,
        },
        "wifi" => SourcePlan {
            initial_source: "wifi".to_string(),
            bind_udp: false,
            run_simulator: false,
            run_wifi: true,
        },
        // Unknown source — preserve it verbatim, no tasks (caller's policy).
        other => SourcePlan {
            initial_source: other.to_string(),
            bind_udp: false,
            run_simulator: false,
            run_wifi: false,
        },
    }
}

#[cfg(test)]
mod issue_1004_source_plan_tests {
    //! Issue #1004 — `--source auto` must NOT latch on `simulate` forever.
    //!
    //! Old behavior: a one-shot boot probe resolved the source once. With no CSI
    //! flowing at boot (the normal case), the server either latched on simulate
    //! (never binding UDP :5005, so later real CSI was silently ignored) or
    //! hard-exited (#937), never picking up CSI that started after launch.
    //!
    //! New behavior (`plan_source`): in `auto` the UDP receiver is ALWAYS bound,
    //! no synthetic data is served while waiting, and
    //! `udp_receiver_task` promotes the offline `source` → "esp32" on the first
    //! real frame. These tests pin the resolution/promotion state machine
    //! directly (no sockets bound).
    use super::*;

    // FAILS ON OLD CODE: the old `auto`-with-no-source path bound no UDP
    // receiver (it spawned only `simulated_data_task`, or exited). This asserts
    // UDP IS bound even when the boot probe finds no source.
    #[test]
    fn auto_with_no_boot_source_still_binds_udp_without_synthetic_data() {
        let plan = plan_source("auto", false, false);
        assert!(
            plan.bind_udp,
            "auto must bind UDP :5005 even with no boot source (#1004)"
        );
        assert!(!plan.run_simulator, "auto must not emit synthetic frames");
        assert!(!plan.run_wifi);
        assert_eq!(plan.initial_source, "esp32");
    }

    #[test]
    fn auto_with_esp32_detected_binds_udp_no_simulator() {
        let plan = plan_source("auto", true, false);
        assert!(plan.bind_udp);
        assert!(
            !plan.run_simulator,
            "real CSI present → no synthetic frames"
        );
        assert_eq!(plan.initial_source, "esp32");
    }

    #[test]
    fn auto_with_wifi_detected_runs_wifi_no_udp() {
        let plan = plan_source("auto", false, true);
        assert!(plan.run_wifi);
        assert!(!plan.bind_udp);
        assert!(!plan.run_simulator);
        assert_eq!(plan.initial_source, "wifi");
    }

    // Explicit `--source simulated` is a hard offline override: it must NOT bind
    // UDP (so it can never be promoted to live), distinguishing it from
    // auto-mode simulate.
    #[test]
    fn explicit_simulated_is_offline_override_no_udp() {
        for s in ["simulated", "simulate"] {
            let plan = plan_source(s, false, false);
            assert!(
                !plan.bind_udp,
                "{s}: explicit simulate must not bind UDP (offline demo)"
            );
            assert!(plan.run_simulator);
            assert_eq!(plan.initial_source, "simulated");
        }
    }

    #[test]
    fn explicit_esp32_binds_udp() {
        let plan = plan_source("esp32", false, false);
        assert!(plan.bind_udp);
        assert!(!plan.run_simulator);
        assert_eq!(plan.initial_source, "esp32");
    }

    // Promotion check: the runtime promotes by setting `AppStateInner.source`
    // to "esp32" on the first real frame; `effective_source()` then reports it
    // (and reverts to "esp32:offline" after a 5 s gap). This asserts the
    // promotion direction the simulator/receiver rely on, without binding a
    // socket — it exercises the same `source` field the UDP task writes.
    #[test]
    fn effective_source_promotes_from_offline_to_esp32_on_real_frame() {
        // Start as the auto/no-source plan does: source = "esp32", with no
        // received frame yet, so effective_source() is explicitly offline.
        let mut src = "esp32".to_string();
        assert_eq!(promote_view(&src, None), "esp32:offline");
        // First real frame arrives → udp_receiver_task sets source = "esp32".
        src = "esp32".to_string();
        let fresh = Some(std::time::Duration::from_millis(10));
        assert_eq!(
            promote_view(&src, fresh),
            "esp32",
            "fresh esp32 frame ⇒ live"
        );
        // After a >5 s gap it reverts to offline (inverse machinery, #1004).
        let stale = Some(ESP32_OFFLINE_TIMEOUT + std::time::Duration::from_secs(1));
        assert_eq!(promote_view(&src, stale), "esp32:offline");
    }

    #[test]
    fn configured_esp32_without_a_frame_is_offline() {
        assert_eq!(promote_view("esp32", None), "esp32:offline");
    }

    /// Mirror of `AppStateInner::effective_source` over just (source, age) so the
    /// promotion/reversion logic is testable without constructing full state.
    fn promote_view(source: &str, last_frame_age: Option<std::time::Duration>) -> String {
        if source == "esp32" {
            match last_frame_age {
                Some(age) if age <= ESP32_OFFLINE_TIMEOUT => {}
                _ => return "esp32:offline".to_string(),
            }
        }
        source.to_string()
    }
}

// ── Simulated data generator ─────────────────────────────────────────────────

fn generate_simulated_frame(tick: u64) -> Esp32Frame {
    let t = tick as f64 * 0.1;
    let n_sub = 56usize;
    let mut amplitudes = Vec::with_capacity(n_sub);
    let mut phases = Vec::with_capacity(n_sub);

    for i in 0..n_sub {
        let base = 15.0 + 5.0 * (i as f64 * 0.1 + t * 0.3).sin();
        let noise = (i as f64 * 7.3 + t * 13.7).sin() * 2.0;
        amplitudes.push((base + noise).max(0.1));
        phases.push((i as f64 * 0.2 + t * 0.5).sin() * std::f64::consts::PI);
    }

    Esp32Frame {
        magic: 0xC511_0001,
        node_id: 1,
        n_antennas: 1,
        n_subcarriers: n_sub as u16,
        freq_mhz: 2437,
        sequence: tick as u32,
        rssi: (-40.0 + 5.0 * (t * 0.2).sin()) as i8,
        noise_floor: -90,
        ppdu_type: wifi_densepose_hardware::PpduType::HtLegacy,
        amplitudes,
        phases,
    }
}

// ── REST endpoints ───────────────────────────────────────────────────────────

async fn health(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "status": "ok",
        "source": s.effective_source(),
        "tick": s.tick,
        "clients": s.tx.receiver_count(),
    }))
}

/// Generate WiFi-derived pose keypoints from sensing data.
///
/// Keypoint positions are modulated by real signal features rather than a pure
/// time-based sine/cosine loop:
///
///   - `motion_band_power`    drives whole-body translation and limb splay
///   - `variance`             seeds per-frame noise so the skeleton never freezes
///   - `breathing_band_power` expands/contracts torso keypoints (shoulders, hips)
///   - `dominant_freq_hz`     tilts the upper body laterally (lean direction)
///   - `change_points`        adds burst jitter to extremities (wrists, ankles)
///
/// When `presence == false` no persons are returned (empty room).
/// When walking is detected (`motion_score > 0.55`) the figure shifts laterally
/// with a stride-swing pattern applied to arms and legs.
// ── Multi-person estimation (issue #97) ──────────────────────────────────────
/// Fuse features across all active nodes for higher SNR.
///
/// When multiple ESP32 nodes observe the same room, their CSI features
/// can be combined:
/// - Variance: use max (most sensitive node dominates)
/// - Motion/breathing/spectral power: weighted average by RSSI (closer node = higher weight)
/// - Dominant frequency: weighted average
/// - Change points: keep current node's value (not meaningful to average)
/// - Mean RSSI: use max (best signal)
fn fuse_multi_node_features(
    current_features: &FeatureInfo,
    node_states: &HashMap<u8, NodeState>,
) -> FeatureInfo {
    let now = std::time::Instant::now();
    let mut active_nodes: Vec<(&u8, &NodeState)> = node_states
        .iter()
        .filter(|(_, ns)| {
            ns.last_frame_time
                .is_some_and(|t| now.duration_since(t).as_secs() < 10)
        })
        .collect();
    active_nodes.sort_by_key(|(node_id, _)| **node_id);
    let active: Vec<(&FeatureInfo, f64)> = active_nodes
        .into_iter()
        .filter_map(|(_, ns)| {
            let feat = ns.latest_features.as_ref()?;
            let rssi = ns.rssi_history.back().copied().unwrap_or(-80.0);
            Some((feat, rssi))
        })
        .collect();

    if active.len() <= 1 {
        return current_features.clone();
    }

    // RSSI-based weights: higher RSSI = closer to person = more weight.
    // Map RSSI relative to best node into [0.1, 1.0].
    let max_rssi = active
        .iter()
        .map(|(_, r)| *r)
        .fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = active
        .iter()
        .map(|(_, r)| (1.0 + (r - max_rssi + 20.0) / 20.0).clamp(0.1, 1.0))
        .collect();
    let w_sum: f64 = weights.iter().sum::<f64>().max(1e-9);

    FeatureInfo {
        // Weighted average variance (not max — max inflates person score
        // and causes count flips between 1↔2 persons).
        variance: active
            .iter()
            .zip(&weights)
            .map(|((f, _), w)| f.variance * w)
            .sum::<f64>()
            / w_sum,
        // Weighted average for motion/breathing/spectral
        motion_band_power: active
            .iter()
            .zip(&weights)
            .map(|((f, _), w)| f.motion_band_power * w)
            .sum::<f64>()
            / w_sum,
        breathing_band_power: active
            .iter()
            .zip(&weights)
            .map(|((f, _), w)| f.breathing_band_power * w)
            .sum::<f64>()
            / w_sum,
        spectral_power: active
            .iter()
            .zip(&weights)
            .map(|((f, _), w)| f.spectral_power * w)
            .sum::<f64>()
            / w_sum,
        dominant_freq_hz: active
            .iter()
            .zip(&weights)
            .map(|((f, _), w)| f.dominant_freq_hz * w)
            .sum::<f64>()
            / w_sum,
        change_points: current_features.change_points, // keep current node's value
        // Best RSSI across nodes
        mean_rssi: active
            .iter()
            .map(|(f, _)| f.mean_rssi)
            .fold(f64::NEG_INFINITY, f64::max),
    }
}

/// Estimate person count from CSI features using a weighted composite heuristic.
///
/// Single ESP32 link limitations: variance-based detection can reliably detect
/// 1-2 persons. 3+ is speculative and requires ≥3 nodes for spatial resolution.
///
/// Returns a raw score (0.0..1.0) that the caller converts to person count
/// after temporal smoothing.
fn compute_person_score(state: &AppStateInner, feat: &FeatureInfo) -> f64 {
    // ADR-044 §5.2: adaptive rolling-P95 normalization.
    // Legacy fixed denominators (variance/300, motion/250, spectral/500) saturate
    // when live ESP32 values exceed those limits — zero dynamic range results.
    // Use the P95 of the last ~30 s of history instead, falling back to the legacy
    // denominators during cold-start (<60 samples) to preserve day-0 behaviour.
    let var_denom = state
        .p95_variance
        .current()
        .map(|p| p.max(50.0))
        .unwrap_or(300.0);
    let motion_denom = state
        .p95_motion_band_power
        .current()
        .map(|p| p.max(50.0))
        .unwrap_or(250.0);
    let sp_denom = state
        .p95_spectral_power
        .current()
        .map(|p| p.max(100.0))
        .unwrap_or(500.0);
    let var_norm = (feat.variance / var_denom).clamp(0.0, 1.0);
    let cp_norm = (feat.change_points as f64 / 30.0).clamp(0.0, 1.0);
    let motion_norm = (feat.motion_band_power / motion_denom).clamp(0.0, 1.0);
    let sp_norm = (feat.spectral_power / sp_denom).clamp(0.0, 1.0);
    var_norm * 0.40 + cp_norm * 0.20 + motion_norm * 0.25 + sp_norm * 0.15
}

/// Estimate person count via ruvector DynamicMinCut on the subcarrier
/// temporal correlation graph.
///
/// Builds a graph where:
/// - Nodes = active subcarriers (variance > noise floor)
/// - Edges = Pearson correlation between subcarrier time series
///   (weight = correlation coefficient; high correlation = heavy edge)
/// - Source = virtual node connected to the most active subcarrier
/// - Sink = virtual node connected to the least correlated subcarrier
///
/// The min-cut value indicates how many independent motion clusters exist:
/// - High min-cut (relative to total edge weight) → one tightly coupled
///   group → 1 person
/// - Low min-cut → two loosely coupled groups → 2 persons
///
/// Uses `ruvector_mincut::DynamicMinCut` for O(V²E) exact max-flow.
fn estimate_persons_from_correlation(frame_history: &VecDeque<Vec<f64>>) -> usize {
    let n_frames = frame_history.len();
    if n_frames < 10 {
        return 1;
    }

    let window: Vec<&Vec<f64>> = frame_history.iter().rev().take(20).collect();
    let n_sub = window[0].len().min(56);
    if n_sub < 4 {
        return 1;
    }
    let k = window.len() as f64;

    // Per-subcarrier mean and variance
    let mut means = vec![0.0f64; n_sub];
    let mut variances = vec![0.0f64; n_sub];
    for frame in &window {
        for sc in 0..n_sub.min(frame.len()) {
            means[sc] += frame[sc] / k;
        }
    }
    for frame in &window {
        for sc in 0..n_sub.min(frame.len()) {
            variances[sc] += (frame[sc] - means[sc]).powi(2) / k;
        }
    }

    // Active subcarriers: variance above noise floor
    let noise_floor = 1.0;
    let active: Vec<usize> = (0..n_sub)
        .filter(|&sc| variances[sc] > noise_floor)
        .collect();
    let m = active.len();
    if m < 3 {
        return if m == 0 { 0 } else { 1 };
    }

    // Build correlation graph edges between active subcarriers.
    // Edge weight = |Pearson correlation|. High correlation → same person.
    let mut edges: Vec<(u64, u64, f64)> = Vec::new();
    let source = m as u64;
    let sink = (m + 1) as u64;

    // Precompute std devs
    let stds: Vec<f64> = active
        .iter()
        .map(|&sc| variances[sc].sqrt().max(1e-9))
        .collect();

    for i in 0..m {
        for j in (i + 1)..m {
            // Pearson correlation between subcarriers i and j
            let mut cov = 0.0f64;
            for frame in &window {
                let si = active[i];
                let sj = active[j];
                if si < frame.len() && sj < frame.len() {
                    cov += (frame[si] - means[si]) * (frame[sj] - means[sj]) / k;
                }
            }
            let corr = (cov / (stds[i] * stds[j])).abs();
            if corr > 0.1 {
                // Bidirectional edges for flow network
                let weight = corr * 10.0; // Scale up for integer-like flow
                edges.push((i as u64, j as u64, weight));
                edges.push((j as u64, i as u64, weight));
            }
        }
    }

    // Source → highest-variance subcarrier, Sink → lowest-variance.
    // partial_cmp returns None on NaN; the outer unwrap_or only catches an
    // empty iterator, not a comparator panic. Same NaN-panic class as #611
    // — a single NaN variance frame would kill the sensing-server process.
    let (max_var_idx, _) = active
        .iter()
        .enumerate()
        .max_by(|(_, &a), (_, &b)| {
            variances[a]
                .partial_cmp(&variances[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((0, &0));
    let (min_var_idx, _) = active
        .iter()
        .enumerate()
        .min_by(|(_, &a), (_, &b)| {
            variances[a]
                .partial_cmp(&variances[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((0, &0));

    if max_var_idx == min_var_idx {
        return 1;
    }

    edges.push((source, max_var_idx as u64, 100.0));
    edges.push((min_var_idx as u64, sink, 100.0));

    // Run min-cut
    let mc: DynamicMinCut = match MinCutBuilder::new()
        .exact()
        .with_edges(edges.clone())
        .build()
    {
        Ok(mc) => mc,
        Err(_) => return 1,
    };

    let cut_value = mc.min_cut_value();
    let total_edge_weight: f64 = edges
        .iter()
        .filter(|(s, t, _)| *s != source && *s != sink && *t != source && *t != sink)
        .map(|(_, _, w)| w)
        .sum::<f64>()
        / 2.0; // bidirectional → halve

    if total_edge_weight < 1e-9 {
        return 1;
    }

    // Normalized cut ratio: low = easy to split = multiple people
    let cut_ratio = cut_value / total_edge_weight;

    if cut_ratio > 0.4 {
        1 // Tightly coupled — one person
    } else if cut_ratio > 0.15 {
        2 // Moderately separable — two people
    } else {
        3 // Highly separable — three+ people
    }
}

/// Map a DynamicMinCut occupancy estimate (`estimate_persons_from_correlation`,
/// 0–3) onto a target score whose steady state round-trips back through
/// `score_to_person_count` to the *same* count (issue #803).
///
/// The CSI path EMA-smooths this target and re-discretises it via
/// `score_to_person_count`. The previous `corr_persons / 3.0` mapping put a
/// 2-person estimate at 0.667 — just under the 0.70 up-threshold — so the
/// smoothed score could never climb past 1, pinning the per-node count to 1
/// even when the min-cut cleanly separated two people. These anchors sit
/// inside the hysteresis bands so a *sustained* estimate converges to the
/// matching count while transient noise stays gated by the EMA:
///   1 → 0.40  (below the 0.55 down-threshold)
///   2 → 0.74  (between the 0.70 up- and 0.78 down-thresholds → reachable
///              both climbing from 1 and falling from 3)
///   3 → 0.96  (above the 0.92 up-threshold)
fn corr_persons_to_score(corr_persons: usize) -> f64 {
    match corr_persons {
        0 => 0.20,
        1 => 0.40,
        2 => 0.74,
        _ => 0.96,
    }
}

#[cfg(test)]
mod corr_persons_round_trip_tests {
    //! Issue #803 — a sustained min-cut occupancy estimate must survive the
    //! CSI path's EMA + `score_to_person_count` re-discretisation instead of
    //! collapsing back to 1.
    use super::*;

    /// Replays the CSI-loop smoothing (`score = score*0.92 + target*0.08`)
    /// followed by `score_to_person_count`, exactly as the per-node path does,
    /// and returns the steady-state reported count.
    fn converge(corr_persons: usize) -> usize {
        let mut score = 0.0f64;
        let mut count = 1usize;
        for _ in 0..400 {
            let target = corr_persons_to_score(corr_persons);
            score = score * 0.92 + target * 0.08;
            count = score_to_person_count(score, count);
        }
        count
    }

    #[test]
    fn sustained_one_person_estimate_reports_one() {
        assert_eq!(converge(1), 1);
    }

    #[test]
    fn sustained_two_person_estimate_reports_two() {
        assert_eq!(converge(2), 2, "#803: min-cut=2 must round-trip to count 2");
    }

    #[test]
    fn sustained_three_person_estimate_reports_three() {
        assert_eq!(converge(3), 3);
    }

    #[test]
    fn old_div3_mapping_would_pin_two_people_to_one() {
        // Regression-documents the bug: 2/3 = 0.667 never crosses the 0.70
        // up-threshold, so the old mapping reported 1 for two people.
        let mut score = 0.0f64;
        let mut count = 1usize;
        for _ in 0..400 {
            score = score * 0.92 + (2.0 / 3.0) * 0.08;
            count = score_to_person_count(score, count);
        }
        assert_eq!(count, 1, "old corr_persons/3.0 mapping was the #803 bug");
    }
}

/// Convert smoothed person score to discrete count with hysteresis.
///
/// Uses asymmetric thresholds: higher threshold to *add* a person, lower to
/// *drop* one.  This prevents flickering when the score hovers near a boundary
/// (the #1 user-reported issue — see #237, #249, #280, #292).
fn score_to_person_count(smoothed_score: f64, prev_count: usize) -> usize {
    // Up-thresholds (must exceed to increase count):
    //   1→2: 0.80  (raised from 0.65 — single-person movement in multipath
    //               rooms easily hits 0.65, causing false 2-person detection)
    //   2→3: 0.92  (raised from 0.85 — 3 persons needs very strong signal)
    // Down-thresholds (must drop below to decrease count):
    //   2→1: 0.55  (hysteresis gap of 0.25)
    //   3→2: 0.78  (hysteresis gap of 0.14)
    match prev_count {
        0 | 1 => {
            if smoothed_score > 0.85 {
                3
            } else if smoothed_score > 0.70 {
                2
            } else {
                1
            }
        }
        2 => {
            if smoothed_score > 0.92 {
                3
            } else if smoothed_score < 0.55 {
                1
            } else {
                2 // hold — within hysteresis band
            }
        }
        _ => {
            // prev_count >= 3
            if smoothed_score < 0.55 {
                1
            } else if smoothed_score < 0.78 {
                2
            } else {
                3 // hold
            }
        }
    }
}

/// Combine the activity-score-derived aggregate count with the count-aware
/// per-node estimates (issue #803).
///
/// The aggregate `s.person_count()` is driven by `smoothed_person_score`, an
/// EMA-smoothed *activity* score (amplitude variance / motion / spectral
/// energy). That score saturates near a single occupant — one moving person
/// can max it out — so it cannot discriminate occupancy *count*, leaving the
/// reported value pinned at 1. Meanwhile the per-node paths already derive a
/// genuinely count-aware estimate (ESP32 firmware `n_persons`, or the
/// DynamicMinCut `corr_persons`) and stash it in `NodeState::prev_person_count`
/// — but that value was being discarded by the aggregator.
///
/// This takes the larger of the two. It can only ever *raise* the count when a
/// node has positively estimated more occupants, so it never regresses the
/// single-person case (a lone occupant yields `node_max == 1`).
fn aggregate_person_count(
    activity_count: usize,
    node_states: &std::collections::HashMap<u8, NodeState>,
) -> usize {
    let node_max = node_states
        .values()
        .map(|n| n.prev_person_count)
        .max()
        .unwrap_or(0);
    activity_count.max(node_max)
}

#[cfg(test)]
mod aggregate_person_count_tests {
    //! Issue #803 — the saturating activity score must not clamp a
    //! count-aware per-node estimate back down to 1.
    use super::*;
    use std::collections::HashMap;

    fn node_with_count(c: usize) -> NodeState {
        let mut n = NodeState::new();
        n.prev_person_count = c;
        n
    }

    #[test]
    fn empty_nodes_fall_back_to_activity_count() {
        let nodes: HashMap<u8, NodeState> = HashMap::new();
        assert_eq!(aggregate_person_count(1, &nodes), 1);
        assert_eq!(aggregate_person_count(0, &nodes), 0);
    }

    #[test]
    fn node_estimate_raises_a_saturated_activity_count() {
        // The activity score saturates at 1, but a node positively reports 2.
        let mut nodes = HashMap::new();
        nodes.insert(1u8, node_with_count(2));
        assert_eq!(
            aggregate_person_count(1, &nodes),
            2,
            "a node reporting 2 must not be discarded by the activity count"
        );
    }

    #[test]
    fn activity_count_wins_when_higher_than_nodes() {
        // Never *lower* a confident activity-derived count to a stale node value.
        let mut nodes = HashMap::new();
        nodes.insert(1u8, node_with_count(1));
        assert_eq!(aggregate_person_count(3, &nodes), 3);
    }

    #[test]
    fn takes_max_across_multiple_nodes() {
        let mut nodes = HashMap::new();
        nodes.insert(1u8, node_with_count(1));
        nodes.insert(2u8, node_with_count(3));
        nodes.insert(3u8, node_with_count(2));
        assert_eq!(aggregate_person_count(1, &nodes), 3);
    }

    #[test]
    fn single_occupant_is_never_inflated() {
        // Regression guard: a lone occupant (every node sees 1) stays 1.
        let mut nodes = HashMap::new();
        nodes.insert(1u8, node_with_count(1));
        nodes.insert(2u8, node_with_count(1));
        assert_eq!(aggregate_person_count(1, &nodes), 1);
    }
}

/// Generate a single person's skeleton with per-person spatial offset and phase stagger.
///
/// `person_idx`: 0-based index of this person.
/// `total_persons`: total number of detected persons (for spacing calculation).
fn derive_single_person_pose(
    update: &SensingUpdate,
    person_idx: usize,
    total_persons: usize,
) -> PersonDetection {
    let cls = &update.classification;
    let feat = &update.features;

    // Per-person phase offset: ~120 degrees apart so they don't move in sync.
    let phase_offset = person_idx as f64 * 2.094;

    // Spatial spread: persons distributed symmetrically around center.
    let half = (total_persons as f64 - 1.0) / 2.0;
    let person_x_offset = (person_idx as f64 - half) * 120.0; // 120px spacing

    // Confidence decays for additional persons (less certain about person 2, 3).
    let conf_decay = 1.0 - person_idx as f64 * 0.15;

    // ── Signal-derived scalars ────────────────────────────────────────────────

    let motion_score = (feat.motion_band_power / 15.0).clamp(0.0, 1.0);
    let is_walking = motion_score > 0.55;
    let breath_amp = (feat.breathing_band_power * 4.0).clamp(0.0, 12.0);

    let breath_phase = if let Some(ref vs) = update.vital_signs {
        let bpm = vs.breathing_rate_bpm.unwrap_or(15.0);
        let freq = (bpm / 60.0).clamp(0.1, 0.5);
        // Slow tick rate (0.02) for gentle breathing, not jerky oscillation.
        (update.tick as f64 * freq * 0.02 * std::f64::consts::TAU + phase_offset).sin()
    } else {
        (update.tick as f64 * 0.02 + phase_offset).sin()
    };

    let lean_x = (feat.dominant_freq_hz / 5.0 - 1.0).clamp(-1.0, 1.0) * 18.0;

    let stride_x = if is_walking {
        let stride_phase =
            (feat.motion_band_power * 0.7 + update.tick as f64 * 0.06 + phase_offset).sin();
        stride_phase * 20.0 * motion_score
    } else {
        0.0
    };

    // Dampen burst and noise to reduce jitter.  The original used
    // tick*17.3 which changed wildly every frame.  Now use slow tick
    // rate and minimal burst scaling for a stable skeleton.
    let burst = (feat.change_points as f64 / 20.0).clamp(0.0, 0.3);

    let noise_seed = person_idx as f64 * 97.1; // stable per-person, no tick
    let noise_val = (noise_seed.sin() * 43758.545).fract();

    let snr_factor = ((feat.variance - 0.5) / 10.0).clamp(0.0, 1.0);
    let base_confidence = cls.confidence * (0.6 + 0.4 * snr_factor) * conf_decay;

    // ── Skeleton base position ────────────────────────────────────────────────

    let base_x = 320.0 + stride_x + lean_x * 0.5 + person_x_offset;
    let base_y = 240.0 - motion_score * 8.0;

    // ── COCO 17-keypoint offsets from hip-center ──────────────────────────────

    let kp_names = [
        "nose",
        "left_eye",
        "right_eye",
        "left_ear",
        "right_ear",
        "left_shoulder",
        "right_shoulder",
        "left_elbow",
        "right_elbow",
        "left_wrist",
        "right_wrist",
        "left_hip",
        "right_hip",
        "left_knee",
        "right_knee",
        "left_ankle",
        "right_ankle",
    ];

    let kp_offsets: [(f64, f64); 17] = [
        (0.0, -80.0),   // 0  nose
        (-8.0, -88.0),  // 1  left_eye
        (8.0, -88.0),   // 2  right_eye
        (-16.0, -82.0), // 3  left_ear
        (16.0, -82.0),  // 4  right_ear
        (-30.0, -50.0), // 5  left_shoulder
        (30.0, -50.0),  // 6  right_shoulder
        (-45.0, -15.0), // 7  left_elbow
        (45.0, -15.0),  // 8  right_elbow
        (-50.0, 20.0),  // 9  left_wrist
        (50.0, 20.0),   // 10 right_wrist
        (-20.0, 20.0),  // 11 left_hip
        (20.0, 20.0),   // 12 right_hip
        (-22.0, 70.0),  // 13 left_knee
        (22.0, 70.0),   // 14 right_knee
        (-24.0, 120.0), // 15 left_ankle
        (24.0, 120.0),  // 16 right_ankle
    ];

    const TORSO_KP: [usize; 4] = [5, 6, 11, 12];
    const EXTREMITY_KP: [usize; 4] = [9, 10, 15, 16];

    let keypoints: Vec<PoseKeypoint> = kp_names
        .iter()
        .zip(kp_offsets.iter())
        .enumerate()
        .map(|(i, (name, (dx, dy)))| {
            let breath_dx = if TORSO_KP.contains(&i) {
                let sign = if *dx < 0.0 { -1.0 } else { 1.0 };
                sign * breath_amp * breath_phase * 0.5
            } else {
                0.0
            };
            let breath_dy = if TORSO_KP.contains(&i) {
                let sign = if *dy < 0.0 { -1.0 } else { 1.0 };
                sign * breath_amp * breath_phase * 0.3
            } else {
                0.0
            };

            let extremity_jitter = if EXTREMITY_KP.contains(&i) {
                let phase = noise_seed + i as f64 * 2.399;
                // Dampened from 12/8 to 4/3 to reduce visual jumping.
                (
                    phase.sin() * burst * motion_score * 4.0,
                    (phase * 1.31).cos() * burst * motion_score * 3.0,
                )
            } else {
                (0.0, 0.0)
            };

            let kp_noise_x = ((noise_seed + i as f64 * 1.618).sin() * 43758.545).fract()
                * feat.variance.sqrt().clamp(0.0, 3.0)
                * motion_score;
            let kp_noise_y = ((noise_seed + i as f64 * std::f64::consts::E).cos() * 31415.926)
                .fract()
                * feat.variance.sqrt().clamp(0.0, 3.0)
                * motion_score
                * 0.6;

            let swing_dy = if is_walking {
                let stride_phase =
                    (feat.motion_band_power * 0.7 + update.tick as f64 * 0.12 + phase_offset).sin();
                match i {
                    7 | 9 => -stride_phase * 20.0 * motion_score,
                    8 | 10 => stride_phase * 20.0 * motion_score,
                    13 | 15 => stride_phase * 25.0 * motion_score,
                    14 | 16 => -stride_phase * 25.0 * motion_score,
                    _ => 0.0,
                }
            } else {
                0.0
            };

            let final_x = base_x + dx + breath_dx + extremity_jitter.0 + kp_noise_x;
            let final_y = base_y + dy + breath_dy + extremity_jitter.1 + kp_noise_y + swing_dy;

            let kp_conf = if EXTREMITY_KP.contains(&i) {
                base_confidence * (0.7 + 0.3 * snr_factor) * (0.85 + 0.15 * noise_val)
            } else {
                base_confidence * (0.88 + 0.12 * ((i as f64 * 0.7 + noise_seed).cos()))
            };

            PoseKeypoint {
                name: name.to_string(),
                x: final_x,
                y: final_y,
                z: lean_x * 0.02,
                confidence: kp_conf.clamp(0.1, 1.0),
            }
        })
        .collect();

    let xs: Vec<f64> = keypoints.iter().map(|k| k.x).collect();
    let ys: Vec<f64> = keypoints.iter().map(|k| k.y).collect();
    let min_x = xs.iter().cloned().fold(f64::MAX, f64::min) - 10.0;
    let min_y = ys.iter().cloned().fold(f64::MAX, f64::min) - 10.0;
    let max_x = xs.iter().cloned().fold(f64::MIN, f64::max) + 10.0;
    let max_y = ys.iter().cloned().fold(f64::MIN, f64::max) + 10.0;

    PersonDetection {
        id: (person_idx + 1) as u32,
        confidence: cls.confidence * conf_decay,
        keypoints,
        bbox: BoundingBox {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(80.0),
            height: (max_y - min_y).max(160.0),
        },
        zone: format!("zone_{}", person_idx + 1),
        // Position/motion_score/pose are attached from the real signal_field
        // peaks by `attach_field_positions` after the tracker step (#1050);
        // default here so the synthetic-skeleton geometry stays unchanged.
        position: [0.0, 0.0, 0.0],
        motion_score: 0.0,
        pose: None,
    }
}

/// Attach real, field-derived per-person world positions to a `SensingUpdate`'s
/// `persons` (issue #1050).
///
/// For each detected person we read a strongest-peak position out of the frame's
/// real `signal_field` (the same grid the Observatory already renders) and map
/// it to room-world coordinates via `field_localize::cell_to_world`. `motion_score`
/// is passed through from the measured `motion_band_power`; `pose` is taken from
/// the real aggregate `posture` estimate when present, else left `None` (never
/// fabricated). Persons beyond the number of resolvable field peaks fall back to
/// the strongest peak so they remain co-located with real energy rather than at
/// a fake origin; if the field has no peak above threshold the position stays at
/// `[0,0,0]` and `motion_score` still reflects real motion power.
/// ADR-262 P3: emit one signed RuField `FieldEvent` for this sensing cycle.
///
/// Joins the cycle's [`SensingUpdate`] (features / classification /
/// signal_field) with the governed engine's trust state (`effective_class` /
/// `demoted`, recorded on `engine_bridge` by `observe_cycle`) into a
/// `SensingSnapshot`, then surfaces it via the P1 bridge on `/api/field` +
/// `/ws/field`. The bridge maps privacy by information content and the surface
/// applies the §10 network egress gate, so above-policy cycles never reach the
/// wire.
///
/// **No phantom events:** an empty/no-presence cycle (`presence == false`)
/// emits nothing — there is no person to describe, so no event is fabricated
/// (ADR-262 §4 P3 / §6). Cycles before the governed engine has produced a trust
/// class are likewise skipped (no class ⇒ nothing honest to stamp).
///
/// `identity_bound` is `false` on the live path: RuView's live cycle does not
/// bind an enrolled identity to the surface yet (that is a per-room-calibration
/// / AETHER concern, ADR-262 §8 Q4). This is conservative for egress — it only
/// ever *lowers* a Derived cycle from P5 to P4, both of which are already held
/// edge-local, so it cannot leak.
fn emit_rufield_event(s: &AppStateInner, update: &SensingUpdate, node_id: u8) {
    // No-presence ⇒ no phantom event.
    if !update.classification.presence {
        return;
    }
    // Need a governed trust class before we can honestly stamp privacy.
    let Some(effective_class) = s.engine_bridge.effective_class() else {
        return;
    };

    let timestamp_ns = if update.timestamp.is_finite() && update.timestamp > 0.0 {
        (update.timestamp * 1_000_000_000.0) as u64
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    };

    let snap = rufield_surface::build_snapshot(
        timestamp_ns,
        format!("esp32_node_{node_id}"),
        rufield_surface::SensingFeatures {
            mean_rssi: update.features.mean_rssi,
            variance: update.features.variance,
            motion_band_power: update.features.motion_band_power,
            breathing_band_power: update.features.breathing_band_power,
            dominant_freq_hz: update.features.dominant_freq_hz,
            change_points: update.features.change_points,
            spectral_power: update.features.spectral_power,
        },
        rufield_surface::SensingClass {
            motion_level: update.classification.motion_level.clone(),
            presence: update.classification.presence,
            confidence: update.classification.confidence,
        },
        Some(rufield_surface::SignalField {
            grid_size: update.signal_field.grid_size,
            values: update.signal_field.values.clone(),
        }),
        rufield_surface::ruview_class_from_bfld(effective_class),
        s.engine_bridge.demoted(),
        false, // identity_bound — see fn-doc (conservative, cannot leak).
    );

    // `field_surface` is its own Arc<RwLock<_>>; `try_write` is non-blocking and
    // never deadlocks against the `s` guard (a different lock). The only other
    // touchers are the read-only `/api/field` / `/ws/field` handlers, so
    // contention is negligible; a rare miss just drops one cycle's event.
    if let Ok(mut fs) = s.field_surface.try_write() {
        fs.emit(&snap);
    }
}

fn attach_field_positions(update: &mut SensingUpdate) {
    let measured_position = if update.source == "esp32" {
        validated_esp32_discrete_position(update).map(|(_, coordinates)| coordinates)
    } else {
        update
            .localization
            .as_ref()
            .and_then(|estimate| estimate.position)
            .map(|position| [position.x, 0.0, position.z])
    };
    let Some(persons) = update.persons.as_mut() else {
        return;
    };
    if persons.is_empty() {
        return;
    }

    let peaks = if measured_position.is_none() && update.source != "esp32" {
        let [nx, _ny, nz] = update.signal_field.grid_size;
        field_localize::extract_peaks(
            &update.signal_field.values,
            nx,
            nz,
            persons.len().max(1),
            3.0,
        )
    } else {
        Vec::new()
    };

    let motion_score = field_localize::motion_score_from_power(update.features.motion_band_power);
    let pose_label = update.posture.clone();

    for (i, person) in persons.iter_mut().enumerate() {
        if let Some(position) = measured_position {
            person.position = position;
        } else if let Some(peak) = peaks.get(i).or_else(|| peaks.first()) {
            person.position = peak.position;
        }
        person.motion_score = motion_score;
        person.pose = pose_label.clone();
    }
}

fn derive_pose_from_sensing(update: &SensingUpdate) -> Vec<PersonDetection> {
    let cls = &update.classification;
    if !cls.presence {
        return vec![];
    }
    if update.source == "esp32" {
        let Some((point_id, position)) = validated_esp32_discrete_position(update) else {
            return vec![];
        };
        return vec![PersonDetection {
            id: 1,
            confidence: cls.confidence.clamp(0.0, 1.0),
            keypoints: Vec::new(),
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            zone: point_id,
            position,
            motion_score: field_localize::motion_score_from_power(
                update.features.motion_band_power,
            ),
            pose: None,
        }];
    }

    // Use estimated_persons if set by the tick loop; otherwise default to 1.
    let person_count = update.estimated_persons.unwrap_or(1).max(1);

    (0..person_count)
        .map(|idx| derive_single_person_pose(update, idx, person_count))
        .collect()
}

fn validated_esp32_discrete_position(update: &SensingUpdate) -> Option<(String, [f64; 3])> {
    if update.source != "esp32" || !update.classification.presence {
        return None;
    }
    let position_live::LivePositionState::Position {
        point_id,
        coordinates_m,
    } = update.position_estimate.as_ref()?
    else {
        return None;
    };
    if !matches!(
        point_id.as_str(),
        "P01" | "P02" | "P03" | "P04" | "P05" | "P06" | "P07" | "P08" | "P09"
    ) {
        return None;
    }
    let room = update.room_dimensions?;
    if coordinates_m.iter().zip(room).any(|(coordinate, limit)| {
        !coordinate.is_finite() || *coordinate < 0.0 || *coordinate > limit
    }) {
        return None;
    }
    Some((point_id.clone(), *coordinates_m))
}

// ── RuVector Phase 2: Temporal EMA smoothing for keypoints ──────────────────

/// Expected bone lengths in pixel-space for the COCO-17 skeleton as used by
/// `derive_single_person_pose`. Pairs are (parent_idx, child_idx).
const POSE_BONE_PAIRS: &[(usize, usize)] = &[
    (5, 7),
    (7, 9),
    (6, 8),
    (8, 10), // arms
    (5, 11),
    (6, 12), // torso
    (11, 13),
    (13, 15),
    (12, 14),
    (14, 16), // legs
    (5, 6),
    (11, 12), // shoulders, hips
];

/// Apply temporal EMA smoothing and bone-length clamping to person detections.
///
/// For the *first* person (index 0) this uses the per-node `prev_keypoints`
/// state. Multi-person smoothing is left for a future phase.
fn apply_temporal_smoothing(persons: &mut [PersonDetection], ns: &mut NodeState) {
    if persons.is_empty() {
        return;
    }

    let alpha = ns.ema_alpha();
    let person = &mut persons[0]; // smooth primary person only

    let current_kps: Vec<[f64; 3]> = person
        .keypoints
        .iter()
        .map(|kp| [kp.x, kp.y, kp.z])
        .collect();

    let smoothed = if let Some(ref prev) = ns.prev_keypoints {
        let mut out = Vec::with_capacity(current_kps.len());
        for (cur, prv) in current_kps.iter().zip(prev.iter()) {
            out.push([
                alpha * cur[0] + (1.0 - alpha) * prv[0],
                alpha * cur[1] + (1.0 - alpha) * prv[1],
                alpha * cur[2] + (1.0 - alpha) * prv[2],
            ]);
        }
        // Clamp bone lengths to ±20% of previous frame.
        clamp_bone_lengths_f64(&mut out, prev);
        out
    } else {
        current_kps.clone()
    };

    // Write smoothed keypoints back into the person detection.
    for (kp, s) in person.keypoints.iter_mut().zip(smoothed.iter()) {
        kp.x = s[0];
        kp.y = s[1];
        kp.z = s[2];
    }

    ns.prev_keypoints = Some(smoothed);
}

/// Clamp bone lengths so no bone changes by more than MAX_BONE_CHANGE_RATIO
/// compared to the previous frame.
fn clamp_bone_lengths_f64(pose: &mut [[f64; 3]], prev: &[[f64; 3]]) {
    for &(p, c) in POSE_BONE_PAIRS {
        if p >= pose.len() || c >= pose.len() {
            continue;
        }
        let prev_len = dist_f64(&prev[p], &prev[c]);
        if prev_len < 1e-6 {
            continue;
        }
        let cur_len = dist_f64(&pose[p], &pose[c]);
        if cur_len < 1e-6 {
            continue;
        }
        let ratio = cur_len / prev_len;
        let lo = 1.0 - MAX_BONE_CHANGE_RATIO;
        let hi = 1.0 + MAX_BONE_CHANGE_RATIO;
        if ratio < lo || ratio > hi {
            let target = prev_len * ratio.clamp(lo, hi);
            let scale = target / cur_len;
            for dim in 0..3 {
                let diff = pose[c][dim] - pose[p][dim];
                pose[c][dim] = pose[p][dim] + diff * scale;
            }
        }
    }
}

fn dist_f64(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

// ── DensePose-compatible REST endpoints ─────────────────────────────────────

async fn health_live(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "status": "alive",
        "uptime": s.start_time.elapsed().as_secs(),
    }))
}

/// Lowercase hex of a 32-byte witness for JSON exposure.
fn witness_hex(w: [u8; 32]) -> String {
    use std::fmt::Write;
    w.iter().fold(String::with_capacity(64), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

fn position_setup_readiness_json(identity: Option<(&str, &str)>) -> serde_json::Value {
    serde_json::json!({
        "active": identity.is_some(),
        "setup_id": identity.map(|(setup_id, _)| setup_id),
        "setup_sha256": identity.map(|(_, setup_sha256)| setup_sha256),
    })
}

fn position_index_readiness_json(identity: Option<(&str, &str)>) -> serde_json::Value {
    serde_json::json!({
        "active": identity.is_some(),
        "index_sha256": identity.map(|(index_sha256, _)| index_sha256),
        "setup_id": identity.map(|(_, setup_id)| setup_id),
    })
}

async fn health_ready(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let position_setup = s.position_setup.as_deref();
    let position_runtime = s.live_position_tracker.runtime();
    Json(serde_json::json!({
        "status": "ready",
        "source": s.effective_source(),
        "position_setup": position_setup_readiness_json(
            position_setup.map(|setup| (setup.setup_id(), setup.setup_sha256())),
        ),
        "position_index": position_index_readiness_json(
            position_runtime.map(|runtime| (runtime.index_sha256(), runtime.setup_id())),
        ),
        // Governed trust-path state (ADR-135..146; review finding 1b): latest
        // witness + privacy class + recalibration flag, and the engine error
        // audit — previously write-only on AppState, now readable here.
        "trust": {
            "last_witness": s.engine_bridge.last_trust_witness().map(witness_hex),
            "effective_class": s.engine_bridge.effective_class().map(|c| format!("{c:?}")),
            "demoted": s.engine_bridge.demoted(),
            "recalibration_recommended": s.engine_bridge.recalibration_recommended(),
            "engine_error_count": s.engine_bridge.engine_error_count(),
            "timestamp_basis": s.engine_bridge.last_time_basis().map(|basis| basis.as_str()),
            "timestamp_spread_us": s.engine_bridge.last_timestamp_spread_us(),
            "host_candidate_spread_us": s.engine_bridge.last_host_candidate_spread_us(),
            "mesh_candidate_spread_us": s.engine_bridge.last_mesh_candidate_spread_us(),
            "selected_host_monotonic_us_by_rx": s.engine_bridge.last_selected_host_monotonic_us(),
            "selected_mesh_timestamp_us_by_rx": s.engine_bridge.last_selected_mesh_timestamp_us(),
            "coherent_cycle_count": s.engine_bridge.coherent_cycle_count(),
            "host_fallback_cycle_count": s.engine_bridge.host_fallback_cycle_count(),
            "raw_outputs_suppressed": s.engine_bridge.suppress_raw_outputs(),
        },
    }))
}

#[cfg(test)]
mod position_readiness_tests {
    use super::*;

    #[test]
    fn setup_and_index_readiness_are_independent_and_path_free() {
        let inactive_setup = position_setup_readiness_json(None);
        assert_eq!(inactive_setup["active"], false);
        assert!(inactive_setup["setup_id"].is_null());
        assert!(inactive_setup["setup_sha256"].is_null());

        let active_setup = position_setup_readiness_json(Some(("setup-0123", "setup-sha")));
        assert_eq!(active_setup["active"], true);
        assert_eq!(active_setup["setup_id"], "setup-0123");
        assert_eq!(active_setup["setup_sha256"], "setup-sha");
        assert!(active_setup.get("path").is_none());

        let inactive_index = position_index_readiness_json(None);
        assert_eq!(inactive_index["active"], false);
        assert!(inactive_index["index_sha256"].is_null());
        assert!(inactive_index["setup_id"].is_null());

        let active_index = position_index_readiness_json(Some(("index-sha", "setup-0123")));
        assert_eq!(active_index["active"], true);
        assert_eq!(active_index["index_sha256"], "index-sha");
        assert_eq!(active_index["setup_id"], "setup-0123");
        assert!(active_index.get("path").is_none());
    }
}

async fn health_system(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let uptime = s.start_time.elapsed().as_secs();
    let source = s.effective_source();
    let source_offline = source.ends_with(":offline");
    Json(serde_json::json!({
        "status": if source_offline { "degraded" } else { "healthy" },
        "components": {
            "api": { "status": "healthy", "message": "Rust Axum server" },
            "hardware": {
                "status": if source_offline { "degraded" } else { "healthy" },
                "message": format!("Source: {}", source)
            },
            "pose": { "status": "healthy", "message": "WiFi-derived pose estimation" },
            "stream": { "status": if s.tx.receiver_count() > 0 { "healthy" } else { "idle" },
                        "message": format!("{} client(s)", s.tx.receiver_count()) },
        },
        "metrics": {
            "uptime_seconds": uptime,
        }
    }))
}

async fn health_version() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": "wifi-densepose-sensing-server",
        "backend": "rust+axum+ruvector",
    }))
}

async fn health_metrics(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "tick": s.tick,
    }))
}

async fn api_info(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "backend": "rust",
        "source": s.effective_source(),
    }))
}

fn create_private_recording_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn route_raw_frame_to_live_position(
    tracker: &mut position_live::LivePositionTracker,
    setup: Option<&position_setup::SealedPositionSetup>,
    grid_accepted: bool,
    raw_frame: raw_csi_recording::RawCsiFrame,
) -> Result<(), String> {
    if !grid_accepted {
        let error = "raw CSI frame was rejected by the active per-node grid gate".to_string();
        tracker.reject_input(error.clone());
        return Err(error);
    }
    if let Some(setup) = setup {
        if let Err(error) = setup.validate_raw_csi_frame(&raw_frame) {
            let error = format!("sealed position setup rejected live frame: {error}");
            tracker.reject_input(error.clone());
            return Err(error);
        }
    }
    tracker.push_frame(raw_frame)
}

fn reject_raw_csi_ingress(state: &mut AppStateInner, rx_id: Option<u8>, reason: impl Into<String>) {
    let reason = reason.into();
    if let Some(node) = rx_id.and_then(|id| state.node_states.get_mut(&id)) {
        node.invalidate_source_binding_attestation();
    }
    if state.recording_active
        && state
            .raw_csi_tx
            .send(RawCsiIngress::Rejected {
                rx_id,
                reason: reason.clone(),
            })
            .is_err()
    {
        warn!("active raw recorder has no receiver for a rejected CSI datagram");
    }
    let estimate = state.live_position_tracker.reject_input(reason);
    replace_latest_esp32_position_estimate(state, estimate);
}

fn replace_latest_esp32_position_estimate(
    state: &mut AppStateInner,
    estimate: position_live::LivePositionState,
) {
    if !apply_latest_esp32_position_estimate(state, estimate) {
        return;
    }
    if let Some(Ok(json)) = state
        .latest_update
        .as_ref()
        .filter(|update| update.source == "esp32")
        .map(serde_json::to_string)
    {
        let _ = state.tx.send(json);
    }
}

/// Update the cached ESP32 position contract under the caller's state lock.
///
/// Any non-position or otherwise invalid state atomically removes the public
/// person markers and resets the pose tracker so no later pose endpoint can
/// reuse a detection from the previous accepted point.
fn apply_latest_esp32_position_estimate(
    state: &mut AppStateInner,
    estimate: position_live::LivePositionState,
) -> bool {
    let clear_pose_cache = {
        let Some(update) = state
            .latest_update
            .as_mut()
            .filter(|update| update.source == "esp32")
        else {
            return false;
        };
        apply_esp32_position_estimate_contract(update, estimate)
    };
    if clear_pose_cache {
        clear_esp32_pose_cache(&mut state.pose_tracker, &mut state.last_tracker_instant);
    }
    true
}

fn gate_mmwave_candidate_for_publication(
    candidate: position_live::LivePositionState,
    publication_allowed: bool,
) -> position_live::LivePositionState {
    if publication_allowed {
        candidate
    } else {
        position_live::LivePositionState::Uncalibrated
    }
}

#[cfg(test)]
mod mmwave_publication_tests {
    use super::*;

    #[test]
    fn an_unvalidated_candidate_never_reaches_the_public_position_contract() {
        let candidate = position_live::LivePositionState::Position {
            point_id: "P05".to_string(),
            coordinates_m: [2.0, 0.0, 1.5],
        };
        assert_eq!(
            gate_mmwave_candidate_for_publication(candidate.clone(), false),
            position_live::LivePositionState::Uncalibrated
        );
        assert_eq!(
            gate_mmwave_candidate_for_publication(candidate.clone(), true),
            candidate
        );
    }
}

/// Apply one position state to the public update and report whether every
/// cached pose representation must be cleared under the same state lock.
fn apply_esp32_position_estimate_contract(
    update: &mut SensingUpdate,
    estimate: position_live::LivePositionState,
) -> bool {
    update.position_estimate = Some(estimate);
    let clear_pose_cache = validated_esp32_discrete_position(update).is_none();
    if clear_pose_cache {
        update.persons = None;
        update.estimated_persons = None;
    }
    clear_pose_cache
}

fn clear_esp32_pose_cache(
    pose_tracker: &mut PoseTracker,
    last_tracker_instant: &mut Option<std::time::Instant>,
) {
    *pose_tracker = PoseTracker::new();
    *last_tracker_instant = None;
}

fn position_raw_input_is_stale(
    last_raw_csi_frame: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    last_raw_csi_frame
        .is_none_or(|last| now.saturating_duration_since(last) >= POSITION_RAW_STALE_TIMEOUT)
}

fn append_raw_recording_frame(
    writer: &mut std::io::BufWriter<std::fs::File>,
    raw_frame: raw_csi_recording::RawCsiFrame,
    position_setup: Option<&position_setup::SealedPositionSetup>,
    session_id: &Option<String>,
    label: &Option<String>,
    ground_truth: &Option<raw_csi_recording::GroundTruth>,
    result: &mut RecordingWriterResult,
) -> Result<(), String> {
    use std::io::Write;

    if let Some(position_setup) = position_setup {
        position_setup
            .validate_raw_csi_frame(&raw_frame)
            .map_err(|error| format!("sealed position setup rejected frame: {error}"))?;
    }
    let raw_frame = raw_frame
        .with_recording_metadata(session_id.clone(), label.clone(), ground_truth.clone())
        .map_err(|error| format!("invalid frame metadata: {error}"))?;
    if let Some(summary) = result.rx_summaries.get(&raw_frame.rx_id) {
        summary.validate_next_frame(&raw_frame)?;
    }
    let line = raw_csi_recording::encode_json_line(&raw_frame)
        .map_err(|error| format!("encode error: {error}"))?;
    writer
        .write_all(line.as_bytes())
        .map_err(|error| format!("write error: {error}"))?;
    match result.rx_summaries.get_mut(&raw_frame.rx_id) {
        Some(summary) => summary.observe_written_frame(&raw_frame),
        None => {
            result.rx_summaries.insert(
                raw_frame.rx_id,
                raw_csi_recording::RawCsiRxSummary::first_written_frame(&raw_frame),
            );
        }
    }
    result.frames_written += 1;
    if result.frames_written % 100 == 0 {
        writer
            .flush()
            .map_err(|error| format!("periodic flush error: {error}"))?;
    }
    Ok(())
}

fn mark_recording_rejected(result: &mut RecordingWriterResult, rx_id: Option<u8>, reason: &str) {
    let receiver = rx_id
        .map(|id| format!("RX{id}"))
        .unwrap_or_else(|| "unknown RX".to_string());
    result.error = Some(format!(
        "{receiver} sent raw CSI rejected before recording: {reason}"
    ));
}

fn resolve_recording_setup_identity(
    body: &serde_json::Value,
    loaded_setup: Option<(&str, &str)>,
) -> Result<(Option<String>, Option<String>), String> {
    let supplied_id = optional_string_field(body, "setup_id")?;
    let supplied_sha256 = optional_string_field(body, "setup_sha256")?;

    match loaded_setup {
        Some((expected_id, expected_sha256)) => {
            if supplied_id.is_some() != supplied_sha256.is_some() {
                return Err(
                    "setup_id and setup_sha256 must both be omitted or supplied together"
                        .to_string(),
                );
            }
            if let Some(actual_id) = supplied_id {
                if actual_id != expected_id || supplied_sha256 != Some(expected_sha256) {
                    return Err(
                        "recording setup identity does not match the loaded sealed setup"
                            .to_string(),
                    );
                }
            }
            Ok((
                Some(expected_id.to_string()),
                Some(expected_sha256.to_string()),
            ))
        }
        None => {
            if supplied_id.is_some() || supplied_sha256.is_some() {
                return Err(
                    "setup_id/setup_sha256 require a server started with --position-setup"
                        .to_string(),
                );
            }
            Ok((None, None))
        }
    }
}

fn optional_string_field<'a>(
    body: &'a serde_json::Value,
    field: &str,
) -> Result<Option<&'a str>, String> {
    match body.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("{field} must be a string when supplied")),
    }
}

fn append_recording_writer_error(result: &mut RecordingWriterResult, error: String) {
    result.error = Some(match result.error.take() {
        Some(previous) => format!("{previous}; {error}"),
        None => error,
    });
}

fn finalize_recording_metadata(
    recording_id: &str,
    duration_secs: u64,
    result: &RecordingWriterResult,
) -> Result<(), String> {
    finalize_recording_metadata_in_dir(
        std::path::Path::new("data/recordings"),
        recording_id,
        duration_secs,
        result,
    )
}

fn finalize_recording_metadata_in_dir(
    recordings_dir: &std::path::Path,
    recording_id: &str,
    duration_secs: u64,
    result: &RecordingWriterResult,
) -> Result<(), String> {
    use std::io::Write;

    raw_csi_recording::validate_recording_id(recording_id).map_err(|error| error.to_string())?;
    let metadata_path = recordings_dir.join(format!("{recording_id}.raw-csi.v1.meta.json"));
    let metadata_bytes =
        std::fs::read(&metadata_path).map_err(|error| format!("metadata read error: {error}"))?;
    let mut metadata: serde_json::Value = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("metadata decode error: {error}"))?;
    metadata["ended_at_unix_seconds"] = serde_json::json!(chrono_timestamp());
    metadata["ended_at_unix_ns"] = serde_json::json!(raw_csi_recording::now_unix_ns()
        .map_err(|error| format!("metadata end timestamp error: {error}"))?);
    metadata["duration_secs"] = serde_json::json!(duration_secs);
    metadata["frames_written"] = serde_json::json!(result.frames_written);
    metadata["rx_summaries"] = serde_json::json!(result.rx_summaries.values().collect::<Vec<_>>());
    metadata["dropped_frames"] = serde_json::json!(result.dropped_frames);
    metadata["incomplete"] = serde_json::json!(result.incomplete());
    metadata["status"] = serde_json::json!(if result.incomplete() {
        "incomplete"
    } else {
        "completed"
    });
    metadata["writer_error"] = serde_json::json!(result.error);

    let encoded = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("metadata encode error: {error}"))?;
    let nonce = raw_csi_recording::now_unix_ns()
        .map_err(|error| format!("metadata timestamp error: {error}"))?;
    let temp_path = recordings_dir.join(format!(
        ".{recording_id}.raw-csi.v1.meta.{}.{}.tmp",
        std::process::id(),
        nonce
    ));

    let write_result = (|| -> Result<(), String> {
        let mut file = create_private_recording_file(&temp_path)
            .map_err(|error| format!("metadata temp create error: {error}"))?;
        file.write_all(&encoded)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("metadata temp write error: {error}"))?;
        std::fs::rename(&temp_path, &metadata_path)
            .map_err(|error| format!("metadata atomic replace error: {error}"))
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_result
}

/// Generate a simple timestamp string (epoch seconds) for recording IDs.
fn chrono_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn info_page() -> Html<String> {
    Html(
        "<html><body>\
         <h1>WiFi-DensePose Sensing Server</h1>\
         <p>Rust + Axum + RuVector</p>\
         <ul>\
         <li><a href='/health'>/health</a> — Server health</li>\
         <li><a href='/api/v1/sensing/latest'>/api/v1/sensing/latest</a> — Latest sensing data</li>\
         <li><a href='/api/v1/vital-signs'>/api/v1/vital-signs</a> — Vital sign estimates (HR/RR)</li>\
         <li><a href='/api/v1/model/info'>/api/v1/model/info</a> — RVF model container info</li>\
         <li>ws://localhost:8765/ws/sensing — WebSocket stream</li>\
         </ul>\
         </body></html>"
         .to_string()
    )
}
/// Map one sensing-broadcast JSON document into the `VitalsSnapshot`(s) to
/// publish over MQTT (issues #872/#898).
///
/// Multi-node sources carry a `nodes` array where **each node has its own
/// `classification`** (`motion_level`, `presence`, `confidence`) and RSSI — so
/// each node must surface its *own* presence/motion, not the room-level
/// aggregate. Previously the bridge applied the aggregate `classification` to
/// every per-node Home-Assistant device, so a node in an empty corner inherited
/// another node's "present" (and `motion_level: "absent"` was mis-mapped to full
/// motion). Vitals (breathing / heart rate) and the person count are room-level
/// and shared across the per-node devices. Falls back to a single aggregate
/// snapshot when there is no per-node data (e.g. wifi / simulate sources).
#[cfg(feature = "mqtt")]
fn vitals_snapshots_from_sensing_json(
    v: &serde_json::Value,
    base_id: &str,
) -> Vec<wifi_densepose_sensing_server::mqtt::state::VitalsSnapshot> {
    use wifi_densepose_sensing_server::mqtt::state::VitalsSnapshot;

    // motion_level string -> motion scalar. "absent"/"none"/"still"/"idle"/""
    // are non-moving; anything else (walking, …) is motion. `fallback` is used
    // when the field is absent so a partial per-node payload defers to the
    // room aggregate rather than silently reading 0.
    fn motion_of(level: Option<&str>, fallback: f64) -> f64 {
        match level {
            Some("none") | Some("still") | Some("idle") | Some("absent") | Some("") => 0.0,
            Some(_) => 1.0,
            None => fallback,
        }
    }

    let ts = (v["timestamp"].as_f64().unwrap_or(0.0) * 1000.0) as i64;
    let vit = &v["vital_signs"];
    let breathing = vit["breathing_rate_bpm"].as_f64();
    let hr = vit["heart_rate_bpm"].as_f64();
    let n_persons = v["persons"]
        .as_array()
        .map(|a| a.len() as u32)
        .or_else(|| v["estimated_persons"].as_u64().map(|x| x as u32))
        .unwrap_or(0);

    // Room-level aggregate: the no-nodes fallback, and the per-node default for
    // any field a node omits.
    let acls = &v["classification"];
    let agg_presence = acls["presence"].as_bool().unwrap_or(false);
    let agg_motion = motion_of(acls["motion_level"].as_str(), 0.0);
    let agg_conf = acls["confidence"].as_f64().unwrap_or(0.0);

    let mk = |node_id: String, presence: bool, motion: f64, conf: f64, rssi: Option<f64>| {
        VitalsSnapshot {
            node_id,
            timestamp_ms: ts,
            presence,
            motion,
            presence_score: if presence { conf.max(0.0) } else { 0.0 },
            breathing_rate_bpm: breathing,
            heartrate_bpm: hr,
            n_persons,
            rssi_dbm: rssi,
            vital_confidence: conf,
            ..Default::default()
        }
    };

    match v["nodes"].as_array() {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .map(|node| {
                let n = node["node_id"].as_u64().unwrap_or(0);
                // Each node carries its OWN classification — use it, deferring to
                // the room aggregate only for fields the node omits.
                let ncls = &node["classification"];
                let presence = ncls["presence"].as_bool().unwrap_or(agg_presence);
                let motion = motion_of(ncls["motion_level"].as_str(), agg_motion);
                let conf = ncls["confidence"].as_f64().unwrap_or(agg_conf);
                mk(
                    format!("{base_id}-node{n}"),
                    presence,
                    motion,
                    conf,
                    node["rssi_dbm"].as_f64(),
                )
            })
            .collect(),
        _ => vec![mk(
            base_id.to_string(),
            agg_presence,
            agg_motion,
            agg_conf,
            v["nodes"][0]["rssi_dbm"].as_f64(),
        )],
    }
}

/// Build the multistatic guard config from the environment (#1031, #1049).
///
/// Three precedence layers, most-specific wins:
/// 1. `WDP_GUARD_INTERVAL_US` (+ optional `WDP_SOFT_GUARD_US`) — a **direct**
///    hard-guard override. This is the #1049 escape hatch: WiFi/ESP-NOW-synced
///    ESP32 nodes drift 10–150 ms (the 100 ms beacon + WiFi-MAC jitter cannot
///    hold two independently-clocked boards within the published default), so a
///    deployment can simply lift the guard past its measured spread (e.g.
///    `WDP_GUARD_INTERVAL_US=200000`) without knowing its exact TDM schedule.
/// 2. `WDP_TDM_SLOTS` + `WDP_TDM_SLOT_US` (both positive) — derive the guard
///    from the declared schedule via [`MultistaticConfig::for_tdm_schedule`].
/// 3. Otherwise the published default (60 ms hard / 20 ms soft).
///
/// The direct override (1) is applied **on top of** whichever base (2 or 3) is
/// selected, so `WDP_GUARD_INTERVAL_US` always wins for the hard guard while a
/// TDM-derived soft band is preserved unless it would exceed the new hard guard.
/// `min_nodes` is *not* set here — the caller overrides it for single-node
/// passthrough.
fn multistatic_guard_config_from_env() -> MultistaticConfig {
    multistatic_guard_config_from(
        std::env::var("WDP_TDM_SLOTS").ok().as_deref(),
        std::env::var("WDP_TDM_SLOT_US").ok().as_deref(),
        std::env::var("WDP_GUARD_INTERVAL_US").ok().as_deref(),
        std::env::var("WDP_SOFT_GUARD_US").ok().as_deref(),
    )
}

/// Pure core of [`multistatic_guard_config_from_env`] for testability.
fn multistatic_guard_config_from(
    slots: Option<&str>,
    slot_us: Option<&str>,
    guard_us: Option<&str>,
    soft_us: Option<&str>,
) -> MultistaticConfig {
    // Base: TDM-schedule-derived when both slot params are valid, else default.
    let mut cfg = match (
        slots.and_then(|s| s.trim().parse::<usize>().ok()),
        slot_us.and_then(|s| s.trim().parse::<u64>().ok()),
    ) {
        (Some(n), Some(us)) if n >= 1 && us >= 1 => MultistaticConfig::for_tdm_schedule(n, us),
        _ => MultistaticConfig::default(),
    };

    // Direct hard-guard override (#1049). Ignored when unset/zero/unparseable so
    // a malformed env var falls back to the base rather than breaking fusion.
    if let Some(g) = guard_us
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&g| g >= 1)
    {
        cfg.guard_interval_us = g;
        // Keep the soft band strictly below the (possibly lowered) hard guard.
        if cfg.soft_guard_us >= g {
            cfg.soft_guard_us = g.saturating_sub(1).max(1);
        }
    }

    // Optional explicit soft-guard override, always clamped strictly below hard.
    if let Some(s) = soft_us
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&s| s >= 1)
    {
        cfg.soft_guard_us = s.min(cfg.guard_interval_us.saturating_sub(1).max(1));
    }

    cfg
}

/// Turn a `ProgressiveLoader::new` failure into an actionable diagnostic (#894).
///
/// The published HuggingFace `ruvnet/wifi-densepose-pretrained` files
/// (`model.safetensors`, `model-q{2,4,8}.bin`, `model.rvf.jsonl`) are a
/// different *format* — and a different encoder architecture — than the RVF
/// binary container the `--model` progressive loader expects (`RVFS` magic
/// `0x52564653`). Feeding one to `--model` produced a bare
/// "invalid magic at offset 0 …" that left users stuck. Detect the common
/// cases and explain plainly what's loadable instead.
///
/// Superseded in the live load path by [`load_or_convert_model`] (which now
/// converts the convertible formats instead of just explaining), but retained
/// as the human-readable format-landscape summary and exercised by tests.
#[allow(dead_code)]
fn diagnose_model_load_error(path: &std::path::Path, data: &[u8], err: &str) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // safetensors: 8-byte LE header length, then a JSON object starting with '{'.
    let looks_safetensors = ext == "safetensors" || (data.len() > 9 && data[8] == b'{');
    // JSONL manifest: starts with '{' (or the well-known suffix).
    let looks_jsonl = ext == "jsonl" || name.ends_with(".rvf.jsonl") || data.first() == Some(&b'{');
    // Quantized weight blob shipped on HF (model-q2/q4/q8.bin).
    let looks_quant_bin = ext == "bin" || name.contains("-q");

    let kind = if looks_safetensors {
        "a safetensors weight file"
    } else if looks_jsonl {
        "a JSONL manifest, not the binary container"
    } else if looks_quant_bin {
        "a quantized weight blob (e.g. HuggingFace model-q4.bin)"
    } else {
        "not an RVF binary container"
    };

    format!(
        "model `{}` could not be loaded: it is {kind}. The --model flag expects an \
         RVF binary container (`RVFS` magic 0x52564653) produced by the \
         wifi-densepose-train pipeline. The HuggingFace ruvnet/wifi-densepose-pretrained \
         files are a different format and encoder architecture, so they do not load \
         here directly (issue #894). Continuing with signal heuristics. (loader: {err})",
        path.display()
    )
}

/// Load a model for `--model`, auto-detecting + converting the published
/// HuggingFace formats when the native RVF loader rejects them (issue #894).
///
/// Order of operations:
/// 1. Try the native RVF `ProgressiveLoader` (the only format with `RVFS` magic).
/// 2. On failure, **auto-detect** the format. If it is convertible
///    (`safetensors` / `model.rvf.jsonl`), convert it in-memory to RVF and load
///    that — so the published `model.safetensors` becomes loadable here.
/// 3. If it is a non-convertible format (quantized blob / unknown), return the
///    typed, actionable [`model_format::ModelLoadError`] message — never the
///    opaque "invalid magic …" string.
///
/// Returns the loaded `ProgressiveLoader` or a human-actionable error string.
fn load_or_convert_model(path: &std::path::Path, data: &[u8]) -> Result<ProgressiveLoader, String> {
    use model_format::{convert_to_rvf, detect_format, ModelFormat};

    // 1. Native RVF.
    if let Ok(loader) = ProgressiveLoader::new(data) {
        return Ok(loader);
    }

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let model_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("converted-model");

    match detect_format(data, &name) {
        // 2. Convertible formats: convert in-memory, then load.
        ModelFormat::Safetensors | ModelFormat::JsonlManifest => {
            match convert_to_rvf(data, &name, model_id) {
                Ok(rvf_bytes) => {
                    info!(
                        "Model `{}` is {} — converting to RVF in-memory and loading (issue #894)",
                        path.display(),
                        detect_format(data, &name).label()
                    );
                    ProgressiveLoader::new(&rvf_bytes).map_err(|e| {
                        format!(
                            "converted {} to RVF but the container failed to load: {e}",
                            detect_format(data, &name).label()
                        )
                    })
                }
                Err(conv_err) => Err(conv_err.to_string()),
            }
        }
        // 3. Non-convertible: typed actionable error.
        _ => Err(
            model_format::classify_load_failure(data, &name, "RVF container parse failed")
                .to_string(),
        ),
    }
}

/// `--convert-model` entry point (issue #894): read `in_path`, convert it to an
/// RVF binary container, write it to `out_path`, and verify the result loads.
/// Returns a process exit code (0 = success).
fn run_convert_model(in_path: &std::path::Path, out_path: &std::path::Path) -> i32 {
    let data = match std::fs::read(in_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("convert-model: failed to read {}: {e}", in_path.display());
            return 1;
        }
    };
    let name = in_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let model_id = in_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("converted-model");

    let detected = model_format::detect_format(&data, &name);
    eprintln!(
        "convert-model: detected {} ({} bytes)",
        detected.label(),
        data.len()
    );

    match model_format::convert_to_rvf(&data, &name, model_id) {
        Ok(rvf_bytes) => {
            // Verify the converted bytes actually load before writing.
            if let Err(e) = ProgressiveLoader::new(&rvf_bytes) {
                eprintln!("convert-model: produced RVF did NOT load (bug): {e}");
                return 1;
            }
            if let Err(e) = std::fs::write(out_path, &rvf_bytes) {
                eprintln!("convert-model: failed to write {}: {e}", out_path.display());
                return 1;
            }
            eprintln!(
                "convert-model: wrote {} ({} bytes). Load it with `--model {}`.",
                out_path.display(),
                rvf_bytes.len(),
                out_path.display()
            );
            0
        }
        Err(e) => {
            eprintln!("convert-model: {e}");
            1
        }
    }
}

/// Whether `--export-rvf` should emit the placeholder container-format demo.
///
/// It must only do so **standalone**. Combined with `--train`/`--pretrain` the
/// real model is produced by the training pipeline, so short-circuiting here
/// would silently skip training and write placeholder weights — the #894 bug
/// where the documented `--train … --export-rvf` workflow produced a fake model.
fn export_emits_placeholder_demo(export_set: bool, train: bool, pretrain: bool) -> bool {
    export_set && !train && !pretrain
}

// ── Main ─────────────────────────────────────────────────────────────────────

/// If `--ui-path` points nowhere (wrong cwd), try common repo layouts relative to cwd.
fn coalesce_ui_path(initial: std::path::PathBuf) -> std::path::PathBuf {
    if initial.is_dir() {
        return initial;
    }
    for rel in &["../ui", "./ui", "../../ui"] {
        let p = std::path::PathBuf::from(rel);
        if p.is_dir() {
            warn!(
                "UI path {} not found; using {} (set --ui-path explicitly if wrong)",
                initial.display(),
                p.display()
            );
            return p;
        }
    }
    initial
}

/// Keep persisted sensing data next to the UI project instead of depending on
/// whichever directory happened to launch the binary. This matters for the
/// browser control helper, which restarts the server without changing cwd.
fn data_dir_for_ui(ui_path: &FilePath) -> PathBuf {
    ui_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join("data"))
        .unwrap_or_else(|| PathBuf::from("data"))
}

#[cfg(test)]
mod data_dir_tests {
    use super::*;

    #[test]
    fn data_dir_follows_the_ui_project() {
        assert_eq!(
            data_dir_for_ui(FilePath::new("/project/software/ruview/ui")),
            PathBuf::from("/project/software/ruview/data")
        );
        assert_eq!(
            data_dir_for_ui(FilePath::new("../ui")),
            PathBuf::from("../data")
        );
    }

    #[test]
    fn bare_ui_path_keeps_the_current_directory_fallback() {
        assert_eq!(data_dir_for_ui(FilePath::new("ui")), PathBuf::from("data"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PositionOfflineMode {
    CreateSetup {
        spec: PathBuf,
        output: PathBuf,
    },
    Inspect {
        protocol: PositionInspectionProtocolArg,
        captures: Vec<PathBuf>,
        output: PathBuf,
    },
    BuildIndex {
        training_manifest: PathBuf,
        output: PathBuf,
    },
    Predict {
        index: PathBuf,
        captures: Vec<PathBuf>,
        output: PathBuf,
    },
    Evaluate {
        predictions: PathBuf,
        truth: PathBuf,
        output: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClassificationOfflineMode {
    Evaluate {
        predictions: PathBuf,
        truth: PathBuf,
        output: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExperimentOfflineMode {
    Evaluate {
        classification_report: PathBuf,
        position_report: PathBuf,
        output: PathBuf,
    },
}

fn requested_experiment_offline_mode(args: &Args) -> Result<Option<ExperimentOfflineMode>, String> {
    let supplied = usize::from(args.experiment_classification_report.is_some())
        + usize::from(args.experiment_position_report.is_some())
        + usize::from(args.experiment_output.is_some());
    if supplied == 0 {
        return Ok(None);
    }
    if supplied != 3 {
        return Err(
            "combined experiment evaluation requires --experiment-classification-report, --experiment-position-report, and --experiment-output together"
                .to_string(),
        );
    }
    let conflicts = args.classification_evaluate.is_some()
        || args.classification_truth.is_some()
        || args.classification_output.is_some()
        || args.replay_calibration.is_some()
        || !args.replay_measurement.is_empty()
        || args.replay_report.is_some()
        || args.position_create_setup.is_some()
        || args.position_inspect.is_some()
        || args.position_build_index.is_some()
        || args.position_predict.is_some()
        || args.position_evaluate.is_some()
        || !args.position_capture.is_empty()
        || args.position_truth.is_some()
        || args.position_output.is_some()
        || args.position_setup.is_some()
        || args.position_index.is_some()
        || args.position_index_sha256.is_some()
        || args.benchmark
        || args.load_rvf.is_some()
        || args.save_rvf.is_some()
        || args.model.is_some()
        || args.progressive
        || args.export_rvf.is_some()
        || args.convert_model.is_some()
        || args.convert_out.is_some()
        || args.train
        || args.dataset.is_some()
        || args.checkpoint_dir.is_some()
        || args.pretrain
        || args.embed
        || args.build_index.is_some()
        || args.calibrate;
    if conflicts {
        return Err(
            "combined experiment evaluation cannot be combined with another server or offline mode"
                .to_string(),
        );
    }
    Ok(Some(ExperimentOfflineMode::Evaluate {
        classification_report: args
            .experiment_classification_report
            .clone()
            .expect("all experiment arguments were checked"),
        position_report: args
            .experiment_position_report
            .clone()
            .expect("all experiment arguments were checked"),
        output: args
            .experiment_output
            .clone()
            .expect("all experiment arguments were checked"),
    }))
}

fn requested_classification_offline_mode(
    args: &Args,
) -> Result<Option<ClassificationOfflineMode>, String> {
    let supplied = usize::from(args.classification_evaluate.is_some())
        + usize::from(args.classification_truth.is_some())
        + usize::from(args.classification_output.is_some());
    if supplied == 0 {
        return Ok(None);
    }
    if supplied != 3 {
        return Err(
            "classification evaluation requires --classification-evaluate, --classification-truth, and --classification-output together"
                .to_string(),
        );
    }
    let conflicts = args.replay_calibration.is_some()
        || !args.replay_measurement.is_empty()
        || args.replay_report.is_some()
        || args.position_create_setup.is_some()
        || args.position_inspect.is_some()
        || args.position_build_index.is_some()
        || args.position_predict.is_some()
        || args.position_evaluate.is_some()
        || !args.position_capture.is_empty()
        || args.position_truth.is_some()
        || args.position_output.is_some()
        || args.position_setup.is_some()
        || args.position_index.is_some()
        || args.position_index_sha256.is_some()
        || args.experiment_classification_report.is_some()
        || args.experiment_position_report.is_some()
        || args.experiment_output.is_some()
        || args.benchmark
        || args.load_rvf.is_some()
        || args.save_rvf.is_some()
        || args.model.is_some()
        || args.progressive
        || args.export_rvf.is_some()
        || args.convert_model.is_some()
        || args.convert_out.is_some()
        || args.train
        || args.dataset.is_some()
        || args.checkpoint_dir.is_some()
        || args.pretrain
        || args.embed
        || args.build_index.is_some()
        || args.calibrate;
    if conflicts {
        return Err(
            "classification evaluation cannot be combined with another server, replay, position, model, benchmark, or training mode"
                .to_string(),
        );
    }
    Ok(Some(ClassificationOfflineMode::Evaluate {
        predictions: args
            .classification_evaluate
            .clone()
            .expect("all classification arguments were checked"),
        truth: args
            .classification_truth
            .clone()
            .expect("all classification arguments were checked"),
        output: args
            .classification_output
            .clone()
            .expect("all classification arguments were checked"),
    }))
}

fn requested_position_offline_mode(args: &Args) -> Result<Option<PositionOfflineMode>, String> {
    let mode_count = usize::from(args.position_create_setup.is_some())
        + usize::from(args.position_inspect.is_some())
        + usize::from(args.position_build_index.is_some())
        + usize::from(args.position_predict.is_some())
        + usize::from(args.position_evaluate.is_some());
    let has_position_adjuncts = !args.position_capture.is_empty()
        || args.position_truth.is_some()
        || args.position_output.is_some();
    if mode_count == 0 {
        if has_position_adjuncts {
            return Err(
                "--position-capture, --position-truth, and --position-output require one position mode"
                    .to_string(),
            );
        }
        return Ok(None);
    }
    if mode_count != 1 {
        return Err(
            "choose exactly one of --position-create-setup, --position-inspect, --position-build-index, --position-predict, or --position-evaluate"
                .to_string(),
        );
    }

    let conflicts_with_other_mode = args.replay_calibration.is_some()
        || !args.replay_measurement.is_empty()
        || args.replay_report.is_some()
        || args.classification_evaluate.is_some()
        || args.classification_truth.is_some()
        || args.classification_output.is_some()
        || args.experiment_classification_report.is_some()
        || args.experiment_position_report.is_some()
        || args.experiment_output.is_some()
        || args.benchmark
        || args.load_rvf.is_some()
        || args.save_rvf.is_some()
        || args.model.is_some()
        || args.progressive
        || args.export_rvf.is_some()
        || args.convert_model.is_some()
        || args.convert_out.is_some()
        || args.train
        || args.dataset.is_some()
        || args.checkpoint_dir.is_some()
        || args.pretrain
        || args.embed
        || args.build_index.is_some()
        || args.position_setup.is_some()
        || args.position_index.is_some()
        || args.position_index_sha256.is_some()
        || args.calibrate;
    if conflicts_with_other_mode {
        return Err("position offline modes cannot be combined with another server, replay, model, benchmark, or training mode".to_string());
    }
    let output = args
        .position_output
        .clone()
        .ok_or_else(|| "position offline mode requires --position-output <OUTPUT>".to_string())?;

    if let Some(spec) = args.position_create_setup.clone() {
        if !args.position_capture.is_empty() || args.position_truth.is_some() {
            return Err(
                "--position-create-setup does not accept --position-capture or --position-truth"
                    .to_string(),
            );
        }
        return Ok(Some(PositionOfflineMode::CreateSetup { spec, output }));
    }

    if let Some(protocol) = args.position_inspect {
        if args.position_capture.is_empty() {
            return Err(
                "--position-inspect requires at least one --position-capture <RAW_CAPTURE>"
                    .to_string(),
            );
        }
        if args.position_truth.is_some() {
            return Err("--position-inspect does not accept --position-truth".to_string());
        }
        return Ok(Some(PositionOfflineMode::Inspect {
            protocol,
            captures: args.position_capture.clone(),
            output,
        }));
    }

    if let Some(training_manifest) = args.position_build_index.clone() {
        if !args.position_capture.is_empty() || args.position_truth.is_some() {
            return Err(
                "--position-build-index does not accept --position-capture or --position-truth"
                    .to_string(),
            );
        }
        return Ok(Some(PositionOfflineMode::BuildIndex {
            training_manifest,
            output,
        }));
    }
    if let Some(index) = args.position_predict.clone() {
        if args.position_capture.is_empty() {
            return Err(
                "--position-predict requires at least one --position-capture <RAW_CAPTURE>"
                    .to_string(),
            );
        }
        if args.position_truth.is_some() {
            return Err(
                "--position-predict must not receive --position-truth; truth is evaluated separately"
                    .to_string(),
            );
        }
        return Ok(Some(PositionOfflineMode::Predict {
            index,
            captures: args.position_capture.clone(),
            output,
        }));
    }

    let predictions = args
        .position_evaluate
        .clone()
        .expect("exactly one position mode was selected");
    if !args.position_capture.is_empty() {
        return Err("--position-evaluate does not accept --position-capture".to_string());
    }
    let truth = args
        .position_truth
        .clone()
        .ok_or_else(|| "--position-evaluate requires --position-truth <TRUTH>".to_string())?;
    Ok(Some(PositionOfflineMode::Evaluate {
        predictions,
        truth,
        output,
    }))
}

fn validate_position_setup_server_mode(args: &Args) -> Result<(), String> {
    if args.position_index.is_some() != args.position_index_sha256.is_some() {
        return Err(
            "--position-index and --position-index-sha256 must be supplied together".to_string(),
        );
    }
    if let Some(sha256) = args.position_index_sha256.as_deref() {
        if !is_lowercase_sha256(sha256) {
            return Err(
                "--position-index-sha256 must be exactly 64 lowercase hexadecimal characters"
                    .to_string(),
            );
        }
        if args.position_setup.is_none() {
            return Err("--position-index requires --position-setup".to_string());
        }
    }
    if args.position_setup.is_none() {
        return Ok(());
    }
    if args.csi_grid_pin.is_some() {
        return Err(
            "--csi-grid-pin cannot be combined with --position-setup; the sealed receiver grids are authoritative"
                .to_string(),
        );
    }
    let conflicts_with_normal_server = args.replay_calibration.is_some()
        || !args.replay_measurement.is_empty()
        || args.replay_report.is_some()
        || args.benchmark
        || args.convert_model.is_some()
        || args.convert_out.is_some()
        || args.export_rvf.is_some()
        || args.train
        || args.pretrain
        || args.embed
        || args.build_index.is_some();
    if conflicts_with_normal_server {
        return Err("--position-setup is only valid for a normal sensing-server start".to_string());
    }
    Ok(())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn run_position_offline_mode(mode: PositionOfflineMode) -> Result<PathBuf, String> {
    match mode {
        PositionOfflineMode::CreateSetup { spec, output } => {
            let setup = position_setup::create_position_setup(&spec)?;
            position_artifact::write_pretty_json_no_clobber(&output, &setup)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
        PositionOfflineMode::Inspect {
            protocol,
            captures,
            output,
        } => {
            let protocol = match protocol {
                PositionInspectionProtocolArg::EmptyCalibration => {
                    position_offline::PositionInspectionProtocol::EmptyCalibration
                }
                PositionInspectionProtocolArg::Position => {
                    position_offline::PositionInspectionProtocol::Position
                }
            };
            let inspection = position_offline::inspect_captures(&captures, protocol)?;
            position_artifact::write_pretty_json_no_clobber(&output, &inspection)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
        PositionOfflineMode::BuildIndex {
            training_manifest,
            output,
        } => {
            let index = position_offline::build_index(&training_manifest)?;
            position_artifact::write_pretty_json_no_clobber(&output, &index)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
        PositionOfflineMode::Predict {
            index,
            captures,
            output,
        } => {
            let predictions = position_offline::predict_blind(&index, &captures)?;
            position_artifact::write_pretty_json_no_clobber(&output, &predictions)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
        PositionOfflineMode::Evaluate {
            predictions,
            truth,
            output,
        } => {
            let report = position_offline::evaluate_predictions(&predictions, &truth)?;
            position_artifact::write_pretty_json_no_clobber(&output, &report)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
    }
}

fn run_classification_offline_mode(mode: ClassificationOfflineMode) -> Result<PathBuf, String> {
    match mode {
        ClassificationOfflineMode::Evaluate {
            predictions,
            truth,
            output,
        } => {
            let report = classification_evaluation::evaluate_files(&predictions, &truth)?;
            position_artifact::write_pretty_json_no_clobber(&output, &report)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
    }
}

fn run_experiment_offline_mode(mode: ExperimentOfflineMode) -> Result<PathBuf, String> {
    match mode {
        ExperimentOfflineMode::Evaluate {
            classification_report,
            position_report,
            output,
        } => {
            let report =
                experiment_evaluation::evaluate_files(&classification_report, &position_report)?;
            position_artifact::write_pretty_json_no_clobber(&output, &report)
                .map_err(|error| error.to_string())?;
            Ok(output)
        }
    }
}

#[cfg(test)]
mod position_offline_cli_tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Args {
        Args::try_parse_from(arguments).expect("valid CLI syntax")
    }

    #[test]
    fn position_modes_are_absent_for_a_normal_server_start() {
        assert_eq!(
            requested_position_offline_mode(&parse(&["sensing-server"])).unwrap(),
            None
        );
    }

    #[test]
    fn build_mode_is_strict_and_complete() {
        let args = parse(&[
            "sensing-server",
            "--position-build-index",
            "training.json",
            "--position-output",
            "index.json",
        ]);
        assert_eq!(
            requested_position_offline_mode(&args).unwrap(),
            Some(PositionOfflineMode::BuildIndex {
                training_manifest: PathBuf::from("training.json"),
                output: PathBuf::from("index.json"),
            })
        );

        let missing_mode = parse(&["sensing-server", "--position-output", "orphan.json"]);
        assert!(requested_position_offline_mode(&missing_mode).is_err());
    }

    #[test]
    fn setup_creation_is_an_exclusive_no_clobber_offline_mode() {
        let args = parse(&[
            "sensing-server",
            "--position-create-setup",
            "setup-spec.json",
            "--position-output",
            "sealed-setup.json",
        ]);
        assert_eq!(
            requested_position_offline_mode(&args).unwrap(),
            Some(PositionOfflineMode::CreateSetup {
                spec: PathBuf::from("setup-spec.json"),
                output: PathBuf::from("sealed-setup.json"),
            })
        );

        let conflict = parse(&[
            "sensing-server",
            "--position-create-setup",
            "setup-spec.json",
            "--position-setup",
            "old-sealed-setup.json",
            "--position-output",
            "sealed-setup.json",
        ]);
        assert!(requested_position_offline_mode(&conflict).is_err());
    }

    #[test]
    fn sealed_setup_is_allowed_only_for_a_normal_server_start() {
        let normal = parse(&["sensing-server", "--position-setup", "sealed-setup.json"]);
        assert_eq!(requested_position_offline_mode(&normal).unwrap(), None);
        validate_position_setup_server_mode(&normal).unwrap();

        let replay = parse(&[
            "sensing-server",
            "--position-setup",
            "sealed-setup.json",
            "--replay-calibration",
            "empty.raw-csi.v1.jsonl",
        ]);
        assert!(validate_position_setup_server_mode(&replay).is_err());

        let duplicate_grid_authority = parse(&[
            "sensing-server",
            "--position-setup",
            "sealed-setup.json",
            "--csi-grid-pin",
            "2437,1,64,0,0",
        ]);
        assert!(validate_position_setup_server_mode(&duplicate_grid_authority).is_err());
    }

    #[test]
    fn live_position_index_cli_is_paired_pinned_and_setup_bound() {
        let sha256 = "a".repeat(64);
        let valid = Args::try_parse_from([
            "sensing-server",
            "--position-setup",
            "sealed-setup.json",
            "--position-index",
            "position-index.json",
            "--position-index-sha256",
            &sha256,
        ])
        .unwrap();
        validate_position_setup_server_mode(&valid).unwrap();

        let missing_sha = parse(&[
            "sensing-server",
            "--position-setup",
            "sealed-setup.json",
            "--position-index",
            "position-index.json",
        ]);
        assert!(validate_position_setup_server_mode(&missing_sha).is_err());

        let missing_setup = Args::try_parse_from([
            "sensing-server",
            "--position-index",
            "position-index.json",
            "--position-index-sha256",
            &sha256,
        ])
        .unwrap();
        assert!(validate_position_setup_server_mode(&missing_setup).is_err());

        let uppercase_hash = parse(&[
            "sensing-server",
            "--position-setup",
            "sealed-setup.json",
            "--position-index",
            "position-index.json",
            "--position-index-sha256",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ]);
        assert!(validate_position_setup_server_mode(&uppercase_hash).is_err());
    }

    #[test]
    fn capture_inspection_requires_a_protocol_and_unlabelled_capture() {
        let args = parse(&[
            "sensing-server",
            "--position-inspect",
            "empty-calibration",
            "--position-capture",
            "empty.raw-csi.v1.jsonl",
            "--position-output",
            "empty-inspection.json",
        ]);
        assert_eq!(
            requested_position_offline_mode(&args).unwrap(),
            Some(PositionOfflineMode::Inspect {
                protocol: PositionInspectionProtocolArg::EmptyCalibration,
                captures: vec![PathBuf::from("empty.raw-csi.v1.jsonl")],
                output: PathBuf::from("empty-inspection.json"),
            })
        );

        let missing_capture = parse(&[
            "sensing-server",
            "--position-inspect",
            "position",
            "--position-output",
            "inspection.json",
        ]);
        assert!(requested_position_offline_mode(&missing_capture).is_err());
    }

    #[test]
    fn blind_prediction_cannot_receive_truth() {
        let missing_capture = parse(&[
            "sensing-server",
            "--position-predict",
            "index.json",
            "--position-output",
            "predictions.json",
        ]);
        assert!(requested_position_offline_mode(&missing_capture).is_err());

        let leaked_truth = parse(&[
            "sensing-server",
            "--position-predict",
            "index.json",
            "--position-capture",
            "blind.raw-csi.v1.jsonl",
            "--position-truth",
            "truth.json",
            "--position-output",
            "predictions.json",
        ]);
        assert!(requested_position_offline_mode(&leaked_truth).is_err());
    }

    #[test]
    fn evaluation_requires_truth_and_rejects_replay_conflicts() {
        let missing_truth = parse(&[
            "sensing-server",
            "--position-evaluate",
            "predictions.json",
            "--position-output",
            "report.json",
        ]);
        assert!(requested_position_offline_mode(&missing_truth).is_err());

        let conflict = parse(&[
            "sensing-server",
            "--position-evaluate",
            "predictions.json",
            "--position-truth",
            "truth.json",
            "--position-output",
            "report.json",
            "--replay-calibration",
            "empty.raw-csi.v1.jsonl",
        ]);
        assert!(requested_position_offline_mode(&conflict).is_err());
    }

    #[test]
    fn classification_evaluation_requires_three_separate_artifacts() {
        let valid = parse(&[
            "sensing-server",
            "--classification-evaluate",
            "classification-predictions.json",
            "--classification-truth",
            "classification-truth.json",
            "--classification-output",
            "classification-report.json",
        ]);
        assert_eq!(
            requested_classification_offline_mode(&valid).unwrap(),
            Some(ClassificationOfflineMode::Evaluate {
                predictions: PathBuf::from("classification-predictions.json"),
                truth: PathBuf::from("classification-truth.json"),
                output: PathBuf::from("classification-report.json"),
            })
        );

        let missing_truth = parse(&[
            "sensing-server",
            "--classification-evaluate",
            "classification-predictions.json",
            "--classification-output",
            "classification-report.json",
        ]);
        assert!(requested_classification_offline_mode(&missing_truth).is_err());
    }

    #[test]
    fn classification_evaluation_rejects_replay_and_position_modes() {
        let replay_conflict = parse(&[
            "sensing-server",
            "--classification-evaluate",
            "classification-predictions.json",
            "--classification-truth",
            "classification-truth.json",
            "--classification-output",
            "classification-report.json",
            "--replay-calibration",
            "empty.raw-csi.v1.jsonl",
        ]);
        assert!(requested_classification_offline_mode(&replay_conflict).is_err());

        let position_conflict = parse(&[
            "sensing-server",
            "--position-evaluate",
            "position-predictions.json",
            "--position-truth",
            "position-truth.json",
            "--position-output",
            "position-report.json",
            "--classification-evaluate",
            "classification-predictions.json",
            "--classification-truth",
            "classification-truth.json",
            "--classification-output",
            "classification-report.json",
        ]);
        assert!(requested_position_offline_mode(&position_conflict).is_err());
    }

    #[test]
    fn combined_experiment_evaluation_is_complete_and_exclusive() {
        let valid = parse(&[
            "sensing-server",
            "--experiment-classification-report",
            "classification-report.json",
            "--experiment-position-report",
            "position-report.json",
            "--experiment-output",
            "experiment-report.json",
        ]);
        assert_eq!(
            requested_experiment_offline_mode(&valid).unwrap(),
            Some(ExperimentOfflineMode::Evaluate {
                classification_report: PathBuf::from("classification-report.json"),
                position_report: PathBuf::from("position-report.json"),
                output: PathBuf::from("experiment-report.json"),
            })
        );

        let incomplete = parse(&[
            "sensing-server",
            "--experiment-classification-report",
            "classification-report.json",
            "--experiment-output",
            "experiment-report.json",
        ]);
        assert!(requested_experiment_offline_mode(&incomplete).is_err());

        let conflict = parse(&[
            "sensing-server",
            "--experiment-classification-report",
            "classification-report.json",
            "--experiment-position-report",
            "position-report.json",
            "--experiment-output",
            "experiment-report.json",
            "--replay-calibration",
            "empty.raw-csi.v1.jsonl",
        ]);
        assert!(requested_experiment_offline_mode(&conflict).is_err());
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let mut args = Args::parse();

    if args.server_control_daemon {
        let Some(spec_path) = args.server_control_spec.take() else {
            eprintln!("Server-Control-Daemon benötigt --server-control-spec");
            std::process::exit(2);
        };
        if let Err(error) = server_control::run_daemon(server_control::DaemonConfig {
            control_port: args.server_control_port,
            ui_port: args.server_control_ui_port,
            spec_path,
            adopt_pid: args.server_control_adopt_pid,
        })
        .await
        {
            eprintln!("Browser-Serversteuerung fehlgeschlagen: {error}");
            std::process::exit(1);
        }
        return;
    }

    let position_offline_mode = match requested_position_offline_mode(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("Position offline mode usage error: {error}");
            std::process::exit(2);
        }
    };
    if let Some(mode) = position_offline_mode {
        match run_position_offline_mode(mode) {
            Ok(output) => {
                eprintln!("Position artifact written to {}", output.display());
                return;
            }
            Err(error) => {
                eprintln!("Position offline mode failed: {error}");
                std::process::exit(1);
            }
        }
    }

    let classification_offline_mode = match requested_classification_offline_mode(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("Classification offline mode usage error: {error}");
            std::process::exit(2);
        }
    };
    if let Some(mode) = classification_offline_mode {
        match run_classification_offline_mode(mode) {
            Ok(output) => {
                eprintln!("Classification evaluation written to {}", output.display());
                return;
            }
            Err(error) => {
                eprintln!("Classification evaluation failed: {error}");
                std::process::exit(1);
            }
        }
    }

    let experiment_offline_mode = match requested_experiment_offline_mode(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("Experiment offline mode usage error: {error}");
            std::process::exit(2);
        }
    };
    if let Some(mode) = experiment_offline_mode {
        match run_experiment_offline_mode(mode) {
            Ok(output) => {
                eprintln!("Combined experiment report written to {}", output.display());
                return;
            }
            Err(error) => {
                eprintln!("Combined experiment evaluation failed: {error}");
                std::process::exit(1);
            }
        }
    }

    if let Err(error) = validate_position_setup_server_mode(&args) {
        eprintln!("Position setup usage error: {error}");
        std::process::exit(2);
    }
    if let Err(error) = server_control::ensure_daemon(args.server_control_port, args.http_port) {
        warn!(%error, "Browser-Serversteuerung konnte nicht aktiviert werden");
    }
    if let Some(grid_pin) = args.csi_grid_pin {
        info!(
            "Pre-setup CSI grid pin active: {}MHz/{}ant/{}sc/ppdu{}/flags0x{:02x}",
            grid_pin.center_frequency_mhz,
            grid_pin.antenna_count,
            grid_pin.subcarrier_count,
            grid_pin.ppdu_type,
            grid_pin.layout_flags,
        );
    }
    let position_setup = match args.position_setup.as_deref() {
        Some(path) => match position_setup::load_position_setup_for_current_executable(path) {
            Ok(setup) => {
                eprintln!(
                    "Validated sealed position setup {} ({})",
                    setup.setup_id(),
                    setup.setup_sha256()
                );
                Some(Arc::new(setup))
            }
            Err(error) => {
                eprintln!("Position setup validation failed: {error}");
                std::process::exit(1);
            }
        },
        None => None,
    };
    let runtime_position_geometry =
        match resolve_runtime_position_geometry(&args, position_setup.as_deref()) {
            Ok(geometry) => geometry,
            Err(error) => {
                eprintln!("Position setup geometry usage error: {error}");
                std::process::exit(2);
            }
        };
    let live_position_runtime = match (
        args.position_index.as_deref(),
        args.position_index_sha256.as_deref(),
    ) {
        (Some(index_path), Some(expected_index_sha256)) => {
            let setup = position_setup
                .as_deref()
                .expect("live position CLI validation requires a sealed setup");
            match position_live::PositionIndexRuntime::load(
                index_path,
                setup.setup_id(),
                setup.setup_sha256(),
                Some(expected_index_sha256),
            ) {
                Ok(runtime) => {
                    eprintln!(
                        "Live position index active: sha256={} setup_id={}",
                        runtime.index_sha256(),
                        runtime.setup_id(),
                    );
                    Some(runtime)
                }
                Err(_) => {
                    eprintln!(
                        "Live position index activation failed: exact index bytes did not pass \
                         the required setup/hash/schema validation"
                    );
                    std::process::exit(1);
                }
            }
        }
        (None, None) => {
            eprintln!("Live position index inactive");
            None
        }
        _ => unreachable!("live position CLI pairing was validated before setup loading"),
    };

    let replay_requested = args.replay_calibration.is_some()
        || !args.replay_measurement.is_empty()
        || args.replay_report.is_some();
    if replay_requested {
        let Some(calibration_path) = args.replay_calibration.as_deref() else {
            eprintln!("Replay requires --replay-calibration <EMPTY.raw-csi.v1.jsonl>.");
            std::process::exit(2);
        };
        if args.replay_measurement.is_empty() {
            eprintln!(
                "Replay requires at least one --replay-measurement <CAPTURE.raw-csi.v1.jsonl>."
            );
            std::process::exit(2);
        }

        let report = match raw_csi_replay::run(calibration_path, &args.replay_measurement) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("Raw CSI replay failed: {error}");
                std::process::exit(1);
            }
        };
        let encoded = match serde_json::to_string_pretty(&report) {
            Ok(encoded) => format!("{encoded}\n"),
            Err(error) => {
                eprintln!("Raw CSI replay report serialization failed: {error}");
                std::process::exit(1);
            }
        };
        if let Some(report_path) = args.replay_report.as_deref() {
            if let Err(error) = std::fs::write(report_path, encoded.as_bytes()) {
                eprintln!(
                    "Raw CSI replay report could not be written to {}: {error}",
                    report_path.display()
                );
                std::process::exit(1);
            }
            eprintln!("Raw CSI replay report written to {}", report_path.display());
        } else {
            print!("{encoded}");
        }
        return;
    }

    args.ui_path = coalesce_ui_path(args.ui_path);
    let data_dir = data_dir_for_ui(&args.ui_path);

    // Handle --benchmark mode: run vital sign benchmark and exit
    if args.benchmark {
        eprintln!("Running vital sign detection benchmark (1000 frames)...");
        let (total, per_frame) = vital_signs::run_benchmark(1000);
        eprintln!();
        eprintln!("Summary: {total:?} total, {per_frame:?} per frame");
        return;
    }

    // Handle --convert-model: turn a published HF model file (safetensors /
    // model.rvf.jsonl) into the RVF binary container --model expects, then exit
    // (issue #894). Gives the reporter a one-command path off the heuristics.
    if let Some(ref in_path) = args.convert_model {
        let out_path = args
            .convert_out
            .clone()
            .unwrap_or_else(|| in_path.with_extension("rvf"));
        std::process::exit(run_convert_model(in_path, &out_path));
    }

    // Handle --export-rvf: writes a CONTAINER-FORMAT DEMO with placeholder
    // weights — it is NOT a trained model. Only short-circuit when standalone:
    // combined with --train/--pretrain the real model is exported by the
    // training pipeline, and short-circuiting here would silently skip training
    // and write placeholder weights (#894 — the documented `--train …
    // --export-rvf` workflow produced a placeholder and never trained).
    if export_emits_placeholder_demo(args.export_rvf.is_some(), args.train, args.pretrain) {
        let rvf_path = args
            .export_rvf
            .as_ref()
            .expect("export_emits_placeholder_demo implies export_rvf is set");
        eprintln!(
            "WARNING: --export-rvf writes a CONTAINER-FORMAT DEMO with placeholder \
             weights — it is NOT a trained model. Train one with \
             `--train --dataset <DIR>` (which exports a calibrated .rvf to the \
             models/ directory), or download a pretrained encoder. See issue #894."
        );
        eprintln!("Exporting RVF container package (placeholder weights)...");
        use rvf_pipeline::RvfModelBuilder;

        let mut builder = RvfModelBuilder::new("wifi-densepose", "1.0.0");

        // Vital sign config (default breathing 0.1-0.5 Hz, heartbeat 0.8-2.0 Hz)
        builder.set_vital_config(0.1, 0.5, 0.8, 2.0);

        // Model profile (input/output spec)
        builder.set_model_profile(
            "56-subcarrier CSI amplitude/phase @ 10-100 Hz",
            "17 COCO keypoints + body part UV + vital signs",
            "ESP32-S3 or Windows WiFi RSSI, Rust 1.85+",
        );

        // Placeholder weights (17 keypoints × 56 subcarriers × 3 dims = 2856 params)
        let placeholder_weights: Vec<f32> = (0..2856).map(|i| (i as f32 * 0.001).sin()).collect();
        builder.set_weights(&placeholder_weights);

        // Training provenance
        builder.set_training_proof(
            "wifi-densepose-rs-v1.0.0",
            serde_json::json!({
                "pipeline": "ADR-023 8-phase",
                "test_count": 229,
                "benchmark_fps": 9520,
                "framework": "wifi-densepose-rs",
            }),
        );

        // SONA default environment profile
        let default_lora: Vec<f32> = vec![0.0; 64];
        builder.add_sona_profile("default", &default_lora, &default_lora);

        match builder.build() {
            Ok(rvf_bytes) => {
                if let Err(e) = std::fs::write(rvf_path, &rvf_bytes) {
                    eprintln!("Error writing RVF: {e}");
                    std::process::exit(1);
                }
                eprintln!("Wrote {} bytes to {}", rvf_bytes.len(), rvf_path.display());
                eprintln!("RVF container exported successfully.");
            }
            Err(e) => {
                eprintln!("Error building RVF: {e}");
                std::process::exit(1);
            }
        }
        return;
    } else if args.export_rvf.is_some() {
        // --export-rvf alongside --train/--pretrain: don't emit a placeholder.
        // Fall through so training runs; it exports the real calibrated model.
        eprintln!(
            "Note: --export-rvf is ignored in training mode — the trained model \
             is exported by the training pipeline to the models/ directory."
        );
    }

    // Handle --pretrain mode: self-supervised contrastive pretraining (ADR-024)
    if args.pretrain {
        eprintln!("=== WiFi-DensePose Contrastive Pretraining (ADR-024) ===");

        let ds_path = args
            .dataset
            .clone()
            .unwrap_or_else(|| PathBuf::from("data"));
        let source = match args.dataset_type.as_str() {
            "wipose" => dataset::DataSource::WiPose(ds_path.clone()),
            _ => dataset::DataSource::MmFi(ds_path.clone()),
        };
        let pipeline = dataset::DataPipeline::new(dataset::DataConfig {
            source,
            ..Default::default()
        });

        // Generate synthetic or load real CSI windows
        let generate_synthetic_windows = || -> Vec<Vec<Vec<f32>>> {
            (0..50)
                .map(|i| {
                    (0..4)
                        .map(|a| {
                            (0..56)
                                .map(|s| ((i * 7 + a * 13 + s) as f32 * 0.31).sin() * 0.5)
                                .collect()
                        })
                        .collect()
                })
                .collect()
        };

        let csi_windows: Vec<Vec<Vec<f32>>> = match pipeline.load() {
            Ok(s) if !s.is_empty() => {
                eprintln!("Loaded {} samples from {}", s.len(), ds_path.display());
                s.into_iter().map(|s| s.csi_window).collect()
            }
            _ => {
                eprintln!("Using synthetic data for pretraining.");
                generate_synthetic_windows()
            }
        };

        let n_subcarriers = csi_windows
            .first()
            .and_then(|w| w.first())
            .map(|f| f.len())
            .unwrap_or(56);

        let tf_config = graph_transformer::TransformerConfig {
            n_subcarriers,
            n_keypoints: 17,
            d_model: 64,
            n_heads: 4,
            n_gnn_layers: 2,
        };
        let transformer = graph_transformer::CsiToPoseTransformer::new(tf_config);
        eprintln!("Transformer params: {}", transformer.param_count());

        let trainer_config = trainer::TrainerConfig {
            epochs: args.pretrain_epochs,
            batch_size: 8,
            lr: 0.001,
            warmup_epochs: 2,
            min_lr: 1e-6,
            early_stop_patience: args.pretrain_epochs + 1,
            pretrain_temperature: 0.07,
            ..Default::default()
        };
        let mut t = trainer::Trainer::with_transformer(trainer_config, transformer);

        let e_config = embedding::EmbeddingConfig {
            d_model: 64,
            d_proj: 128,
            temperature: 0.07,
            normalize: true,
        };
        let mut projection = embedding::ProjectionHead::new(e_config.clone());
        let augmenter = embedding::CsiAugmenter::new();

        eprintln!(
            "Starting contrastive pretraining for {} epochs...",
            args.pretrain_epochs
        );
        let start = std::time::Instant::now();
        for epoch in 0..args.pretrain_epochs {
            let loss = t.pretrain_epoch(&csi_windows, &augmenter, &mut projection, 0.07, epoch);
            if epoch % 10 == 0 || epoch == args.pretrain_epochs - 1 {
                eprintln!("  Epoch {epoch}: contrastive loss = {loss:.4}");
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        eprintln!("Pretraining complete in {elapsed:.1}s");

        // Save pretrained model as RVF with embedding segment
        if let Some(ref save_path) = args.save_rvf {
            eprintln!("Saving pretrained model to RVF: {}", save_path.display());
            t.sync_transformer_weights();
            let weights = t.params().to_vec();
            let mut proj_weights = Vec::new();
            projection.flatten_into(&mut proj_weights);

            let mut builder = RvfBuilder::new();
            builder.add_manifest(
                "wifi-densepose-pretrained",
                env!("CARGO_PKG_VERSION"),
                "WiFi DensePose contrastive pretrained model (ADR-024)",
            );
            builder.add_weights(&weights);
            builder.add_embedding(
                &serde_json::json!({
                    "d_model": e_config.d_model,
                    "d_proj": e_config.d_proj,
                    "temperature": e_config.temperature,
                    "normalize": e_config.normalize,
                    "pretrain_epochs": args.pretrain_epochs,
                }),
                &proj_weights,
            );
            match builder.write_to_file(save_path) {
                Ok(()) => eprintln!(
                    "RVF saved ({} transformer + {} projection params)",
                    weights.len(),
                    proj_weights.len()
                ),
                Err(e) => eprintln!("Failed to save RVF: {e}"),
            }
        }

        return;
    }

    // Handle --embed mode: extract embeddings from CSI data
    if args.embed {
        eprintln!("=== WiFi-DensePose Embedding Extraction (ADR-024) ===");

        let model_path = match &args.model {
            Some(p) => p.clone(),
            None => {
                eprintln!("Error: --embed requires --model <path> to a pretrained .rvf file");
                std::process::exit(1);
            }
        };

        let reader = match RvfReader::from_file(&model_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to load model: {e}");
                std::process::exit(1);
            }
        };

        let weights = reader.weights().unwrap_or_default();
        let (embed_config_json, proj_weights) = reader.embedding().unwrap_or_else(|| {
            eprintln!("Warning: no embedding segment in RVF, using defaults");
            (
                serde_json::json!({"d_model":64,"d_proj":128,"temperature":0.07,"normalize":true}),
                Vec::new(),
            )
        });

        let d_model = embed_config_json["d_model"].as_u64().unwrap_or(64) as usize;
        let d_proj = embed_config_json["d_proj"].as_u64().unwrap_or(128) as usize;

        let tf_config = graph_transformer::TransformerConfig {
            n_subcarriers: 56,
            n_keypoints: 17,
            d_model,
            n_heads: 4,
            n_gnn_layers: 2,
        };
        let e_config = embedding::EmbeddingConfig {
            d_model,
            d_proj,
            temperature: 0.07,
            normalize: true,
        };
        let mut extractor = embedding::EmbeddingExtractor::new(tf_config, e_config.clone());

        // Load transformer weights
        if !weights.is_empty() {
            if let Err(e) = extractor.transformer.unflatten_weights(&weights) {
                eprintln!("Warning: failed to load transformer weights: {e}");
            }
        }
        // Load projection weights
        if !proj_weights.is_empty() {
            let (proj, _) = embedding::ProjectionHead::unflatten_from(&proj_weights, &e_config);
            extractor.projection = proj;
        }

        // Load dataset and extract embeddings
        let _ds_path = args
            .dataset
            .clone()
            .unwrap_or_else(|| PathBuf::from("data"));
        let csi_windows: Vec<Vec<Vec<f32>>> = (0..10)
            .map(|i| {
                (0..4)
                    .map(|a| {
                        (0..56)
                            .map(|s| ((i * 7 + a * 13 + s) as f32 * 0.31).sin() * 0.5)
                            .collect()
                    })
                    .collect()
            })
            .collect();

        eprintln!(
            "Extracting embeddings from {} CSI windows...",
            csi_windows.len()
        );
        let embeddings = extractor.extract_batch(&csi_windows);
        for (i, emb) in embeddings.iter().enumerate() {
            let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            eprintln!("  Window {i}: {d_proj}-dim embedding, ||e|| = {norm:.4}");
        }
        eprintln!(
            "Extracted {} embeddings of dimension {d_proj}",
            embeddings.len()
        );

        return;
    }

    // Handle --build-index mode: build a fingerprint index from embeddings
    if let Some(ref index_type_str) = args.build_index {
        eprintln!("=== WiFi-DensePose Fingerprint Index Builder (ADR-024) ===");

        let index_type = match index_type_str.as_str() {
            "env" | "environment" => embedding::IndexType::EnvironmentFingerprint,
            "activity" => embedding::IndexType::ActivityPattern,
            "temporal" => embedding::IndexType::TemporalBaseline,
            "person" => embedding::IndexType::PersonTrack,
            _ => {
                eprintln!(
                    "Unknown index type '{}'. Use: env, activity, temporal, person",
                    index_type_str
                );
                std::process::exit(1);
            }
        };

        let tf_config = graph_transformer::TransformerConfig::default();
        let e_config = embedding::EmbeddingConfig::default();
        let mut extractor = embedding::EmbeddingExtractor::new(tf_config, e_config);

        // Generate synthetic CSI windows for demo
        let csi_windows: Vec<Vec<Vec<f32>>> = (0..20)
            .map(|i| {
                (0..4)
                    .map(|a| {
                        (0..56)
                            .map(|s| ((i * 7 + a * 13 + s) as f32 * 0.31).sin() * 0.5)
                            .collect()
                    })
                    .collect()
            })
            .collect();

        let mut index = embedding::FingerprintIndex::new(index_type);
        for (i, window) in csi_windows.iter().enumerate() {
            let emb = extractor.extract(window);
            index.insert(emb, format!("window_{i}"), i as u64 * 100);
        }

        eprintln!("Built {:?} index with {} entries", index_type, index.len());

        // Test a query
        let query_emb = extractor.extract(&csi_windows[0]);
        let results = index.search(&query_emb, 5);
        eprintln!("Top-5 nearest to window_0:");
        for r in &results {
            eprintln!(
                "  entry={}, distance={:.4}, metadata={}",
                r.entry, r.distance, r.metadata
            );
        }

        return;
    }

    // Handle --train mode: train a model and exit
    if args.train {
        eprintln!("=== WiFi-DensePose Training Mode ===");

        // Build data pipeline
        let ds_path = args
            .dataset
            .clone()
            .unwrap_or_else(|| PathBuf::from("data"));
        let source = match args.dataset_type.as_str() {
            "wipose" => dataset::DataSource::WiPose(ds_path.clone()),
            _ => dataset::DataSource::MmFi(ds_path.clone()),
        };
        let pipeline = dataset::DataPipeline::new(dataset::DataConfig {
            source,
            ..Default::default()
        });

        // Generate synthetic training data (50 samples with deterministic CSI + keypoints)
        let generate_synthetic = || -> Vec<dataset::TrainingSample> {
            (0..50)
                .map(|i| {
                    let csi: Vec<Vec<f32>> = (0..4)
                        .map(|a| {
                            (0..56)
                                .map(|s| ((i * 7 + a * 13 + s) as f32 * 0.31).sin() * 0.5)
                                .collect()
                        })
                        .collect();
                    let mut kps = [(0.0f32, 0.0f32, 1.0f32); 17];
                    for (k, kp) in kps.iter_mut().enumerate() {
                        kp.0 = (k as f32 * 0.1 + i as f32 * 0.02).sin() * 100.0 + 320.0;
                        kp.1 = (k as f32 * 0.15 + i as f32 * 0.03).cos() * 80.0 + 240.0;
                    }
                    dataset::TrainingSample {
                        csi_window: csi,
                        pose_label: dataset::PoseLabel {
                            keypoints: kps,
                            body_parts: Vec::new(),
                            confidence: 1.0,
                        },
                        source: "synthetic",
                    }
                })
                .collect()
        };

        // Load samples (fall back to synthetic if dataset missing/empty)
        let samples = match pipeline.load() {
            Ok(s) if !s.is_empty() => {
                eprintln!("Loaded {} samples from {}", s.len(), ds_path.display());
                s
            }
            Ok(_) => {
                eprintln!(
                    "No samples found at {}. Using synthetic data.",
                    ds_path.display()
                );
                generate_synthetic()
            }
            Err(e) => {
                eprintln!("Failed to load dataset: {e}. Using synthetic data.");
                generate_synthetic()
            }
        };

        // Convert dataset samples to trainer format
        let trainer_samples: Vec<trainer::TrainingSample> =
            samples.iter().map(trainer::from_dataset_sample).collect();

        // Split 80/20 train/val
        let split = (trainer_samples.len() * 4) / 5;
        let (train_data, val_data) = trainer_samples.split_at(split.max(1));
        eprintln!(
            "Train: {} samples, Val: {} samples",
            train_data.len(),
            val_data.len()
        );

        // Create transformer + trainer
        let n_subcarriers = train_data
            .first()
            .and_then(|s| s.csi_features.first())
            .map(|f| f.len())
            .unwrap_or(56);
        let tf_config = graph_transformer::TransformerConfig {
            n_subcarriers,
            n_keypoints: 17,
            d_model: 64,
            n_heads: 4,
            n_gnn_layers: 2,
        };
        let transformer = graph_transformer::CsiToPoseTransformer::new(tf_config);
        eprintln!("Transformer params: {}", transformer.param_count());

        let trainer_config = trainer::TrainerConfig {
            epochs: args.epochs,
            batch_size: 8,
            lr: 0.001,
            warmup_epochs: 5,
            min_lr: 1e-6,
            early_stop_patience: 20,
            checkpoint_every: 10,
            ..Default::default()
        };
        let mut t = trainer::Trainer::with_transformer(trainer_config, transformer);

        // Run training
        eprintln!("Starting training for {} epochs...", args.epochs);
        let result = t.run_training(train_data, val_data);
        eprintln!("Training complete in {:.1}s", result.total_time_secs);
        // ADR-155 §2.1: `best_pck` is RAW-threshold PCK (no torso norm) and
        // `best_oks` uses the fake-Gold area=1.0 proxy — NOT the canonical
        // hip↔hip `pck_canonical` / COCO OKS. Label them distinctly so the
        // printed numbers are never read as claim-grade canonical metrics.
        eprintln!(
            "  Best epoch: {}, pck_raw@0.2: {:.4}, oks_map(area=1.0 proxy): {:.4}",
            result.best_epoch, result.best_pck, result.best_oks
        );

        // Save checkpoint
        if let Some(ref ckpt_dir) = args.checkpoint_dir {
            let _ = std::fs::create_dir_all(ckpt_dir);
            let ckpt_path = ckpt_dir.join("best_checkpoint.json");
            let ckpt = t.checkpoint();
            match ckpt.save_to_file(&ckpt_path) {
                Ok(()) => eprintln!("Checkpoint saved to {}", ckpt_path.display()),
                Err(e) => eprintln!("Failed to save checkpoint: {e}"),
            }
        }

        // Sync weights back to transformer and save as RVF
        t.sync_transformer_weights();
        if let Some(ref save_path) = args.save_rvf {
            eprintln!("Saving trained model to RVF: {}", save_path.display());
            let weights = t.params().to_vec();
            let mut builder = RvfBuilder::new();
            builder.add_manifest(
                "wifi-densepose-trained",
                env!("CARGO_PKG_VERSION"),
                "WiFi DensePose trained model weights",
            );
            builder.add_metadata(&serde_json::json!({
                "training": {
                    "epochs": args.epochs,
                    "best_epoch": result.best_epoch,
                    "best_pck": result.best_pck,
                    "best_oks": result.best_oks,
                    "n_train_samples": train_data.len(),
                    "n_val_samples": val_data.len(),
                    "n_subcarriers": n_subcarriers,
                    "param_count": weights.len(),
                },
            }));
            builder.add_vital_config(&VitalSignConfig::default());
            builder.add_weights(&weights);
            match builder.write_to_file(save_path) {
                Ok(()) => eprintln!(
                    "RVF saved ({} params, {} bytes)",
                    weights.len(),
                    weights.len() * 4
                ),
                Err(e) => eprintln!("Failed to save RVF: {e}"),
            }
        }

        return;
    }

    info!("WiFi-DensePose Sensing Server (Rust + Axum + RuVector)");
    info!("  HTTP:      http://localhost:{}", args.http_port);
    info!("  WebSocket: ws://localhost:{}/ws/sensing", args.ws_port);
    info!("  UDP:       0.0.0.0:{} (ESP32 CSI)", args.udp_port);
    info!("  UI path:   {}", args.ui_path.display());
    info!("  Source:    {}", args.source);

    // Resolve the data source into a concrete task plan (issue #1004).
    //
    // Issue #937/#1004: `auto` must not serve fake CSI while waiting for the
    // firmware/server startup race to settle. It always binds the UDP receiver,
    // reports `esp32:offline` until the first real frame, and then promotes
    // `source` → "esp32". Explicit `--source simulated` remains a hard,
    // UDP-free override for offline demos.
    let normalized = if args.source == "simulate" {
        "simulated"
    } else {
        args.source.as_str()
    };
    let plan = if normalized == "auto" {
        info!(
            "Auto-detecting data source (UDP :{} bound either way)...",
            args.udp_port
        );
        let esp32 = probe_esp32(args.udp_port).await;
        let wifi = if esp32 {
            false
        } else {
            probe_windows_wifi().await
        };
        if esp32 {
            info!("  ESP32 CSI detected on UDP :{}", args.udp_port);
        } else if wifi {
            info!("  Windows WiFi detected");
        } else {
            warn!(
                "No real CSI source at boot — staying offline while the UDP :{} receiver \
                 remains bound. The server promotes to live on the first real frame; \
                 pass --source simulated explicitly for synthetic demo data.",
                args.udp_port
            );
        }
        plan_source("auto", esp32, wifi)
    } else {
        plan_source(normalized, false, false)
    };
    let source: &str = plan.initial_source.as_str();

    info!(
        "Data source: {source} (udp_receiver={}, simulator={}, wifi={})",
        plan.bind_udp, plan.run_simulator, plan.run_wifi
    );

    // Shared state
    // Vital sign sample rate derives from tick interval (e.g. 500ms tick => 2 Hz)
    let vital_sample_rate = 1000.0 / args.tick_ms as f64;
    info!("Vital sign detector sample rate: {vital_sample_rate:.1} Hz");

    // Load RVF container if --load-rvf was specified
    let rvf_info = if let Some(ref rvf_path) = args.load_rvf {
        info!("Loading RVF container from {}", rvf_path.display());
        match RvfReader::from_file(rvf_path) {
            Ok(reader) => {
                let info = reader.info();
                info!(
                    "  RVF loaded: {} segments, {} bytes",
                    info.segment_count, info.total_size
                );
                if let Some(ref manifest) = info.manifest {
                    if let Some(model_id) = manifest.get("model_id") {
                        info!("  Model ID: {model_id}");
                    }
                    if let Some(version) = manifest.get("version") {
                        info!("  Version:  {version}");
                    }
                }
                if info.has_weights {
                    if let Some(w) = reader.weights() {
                        info!("  Weights: {} parameters", w.len());
                    }
                }
                if info.has_vital_config {
                    info!("  Vital sign config: present");
                }
                if info.has_quant_info {
                    info!("  Quantization info: present");
                }
                if info.has_witness {
                    info!("  Witness/proof: present");
                }
                Some(info)
            }
            Err(e) => {
                error!("Failed to load RVF container: {e}");
                None
            }
        }
    } else {
        None
    };

    // Load trained model via --model (uses progressive loading if --progressive set)
    let model_path = args.model.as_ref().or(args.load_rvf.as_ref());
    let mut progressive_loader: Option<ProgressiveLoader> = None;
    let mut model_loaded = false;
    if let Some(mp) = model_path {
        if args.progressive || args.model.is_some() {
            info!("Loading trained model (progressive) from {}", mp.display());
            match std::fs::read(mp) {
                Ok(data) => match load_or_convert_model(mp, &data) {
                    Ok(mut loader) => {
                        let mut accepted = true;
                        if let Ok(la) = loader.load_layer_a() {
                            info!(
                                "  Layer A ready: model={} v{} ({} segments)",
                                la.model_name, la.version, la.n_segments
                            );
                            if la.manifest.get("task").and_then(|value| value.as_str())
                                == Some("torso")
                            {
                                accepted = match loader.load_layer_c() {
                                    Ok(layer_c) => match torso::validate_live_torso_manifest(
                                        &la.manifest,
                                        &layer_c.all_weights,
                                    ) {
                                        Ok(_) => {
                                            info!("  Torso-v1 manifest and weights accepted for live inference");
                                            true
                                        }
                                        Err(error) => {
                                            error!("Torso model rejected fail-closed: {error}");
                                            false
                                        }
                                    },
                                    Err(error) => {
                                        error!("Torso model weights could not be loaded: {error}");
                                        false
                                    }
                                };
                            }
                        } else {
                            accepted = false;
                            error!(
                                "Model manifest could not be loaded; inference remains disabled"
                            );
                        }
                        if accepted {
                            model_loaded = true;
                            progressive_loader = Some(loader);
                        }
                    }
                    Err(e) => {
                        // #894: typed, actionable message (never the opaque magic)
                        // and a LOUD warning that we are degrading to heuristics.
                        error!("{e}");
                        error!(
                            "Model NOT loaded — falling back to signal heuristics. \
                             Pose/person-count output will be approximate (issue #894)."
                        );
                    }
                },
                Err(e) => error!("Failed to read model file: {e}"),
            }
        }
    }

    // Ensure data directories exist for models and recordings
    let models_dir = model_routes::effective_models_dir();
    let _ = std::fs::create_dir_all(&models_dir);
    let _ = std::fs::create_dir_all("data/recordings");

    // Discover model and recording files on startup
    let initial_models = model_routes::scan_model_files();
    let initial_recordings = recording_routes::scan_recording_files();
    info!(
        "Discovered {} model files, {} recording files",
        initial_models.len(),
        initial_recordings.len()
    );

    // ADR-044 §5.3: load persisted runtime config from the data directory.
    let runtime_config = load_runtime_config(&data_dir);
    info!(
        "Loaded runtime config: dedup_factor={:.2}",
        runtime_config.dedup_factor
    );

    let experiment_store = match experiment::ExperimentStore::open(&data_dir).await {
        Ok(store) => {
            info!(
                "Observatory experiment catalogue ready at {}",
                store.db_path().display()
            );
            Some(Arc::new(store))
        }
        Err(error) => {
            warn!(
                "Observatory experiment catalogue unavailable: {error}; live sensing remains available"
            );
            None
        }
    };

    // Restore the newest calibration only when the current profile still has
    // the same baseline-relevant context. A label or P01-P09 change keeps the
    // context and can reuse the bundle; a moved TX/RX, changed room metadata,
    // or changed sealed setup leaves the server uncalibrated.
    let restored_calibration = if let (Some(store), Some(setup)) =
        (experiment_store.as_ref(), position_setup.as_deref())
    {
        match store
            .latest_calibration_for_setup(setup.setup_id(), setup.setup_sha256())
            .await
        {
            Ok(Some(bundle)) => match store.get_profile(&bundle.profile_id).await {
                Ok(Some(profile)) => {
                    match calibration_persistence::profile_context_sha256(&profile.document) {
                        Ok(context) if context == bundle.profile_context_sha256 => {
                            info!(
                                "Restored persisted D5/D6 calibration {} for setup {}",
                                bundle.calibration_id,
                                setup.setup_id()
                            );
                            Some(Arc::new(bundle))
                        }
                        Ok(_) => {
                            info!(
                            "Persisted D5/D6 calibration is stale for the current setup profile; new empty-room calibration required"
                        );
                            None
                        }
                        Err(error) => {
                            warn!("Could not derive current profile calibration context: {error}");
                            None
                        }
                    }
                }
                Ok(None) => {
                    warn!("Persisted D5/D6 calibration references a missing profile; ignoring it");
                    None
                }
                Err(error) => {
                    warn!("Could not read persisted calibration profile: {error}");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                warn!("Could not restore persisted D5/D6 calibration: {error}");
                None
            }
        }
    } else {
        None
    };
    let restored_calibration_context = restored_calibration.as_ref().map(|bundle| bundle.context());
    let mut initial_d5_presence = d5_presence::PresenceFusionState::default();
    if restored_calibration.is_some() {
        initial_d5_presence.restore_ready(std::time::Instant::now());
    }

    // ADR-102: optional Edge Module Registry. None when --no-edge-registry
    // is set (or when the URL is empty); otherwise we construct one with
    // the configured TTL. The fetch happens lazily on first request.
    let edge_registry: Option<
        std::sync::Arc<wifi_densepose_sensing_server::edge_registry::EdgeRegistry>,
    > = if args.no_edge_registry || args.edge_registry_url.is_empty() {
        info!("Edge module registry: DISABLED (--no-edge-registry or empty URL)");
        None
    } else {
        info!(
            "Edge module registry: enabled — upstream={} ttl={}s",
            args.edge_registry_url, args.edge_registry_ttl_secs
        );
        Some(std::sync::Arc::new(
            wifi_densepose_sensing_server::edge_registry::EdgeRegistry::new(
                args.edge_registry_url.clone(),
                std::time::Duration::from_secs(args.edge_registry_ttl_secs),
            ),
        ))
    };

    let (tx, _) = broadcast::channel::<String>(256);
    let (raw_csi_tx, _) = broadcast::channel::<RawCsiIngress>(2048);
    // ADR-099: parallel broadcast for the per-frame introspection snapshot stream
    // consumed by `/ws/introspection`. Same ring size as `tx` (256) — slow
    // clients drop oldest, identical backpressure shape.
    let (intro_tx, _) = broadcast::channel::<String>(256);

    // #872: actually start the MQTT publisher when `--mqtt` is set. The publisher
    // (mqtt::) consumes a typed VitalsSnapshot stream; we bridge the existing JSON
    // sensing broadcast into it with a defensive serde_json::Value mapping (absent
    // fields default — never publish wrong values). Gated on the `mqtt` feature
    // (the Docker image is built `--features mqtt`); without it `--mqtt` WARNs and
    // no-ops, matching the documented contract.
    if args.mqtt_opts.mqtt {
        #[cfg(feature = "mqtt")]
        {
            use wifi_densepose_sensing_server::mqtt;
            let mcfg = std::sync::Arc::new(mqtt::config::MqttConfig::from_args(&args.mqtt_opts));
            match mcfg.validate() {
                Ok(()) => {
                    let node_id = mcfg.client_id.clone();
                    let builder = mqtt::publisher::OwnedDiscoveryBuilder {
                        discovery_prefix: mcfg.discovery_prefix.clone(),
                        node_id: node_id.clone(),
                        node_friendly_name: Some("RuView".to_string()),
                        sw_version: env!("CARGO_PKG_VERSION").to_string(),
                        model: "RuView WiFi Sensing".to_string(),
                        via_device: None,
                    };
                    let (vtx, vrx) = broadcast::channel::<mqtt::state::VitalsSnapshot>(64);
                    let (host, port) = (mcfg.host.clone(), mcfg.port);
                    mqtt::publisher::spawn(mcfg, builder, vrx);
                    let mut jrx = tx.subscribe();
                    tokio::spawn(async move {
                        while let Ok(json) = jrx.recv().await {
                            let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
                                continue;
                            };
                            // #898/#872: emit one snapshot per physical node so
                            // each surfaces as its own Home-Assistant device with
                            // its *own* presence/motion/RSSI (see
                            // vitals_snapshots_from_sensing_json). Falls back to a
                            // single aggregate snapshot for per-node-less sources.
                            for snap in vitals_snapshots_from_sensing_json(&v, &node_id) {
                                let _ = vtx.send(snap);
                            }
                        }
                    });
                    tracing::info!("MQTT publisher started -> {host}:{port}");
                }
                Err(e) => tracing::error!("MQTT config invalid: {e}; publisher not started"),
            }
        }
        #[cfg(not(feature = "mqtt"))]
        tracing::warn!(
            "--mqtt set but this binary was built without the `mqtt` feature; the publisher is a \
             no-op. Use the official Docker image (built `--features mqtt`) or rebuild with \
             `cargo build -p wifi-densepose-sensing-server --features mqtt`."
        );
    }

    // ADR-262 P3: build the live RuField surface with a provisioned ed25519
    // signer. A missing or malformed seed is a startup error: live events must
    // never be signed with a publicly recoverable fallback key.
    let field_surface: rufield_surface::FieldState = match rufield_surface::FieldSurface::from_env()
    {
        Ok(surface) => Arc::new(RwLock::new(surface)),
        Err(error) => {
            tracing::error!("RuField surface configuration rejected: {error}");
            std::process::exit(78);
        }
    };

    let mmwave_url_configured = args.mmwave_node_url.is_some();
    let mmwave_token = mmwave_connection::load_token(&args.mmwave_token_env);
    let mmwave_token_configured = mmwave_token.is_some();
    let mmwave_control = args.mmwave_node_url.as_ref().and_then(|base_url| {
        match mmwave_token.clone() {
            Some(bearer_token) => {
                Some(mmwave_calibration::NodeControl {
                    base_url: base_url.clone(),
                    bearer_token,
                })
            }
            _ => {
                warn!(
                    "mmWave node URL is configured, but {} is empty; mode and transform writes are disabled",
                    args.mmwave_token_env
                );
                None
            }
        }
    });

    let mut mmwave_manager = mmwave_calibration::MmwaveManager::new(
        args.mmwave_udp_port,
        runtime_position_geometry.room_dimensions,
        mmwave_control,
        position_setup
            .as_deref()
            .and_then(position_setup::SealedPositionSetup::mmwave)
            .map(|definition| {
                let (origin_x_mm, origin_z_mm, yaw_mdeg, raw_x_inverted) = definition.transform();
                mmwave_calibration::ExpectedNode {
                    node_id: definition.node_id().to_string(),
                    mounting_position_m: Some(definition.mounting_position_m()),
                    transform: mmwave_calibration::CoordinateFrame {
                        local: "x_right_y_forward_mm".to_string(),
                        room: "x_length_z_width_mm".to_string(),
                        origin_x_mm,
                        origin_z_mm,
                        yaw_mdeg,
                        raw_x_inverted,
                    },
                }
            }),
        position_setup.as_deref().and_then(|setup| {
            setup.mmwave()?;
            let receiver_positions_m = setup.receiver_positions_m();
            let calibration_receiver_positions_m = setup.receiver_calibration_positions_m();
            let calibration_rx_positions_m = calibration_receiver_positions_m
                .iter()
                .any(|position| position.is_some())
                .then(|| {
                    calibration_receiver_positions_m
                        .into_iter()
                        .zip(receiver_positions_m)
                        .map(|(calibration, device)| calibration.unwrap_or(device))
                        .collect()
                });
            Some(mmwave_calibration::ExperimentContext {
                setup_id: setup.setup_id().to_string(),
                setup_sha256: setup.setup_sha256().to_string(),
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                geometry: position_capture::PositionCaptureGeometry {
                    room_dimensions_m: setup.room_dimensions_m(),
                    tx_position_m: setup.transmitter_position_m(),
                    rx_positions_m: receiver_positions_m.to_vec(),
                },
                calibration_tx_position_m: setup.transmitter_calibration_position_m(),
                calibration_rx_positions_m,
            })
        }),
    );
    mmwave_manager.set_node_control_configuration(mmwave_url_configured, mmwave_token_configured);
    if position_setup.is_none() {
        if let Some(store) = &experiment_store {
            match store.list_profiles().await {
                Ok(profiles) => {
                    if let Some(profile) = profiles.first() {
                        mmwave_manager.apply_cad_profile(profile);
                    }
                }
                Err(error) => warn!("Could not restore CAD radar geometry: {error}"),
            }
        }
    }
    if let Err(error) = mmwave_manager.restore_yaw_calibration(&data_dir) {
        warn!("Could not restore mmWave yaw calibration: {error}");
    }
    if let Err(error) = mmwave_manager.restore_session_manifests(&data_dir) {
        warn!("Could not restore mmWave session manifests: {error}");
    }

    let shutdown = Arc::new(Notify::new());
    let state: SharedState = Arc::new(RwLock::new(AppStateInner {
        latest_update: None,
        rssi_history: VecDeque::new(),
        frame_history: VecDeque::new(),
        tick: 0,
        source: source.into(),
        tx_position: runtime_position_geometry.tx_position,
        room_dimensions: runtime_position_geometry.room_dimensions,
        position_setup: position_setup.clone(),
        csi_grid_pin: args.csi_grid_pin,
        mmwave: mmwave_manager,
        mmwave_node_diagnostics: MmwaveNodeDiagnosticsCache::default(),
        mmwave_connection: mmwave_connection::ConnectionStatus::default(),
        live_position_tracker: position_live::LivePositionTracker::new(live_position_runtime),
        last_esp32_frame: None,
        last_raw_csi_frame: None,
        tx,
        raw_csi_tx,
        intro: wifi_densepose_sensing_server::introspection::IntrospectionState::new(),
        intro_tx,
        total_detections: 0,
        start_time: std::time::Instant::now(),
        vital_detector: VitalSignDetector::new(vital_sample_rate),
        latest_vitals: VitalSigns::default(),
        rvf_info,
        save_rvf_path: args.save_rvf.clone(),
        progressive_loader,
        active_sona_profile: None,
        model_loaded,
        smoothed_person_score: 0.0,
        prev_person_count: 0,
        smoothed_motion: 0.0,
        current_motion_level: "absent".to_string(),
        debounce_counter: 0,
        debounce_candidate: "absent".to_string(),
        baseline_motion: 0.0,
        baseline_frames: 0,
        smoothed_hr: 0.0,
        smoothed_br: 0.0,
        smoothed_hr_conf: 0.0,
        smoothed_br_conf: 0.0,
        hr_buffer: VecDeque::with_capacity(8),
        br_buffer: VecDeque::with_capacity(8),
        edge_vitals: None,
        latest_wasm_events: None,
        // Model management
        discovered_models: initial_models,
        active_model_id: None,
        // Recording
        recordings: initial_recordings,
        recording_lifecycle: Arc::new(Mutex::new(())),
        recording_phase: RecordingLifecyclePhase::Idle,
        recording_active: false,
        recording_start_time: None,
        recording_current_id: None,
        recording_stop_tx: None,
        recording_done_rx: None,
        adaptive_model:
            adaptive_classifier::AdaptiveModel::load(&adaptive_classifier::model_path())
                .ok()
                .inspect(|m| {
                    if adaptive_model_is_trusted(m.training_accuracy) {
                        info!(
                            "Loaded adaptive classifier: {} frames, {:.1}% accuracy",
                            m.trained_frames,
                            m.training_accuracy * 100.0
                        );
                    } else {
                        warn!(
                        "Ignoring adaptive classifier with only {:.1}% accuracy (minimum {:.0}%)",
                        m.training_accuracy * 100.0,
                        MIN_ADAPTIVE_MODEL_ACCURACY * 100.0
                    );
                    }
                })
                .filter(|m| adaptive_model_is_trusted(m.training_accuracy)),
        node_states: HashMap::new(),
        d5_presence: initial_d5_presence,
        active_calibration_bundle: restored_calibration,
        active_calibration_source: Some("restored".to_string())
            .filter(|_| restored_calibration_context.is_some()),
        calibration_context: restored_calibration_context,
        // Accuracy sprint
        pose_tracker: PoseTracker::new(),
        last_tracker_instant: None,
        multistatic_fuser: {
            // #1031/#1049: the default guard (60 ms hard / 20 ms soft)
            // accommodates a real TDM slot offset. A deployment overrides it via
            // WDP_GUARD_INTERVAL_US (direct, e.g. 200000 for WiFi/ESP-NOW sync —
            // #1049) or WDP_TDM_SLOTS + WDP_TDM_SLOT_US (derive from schedule).
            let cfg = multistatic_guard_config_from_env();
            info!(
                "Multistatic fusion guard: {} µs hard / {} µs soft (override via \
                 WDP_GUARD_INTERVAL_US / WDP_SOFT_GUARD_US, or WDP_TDM_SLOTS+WDP_TDM_SLOT_US)",
                cfg.guard_interval_us, cfg.soft_guard_us
            );
            let mut fuser = MultistaticFuser::with_config(MultistaticConfig {
                min_nodes: 1, // single-node passthrough
                ..cfg
            });
            if let Some(positions) = runtime_position_geometry.node_positions.clone() {
                info!(
                    "Configured {} node positions for multistatic fusion",
                    positions.len()
                );
                fuser.set_node_positions(positions);
            }
            fuser
        },
        engine_bridge: engine_bridge::EngineBridge::new(
            wifi_densepose_bfld::PrivacyMode::PrivateHome,
            1,
            "default",
            "Default Room",
        ),
        field_model: if args.calibrate {
            info!("Field model calibration enabled — room should be empty during startup");
            FieldModel::new(field_bridge::single_link_config()).ok()
        } else {
            None
        },
        // ADR-044 §5.2: rolling-P95 over ~30 s at 20 Hz; warm-up after 60 samples.
        p95_variance: RollingP95::new(600, 60),
        p95_motion_band_power: RollingP95::new(600, 60),
        p95_spectral_power: RollingP95::new(600, 60),
        // ADR-044 §5.3: runtime-configurable dedup factor (persisted).
        dedup_factor: runtime_config.dedup_factor,
        data_dir: data_dir.clone(),
        experiment_store,
        field_surface: field_surface.clone(),
    }));

    // Start background tasks from the resolved plan (issue #1004).
    //
    // In `auto` mode with no boot source, the UDP receiver is bound and the
    // source stays explicitly offline until a real frame promotes it. Only
    // explicit `--source simulated` starts the synthetic data task.
    if plan.bind_udp {
        tokio::spawn(udp_receiver_task(state.clone(), args.udp_port));
        tokio::spawn(broadcast_tick_task(state.clone(), args.tick_ms));
    }
    if plan.run_wifi {
        tokio::spawn(windows_wifi_task(state.clone(), args.tick_ms));
    }
    if plan.run_simulator {
        tokio::spawn(simulated_data_task(state.clone(), args.tick_ms));
    }
    tokio::spawn(mmwave_receiver_task(
        state.clone(),
        args.mmwave_udp_port,
        args.mmwave_receive_buffer_bytes,
        args.mmwave_reorder_hold_ms,
    ));
    mmwave_discovery::spawn_listener(
        mmwave_discovery::DEFAULT_DISCOVERY_PORT,
        args.mmwave_udp_port,
        mmwave_token.clone(),
    );
    spawn_mmwave_node_diagnostics_poller(state.clone(), args.mmwave_node_url.clone(), mmwave_token);
    spawn_mmwave_session_ticker(state.clone());

    // ADR-166: Parse bind address once, use for all listeners
    let bind_ip: std::net::IpAddr = args
        .bind_addr
        .parse()
        .expect("Invalid --bind-addr (use 127.0.0.1 or 0.0.0.0)");

    // #443: bearer-token auth on `/api/v1/*`. A token is optional only for a
    // loopback-bound server; routable binds refuse to start without one.
    // When configured, every `/api/v1/*` request must carry
    // `Authorization: Bearer <token>`.
    let bearer_auth_state = wifi_densepose_sensing_server::bearer_auth::AuthState::from_env();
    if bearer_auth_state.is_enabled() {
        info!("API auth: bearer-token enforcement ON for /api/v1/* and live WebSockets (RUVIEW_API_TOKEN set)");
        if bind_ip.is_unspecified() {
            warn!(
                "API auth ON but bind-addr is {} — consider --bind-addr 127.0.0.1 for LAN-only deployments",
                bind_ip
            );
        }
    } else {
        if !bind_ip.is_loopback() {
            tracing::error!(
                "Refusing non-loopback bind on {bind_ip} without RUVIEW_API_TOKEN; \
                 set a strong bearer token before exposing the sensing API"
            );
            std::process::exit(64);
        }
        info!(
            "API auth: loopback-only mode — set RUVIEW_API_TOKEN=<token> before using a \
             routable --bind-addr"
        );
    }

    // DNS-rebinding defense: validate the `Host` header against an allowlist
    // before any handler runs. Default is loopback-only (`localhost`,
    // `127.0.0.1`, `[::1]`, each with or without a port). Operators extend
    // the set via `--allowed-host` flags or the `SENSING_ALLOWED_HOSTS` env
    // var; `--disable-host-validation` opts out entirely for reverse-proxy
    // setups that already canonicalise `Host`.
    let host_allowlist = if args.disable_host_validation {
        warn!(
            "Host-header validation DISABLED — server is reachable via any Host. \
             Only use this behind a reverse proxy that pins Host."
        );
        wifi_densepose_sensing_server::host_validation::HostAllowlist::disabled()
    } else {
        let allowlist =
            wifi_densepose_sensing_server::host_validation::HostAllowlist::from_cli_and_env(
                args.allowed_hosts.iter().cloned(),
            );
        info!(
            "Host-header validation ON ({} entries; loopback names always included)",
            allowlist.entries_for_test().len()
        );
        allowlist
    };

    let browser_origin_allowlist =
        wifi_densepose_sensing_server::host_validation::BrowserOriginAllowlist::from_cli_and_env(
            args.allowed_origins.iter().cloned(),
            args.http_port,
        )
        .unwrap_or_else(|error| {
            tracing::error!("Invalid browser Origin configuration: {error}");
            std::process::exit(64);
        });
    let browser_origin_count = browser_origin_allowlist.entries_for_test().len();
    if browser_origin_count == 0 {
        warn!(
            "Browser Origin validation has no allowed origins because --http-port is 0; \
             configure --allowed-origin or {} before using browser clients",
            wifi_densepose_sensing_server::host_validation::ALLOWED_ORIGINS_ENV
        );
    } else {
        info!(
            "Browser Origin validation ON ({} exact origin(s))",
            browser_origin_count
        );
    }

    // WebSocket server on dedicated port (8765)
    let ws_state = state.clone();
    let ws_app = Router::new()
        .merge(sensing_routes::dedicated_websocket_routes())
        .route("/health", get(health))
        .with_state(ws_state)
        // ADR-262 P3: additive `/ws/field` (+ `/api/field`) on the WS port too,
        // so a client on :8765 can stream signed RuField FieldEvents alongside
        // `/ws/sensing`. Merged with its own FieldState (different state type).
        .merge(rufield_surface::router(field_surface.clone()))
        // Browser WebSocket handshakes carry an Origin but are not protected
        // by CORS; reject foreign origins before the handler can upgrade.
        .layer(axum::middleware::from_fn_with_state(
            browser_origin_allowlist.clone(),
            wifi_densepose_sensing_server::host_validation::require_safe_browser_origin,
        ))
        .layer(axum::middleware::from_fn_with_state(
            bearer_auth_state.clone(),
            wifi_densepose_sensing_server::bearer_auth::require_bearer,
        ))
        // Browser WebSocket handshakes carry an Origin but are not protected
        // by CORS; reject foreign origins before the handler can upgrade.
        .layer(axum::middleware::from_fn_with_state(
            browser_origin_allowlist.clone(),
            wifi_densepose_sensing_server::host_validation::require_safe_browser_origin,
        ))
        .layer(axum::middleware::from_fn_with_state(
            host_allowlist.clone(),
            wifi_densepose_sensing_server::host_validation::require_allowed_host,
        ));

    let ws_addr = SocketAddr::from((bind_ip, args.ws_port));
    let ws_listener = tokio::net::TcpListener::bind(ws_addr)
        .await
        .expect("Failed to bind WebSocket port");
    info!("WebSocket server listening on {ws_addr}");

    tokio::spawn(async move {
        axum::serve(ws_listener, ws_app).await.unwrap();
    });

    // HTTP server (serves UI + full DensePose-compatible REST API)
    let ui_path = args.ui_path.clone();
    let http_app = Router::new()
        .route("/", get(info_page))
        // Health endpoints (DensePose-compatible)
        .route("/health", get(health))
        .route("/health/health", get(health_system))
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/health/version", get(health_version))
        .route("/health/metrics", get(health_metrics))
        // Read-only API observability routes.
        .merge(routes::api_routes())
        // Static UI files
        .nest_service("/ui", ServeDir::new(&ui_path))
        // ADR-102: make the edge registry handle (Option<Arc<EdgeRegistry>>)
        // available to the /api/v1/edge/registry handler. None when disabled.
        .layer(Extension(edge_registry.clone()))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        .with_state(state.clone())
        // ADR-262 P3: additive RuField surface (`/api/field` + `/ws/field`).
        // Merged AFTER `.with_state` (so http_app is already `Router<()>` and
        // can absorb the field router's own `FieldState`).
        .merge(rufield_surface::router(field_surface.clone()))
        // Bearer-token auth on `/api/v1/*`, `/api/field`, and every live
        // WebSocket (#443). Apply it after the RuField merge so the additive
        // routes share the same reader-authorization boundary. It is optional
        // only for a loopback-bound server; `/health*` and `/ui/*` stay public.
        .layer(axum::middleware::from_fn_with_state(
            bearer_auth_state.clone(),
            wifi_densepose_sensing_server::bearer_auth::require_bearer,
        ))
        // Reject foreign browser Origins on state-changing `/api/v1/*`
        // requests and every live WebSocket route. The Host layer below still
        // provides the DNS-rebinding defense.
        .layer(axum::middleware::from_fn_with_state(
            browser_origin_allowlist.clone(),
            wifi_densepose_sensing_server::host_validation::require_safe_browser_origin,
        ))
        // DNS-rebinding defense: applied last so it runs first on the request
        // path (axum layers run outermost-in). Rejects requests whose `Host`
        // header is not in the allowlist before any handler — including
        // `/health`, `/ws/*`, and the merged `/api/field` + `/ws/field` —
        // observes the body.
        .layer(axum::middleware::from_fn_with_state(
            host_allowlist.clone(),
            wifi_densepose_sensing_server::host_validation::require_allowed_host,
        ));

    let http_addr = SocketAddr::from((bind_ip, args.http_port));
    let http_listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .expect("Failed to bind HTTP port");
    info!("HTTP server listening on {http_addr}");
    info!(
        "Open http://localhost:{}/ui/index.html in your browser",
        args.http_port
    );

    // Run the HTTP server with graceful shutdown support
    let shutdown_state = state.clone();
    let shutdown_signal = shutdown.clone();
    let server = axum::serve(http_listener, http_app).with_graceful_shutdown(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to install SIGTERM handler");
            tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    result.expect("failed to install CTRL+C handler");
                    info!("Shutdown signal received");
                }
                _ = terminate.recv() => info!("SIGTERM received; shutting down"),
                _ = shutdown_signal.notified() => info!("Browser shutdown requested"),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    result.expect("failed to install CTRL+C handler");
                    info!("Shutdown signal received");
                }
                _ = shutdown_signal.notified() => info!("Browser shutdown requested"),
            }
        }
    });

    server.await.unwrap();

    match settle_recording_on_shutdown(&shutdown_state).await {
        Ok(Some((recording_id, _, result))) => info!(
                "Recording {recording_id} finalized during shutdown: {} frames, {} dropped, incomplete={}",
                result.frames_written,
                result.dropped_frames,
                result.incomplete()
            ),
        Ok(None) => {}
        Err(error) => {
            error!("Failed to settle recording during shutdown: {error}");
        }
    }

    // Save RVF container on shutdown if --save-rvf was specified
    let s = shutdown_state.read().await;
    if let Some(ref save_path) = s.save_rvf_path {
        info!("Saving RVF container to {}", save_path.display());
        let mut builder = RvfBuilder::new();
        builder.add_manifest(
            "wifi-densepose-sensing",
            env!("CARGO_PKG_VERSION"),
            "WiFi DensePose sensing model state",
        );
        builder.add_metadata(&serde_json::json!({
            "source": s.effective_source(),
            "total_ticks": s.tick,
            "total_detections": s.total_detections,
            "uptime_secs": s.start_time.elapsed().as_secs(),
        }));
        builder.add_vital_config(&VitalSignConfig::default());
        // Save transformer weights if a model is loaded, otherwise empty
        let weights: Vec<f32> = if s.model_loaded {
            // If we loaded via --model, the progressive loader has the weights
            // For now, save runtime state placeholder
            let tf = graph_transformer::CsiToPoseTransformer::new(Default::default());
            tf.flatten_weights()
        } else {
            Vec::new()
        };
        builder.add_weights(&weights);
        match builder.write_to_file(save_path) {
            Ok(()) => info!("  RVF saved ({} weight params)", weights.len()),
            Err(e) => error!("  Failed to save RVF: {e}"),
        }
    }

    info!("Server shut down cleanly");
}

#[cfg(test)]
mod multistatic_guard_config_tests {
    //! #1049 — the multistatic guard interval must be operator-configurable so a
    //! WiFi/ESP-NOW deployment (10–150 ms inter-node clock drift) can lift the
    //! guard past its measured timestamp spread instead of being permanently
    //! demoted to Restricted with no escape hatch.
    use super::*;

    #[test]
    fn default_guard_when_nothing_set() {
        let cfg = multistatic_guard_config_from(None, None, None, None);
        assert_eq!(
            cfg.guard_interval_us,
            MultistaticConfig::default().guard_interval_us
        );
        assert_eq!(
            cfg.soft_guard_us,
            MultistaticConfig::default().soft_guard_us
        );
    }

    #[test]
    fn direct_guard_override_wins_and_unblocks_wifi_spread() {
        // The #1049 reporter's measured ~70 ms spread exceeds the 60 ms default
        // → permanent demotion. A direct 200 ms override accepts it.
        let cfg = multistatic_guard_config_from(None, None, Some("200000"), None);
        assert_eq!(cfg.guard_interval_us, 200_000);
        assert!(cfg.soft_guard_us < cfg.guard_interval_us);
        // 70 ms spread now sits inside the guard.
        assert!(70_000 < cfg.guard_interval_us);
    }

    #[test]
    fn direct_guard_override_beats_tdm_derived() {
        // Both TDM params AND a direct override set → the direct hard guard wins,
        // the TDM-derived soft band is preserved (still strictly below hard).
        let cfg = multistatic_guard_config_from(Some("2"), Some("18000"), Some("200000"), None);
        assert_eq!(cfg.guard_interval_us, 200_000);
        assert!(cfg.soft_guard_us < cfg.guard_interval_us);
        assert!(cfg.soft_guard_us >= 1);
    }

    #[test]
    fn soft_override_is_clamped_strictly_below_hard() {
        // A soft guard ≥ hard would be nonsensical → clamped below the hard guard.
        let cfg = multistatic_guard_config_from(None, None, Some("50000"), Some("999999"));
        assert_eq!(cfg.guard_interval_us, 50_000);
        assert!(cfg.soft_guard_us < 50_000);
    }

    #[test]
    fn lowering_hard_below_default_soft_pulls_soft_down() {
        // Override hard to 10 ms (< default 20 ms soft) → soft drops below it.
        let cfg = multistatic_guard_config_from(None, None, Some("10000"), None);
        assert_eq!(cfg.guard_interval_us, 10_000);
        assert!(cfg.soft_guard_us < 10_000);
    }

    #[test]
    fn malformed_or_zero_override_falls_back_to_base() {
        // Garbage / zero must not break fusion — fall back to the base config.
        for bad in ["", "abc", "0", "-5", "12.5"] {
            let cfg = multistatic_guard_config_from(None, None, Some(bad), None);
            assert_eq!(
                cfg.guard_interval_us,
                MultistaticConfig::default().guard_interval_us,
                "override {bad:?} should be ignored"
            );
        }
    }
}

#[cfg(test)]
mod node_sync_snapshot_serialization_tests {
    //! ADR-110 iter 24 — JSON public-API contract for the iter 23
    //! NodeSyncSnapshot field. Any future rename / removal here must be
    //! intentional and update both Rust + UI/automation consumers.

    use super::*;

    fn sample_sync() -> NodeSyncSnapshot {
        NodeSyncSnapshot {
            offset_us: 1_163_565,
            is_leader: false,
            is_valid: true,
            smoothed: true,
            sequence: 20,
            csi_fps_ema: 10.0,
            csi_fps_samples: 47,
            staleness_ms: Some(120),
        }
    }

    fn sample_node(sync: Option<NodeSyncSnapshot>) -> NodeInfo {
        NodeInfo {
            node_id: 9,
            rssi_dbm: -38.0,
            position: [2.0, 0.0, 1.5],
            amplitude: vec![],
            subcarrier_count: 0,
            sync,
        }
    }

    #[test]
    fn sync_present_serializes_all_seven_fields() {
        let v = serde_json::to_value(sample_node(Some(sample_sync()))).unwrap();
        let s = v.get("sync").expect("sync key must be present");
        // All eight contract fields named exactly as iter 23/34 documented.
        for key in [
            "offset_us",
            "is_leader",
            "is_valid",
            "smoothed",
            "sequence",
            "csi_fps_ema",
            "csi_fps_samples",
            "staleness_ms",
        ] {
            assert!(
                s.get(key).is_some(),
                "sync object missing field `{}` — UI contract broken",
                key
            );
        }
        // Spot-check values round-trip.
        assert_eq!(s["offset_us"], 1_163_565);
        assert_eq!(s["is_leader"], false);
        assert_eq!(s["sequence"], 20);
        assert_eq!(s["csi_fps_samples"], 47);
    }

    #[test]
    fn sync_absent_omits_the_key_entirely() {
        // skip_serializing_if = "Option::is_none" must drop the key, not
        // emit `"sync": null`. The non-mesh paths rely on this for
        // backwards compatibility with pre-iter-23 UI clients.
        let v = serde_json::to_value(sample_node(None)).unwrap();
        assert!(
            v.get("sync").is_none(),
            "expected `sync` key omitted when None, got {:?}",
            v.get("sync")
        );
        // The base NodeInfo fields are still there.
        assert_eq!(v["node_id"], 9);
        assert_eq!(v["rssi_dbm"], -38.0);
    }

    #[test]
    fn sync_round_trips_through_serde() {
        let original = sample_node(Some(sample_sync()));
        let json = serde_json::to_string(&original).unwrap();
        let parsed: NodeInfo = serde_json::from_str(&json).unwrap();
        // Field-level equality on the sync sub-object.
        let s_orig = original.sync.unwrap();
        let s_parsed = parsed.sync.expect("sync should survive round-trip");
        assert_eq!(s_parsed.offset_us, s_orig.offset_us);
        assert_eq!(s_parsed.is_leader, s_orig.is_leader);
        assert_eq!(s_parsed.is_valid, s_orig.is_valid);
        assert_eq!(s_parsed.smoothed, s_orig.smoothed);
        assert_eq!(s_parsed.sequence, s_orig.sequence);
        assert!((s_parsed.csi_fps_ema - s_orig.csi_fps_ema).abs() < 1e-9);
        assert_eq!(s_parsed.csi_fps_samples, s_orig.csi_fps_samples);
    }
}

#[cfg(test)]
mod sync_snapshot_helper_tests {
    //! ADR-110 iter 30 — covers the pure helper that backs both
    //! `/api/v1/nodes/:id/sync` and `/api/v1/mesh` REST endpoints and
    //! the WebSocket sensing_update broadcast. Tests at this layer keep
    //! the public-API contract honest without spinning up the axum
    //! router or constructing a full AppStateInner.

    use super::*;
    use wifi_densepose_hardware::{SyncPacket, SyncPacketFlags};

    fn populated_sync(node_id: u8) -> SyncPacket {
        SyncPacket {
            node_id,
            proto_ver: 1,
            flags: SyncPacketFlags {
                is_leader: false,
                is_valid: true,
                smoothed_used: true,
            },
            local_us: 28_798_450,
            epoch_us: 27_634_885,
            sequence: 20,
        }
    }

    #[test]
    fn fresh_node_with_no_sync_returns_none() {
        // Mirrors the REST 404 "no_sync" branch.
        let ns = NodeState::new();
        assert!(ns.sync_snapshot().is_none());
    }

    #[test]
    fn node_with_latest_sync_produces_correct_snapshot() {
        // Mirrors the REST 200 OK branch + the WebSocket sync field.
        let mut ns = NodeState::new();
        ns.latest_sync = Some(populated_sync(9));
        ns.latest_sync_at = Some(std::time::Instant::now());
        // Pretend the fps EMA has settled (iter 18 5-sample warmup).
        ns.csi_fps_ema = 10.5;
        ns.csi_fps_samples = 42;

        let snap = ns
            .sync_snapshot()
            .expect("populated state must produce a snapshot");
        assert_eq!(snap.offset_us, 1_163_565); // §A0.10 measured boot delta
        assert!(!snap.is_leader);
        assert!(snap.is_valid);
        assert!(snap.smoothed);
        assert_eq!(snap.sequence, 20);
        assert!((snap.csi_fps_ema - 10.5).abs() < 1e-9);
        assert_eq!(snap.csi_fps_samples, 42);
    }

    #[test]
    fn apply_sync_packet_populates_a_fresh_node() {
        // Mirrors what udp_receiver_task does on the very first sync
        // packet from a previously-unseen node.
        let mut ns = NodeState::new();
        assert!(ns.latest_sync.is_none());
        assert!(ns.latest_sync_at.is_none());

        let now = std::time::Instant::now();
        ns.apply_sync_packet(populated_sync(9), now);

        let sync = ns.latest_sync.as_ref().expect("must be populated");
        assert_eq!(sync.node_id, 9);
        assert_eq!(sync.sequence, 20);
        // latest_sync_at must be exactly the Instant we passed (no clock skew).
        assert_eq!(ns.latest_sync_at, Some(now));
        // sync_snapshot now produces a value (REST 200 OK path).
        assert!(ns.sync_snapshot().is_some());
    }

    #[test]
    fn accepted_csi_frame_records_mesh_timestamp_for_fusion() {
        let mut ns = NodeState::new();
        let now = std::time::Instant::now() - std::time::Duration::from_secs(4);
        let sync = populated_sync(9);
        ns.apply_sync_packet(sync, now);
        let mut second = populated_sync(9);
        second.sequence = 60;
        second.local_us += 2_000_000;
        second.epoch_us += 2_000_000;
        ns.apply_sync_packet(second.clone(), now + std::time::Duration::from_secs(2));
        let expected = second.mesh_aligned_us_for_sequence(62, 20.0);

        let frame_time = now + std::time::Duration::from_millis(2_050);
        ns.observe_accepted_csi_frame(62, frame_time);

        assert_eq!(ns.last_frame_time, Some(frame_time));
        assert_eq!(ns.latest_frame_mesh_time_us, Some(expected));
    }

    #[test]
    fn apply_sync_packet_overwrites_older_data() {
        // Subsequent packets must replace, not accumulate. Otherwise the
        // §A0.10-smoothed offset would lag the latest beacon.
        let mut ns = NodeState::new();
        let t0 = std::time::Instant::now();
        ns.apply_sync_packet(populated_sync(9), t0);

        // Second packet: same node, advanced sequence + offset.
        let mut second = populated_sync(9);
        second.sequence = 40;
        second.local_us = 30_000_000;
        second.epoch_us = 28_834_900;
        let t1 = t0 + std::time::Duration::from_secs(2);
        ns.apply_sync_packet(second, t1);

        let cur = ns.latest_sync.as_ref().unwrap();
        assert_eq!(cur.sequence, 40); // newer sequence persisted
        assert_eq!(cur.local_us, 30_000_000); // newer local persisted
        assert_eq!(ns.latest_sync_at, Some(t1)); // staleness clock reset
    }

    #[test]
    fn sync_intervals_estimate_sequence_rate_despite_lost_csi_packets() {
        let mut ns = NodeState::new();
        let t0 = std::time::Instant::now() - std::time::Duration::from_secs(4);
        let mut first = populated_sync(9);
        first.sequence = 100;
        first.local_us = 1_000_000;
        first.epoch_us = 1_000_000;
        ns.apply_sync_packet(first, t0);

        let mut second = populated_sync(9);
        second.sequence = 140;
        second.local_us = 3_000_000;
        second.epoch_us = 3_000_000;
        ns.apply_sync_packet(second, t0 + std::time::Duration::from_secs(2));

        assert_eq!(ns.csi_fps_samples, 1);
        assert!((ns.csi_fps_ema - 20.0).abs() < 0.01);
    }

    #[test]
    fn sync_fps_estimation_handles_sequence_wrap() {
        let mut ns = NodeState::new();
        let t0 = std::time::Instant::now() - std::time::Duration::from_secs(4);
        let mut first = populated_sync(9);
        first.sequence = u32::MAX - 9;
        first.local_us = 1_000_000;
        first.epoch_us = 1_000_000;
        ns.apply_sync_packet(first, t0);

        let mut second = populated_sync(9);
        second.sequence = 10;
        second.local_us = 3_000_000;
        second.epoch_us = 3_000_000;
        ns.apply_sync_packet(second, t0 + std::time::Duration::from_secs(2));

        assert_eq!(ns.csi_fps_samples, 1);
        assert!((ns.csi_fps_ema - 10.0).abs() < 0.01);
    }

    #[test]
    fn invalid_sync_is_never_used_for_mesh_time() {
        let mut ns = NodeState::new();
        let mut sync = populated_sync(9);
        sync.flags.is_valid = false;
        ns.apply_sync_packet(sync, std::time::Instant::now());

        assert!(ns.mesh_aligned_us_for_csi_frame(20).is_none());
    }

    #[test]
    fn frame_older_than_sync_high_water_fails_closed() {
        let mut ns = NodeState::new();
        let t0 = std::time::Instant::now() - std::time::Duration::from_secs(4);
        let mut first = populated_sync(9);
        first.sequence = 100;
        first.local_us = 1_000_000;
        first.epoch_us = 1_000_000;
        ns.apply_sync_packet(first, t0);
        let mut second = populated_sync(9);
        second.sequence = 120;
        second.local_us = 3_000_000;
        second.epoch_us = 3_000_000;
        ns.apply_sync_packet(second, t0 + std::time::Duration::from_secs(2));

        assert!(ns.mesh_aligned_us_for_csi_frame(119).is_none());
    }

    #[test]
    fn implausible_forward_sequence_jump_fails_closed() {
        let mut ns = NodeState::new();
        let t0 = std::time::Instant::now() - std::time::Duration::from_secs(4);
        let mut first = populated_sync(9);
        first.sequence = 100;
        first.local_us = 1_000_000;
        first.epoch_us = 1_000_000;
        ns.apply_sync_packet(first, t0);
        let mut second = populated_sync(9);
        second.sequence = 120;
        second.local_us = 3_000_000;
        second.epoch_us = 3_000_000;
        ns.apply_sync_packet(second, t0 + std::time::Duration::from_secs(2));

        assert!(ns.mesh_aligned_us_for_csi_frame(10_000).is_none());
    }

    #[test]
    fn node_reboot_resets_rate_and_requires_a_fresh_sync_interval() {
        let mut ns = NodeState::new();
        let t0 = std::time::Instant::now() - std::time::Duration::from_secs(6);
        let mut first = populated_sync(9);
        first.sequence = 100;
        first.local_us = 10_000_000;
        first.epoch_us = 10_000_000;
        ns.apply_sync_packet(first, t0);
        let mut second = populated_sync(9);
        second.sequence = 140;
        second.local_us = 12_000_000;
        second.epoch_us = 12_000_000;
        ns.apply_sync_packet(second, t0 + std::time::Duration::from_secs(2));
        assert_eq!(ns.csi_fps_samples, 1);
        ns.fusion_frame_history.push_back(FusionFrameSample {
            amplitude: vec![1.0; 56],
            host_monotonic_us: 10_000,
            mesh_timestamp_us: Some(10_000),
        });

        let mut rebooted = populated_sync(9);
        rebooted.sequence = 3;
        rebooted.local_us = 200_000;
        rebooted.epoch_us = 200_000;
        ns.apply_sync_packet(rebooted, t0 + std::time::Duration::from_secs(4));

        assert_eq!(ns.csi_fps_samples, 0);
        assert!(
            ns.fusion_frame_history.is_empty(),
            "a reboot must not bridge pre-reboot frames into the next quartet"
        );
        assert!(ns.mesh_aligned_us_for_csi_frame(4).is_none());
    }

    #[test]
    fn regressing_csi_sequence_is_excluded_from_fusion_queue() {
        let mut ns = NodeState::new();
        ns.latest_sequence = Some(100);
        ns.fusion_frame_history.push_back(FusionFrameSample {
            amplitude: vec![1.0; 56],
            host_monotonic_us: 10_000,
            mesh_timestamp_us: Some(10_000),
        });

        let now = std::time::Instant::now();
        ns.observe_accepted_csi_frame(90, now);
        assert!(ns.fusion_frame_history.is_empty());

        ns.observe_live_fusion_frame(&[1.0; 56], 11_000_000);
        assert!(
            ns.fusion_frame_history.is_empty(),
            "the regressing frame itself must remain out of fusion"
        );
    }

    #[test]
    fn snapshot_staleness_ms_tracks_apply_time() {
        // Iter 34: staleness_ms = (Instant::now() - latest_sync_at).as_millis().
        // We can't pass a synthetic "now" through sync_snapshot, but we can
        // pin latest_sync_at to a past instant and assert the value lands
        // in a plausible window.
        let mut ns = NodeState::new();
        ns.latest_sync = Some(populated_sync(9));
        ns.latest_sync_at =
            std::time::Instant::now().checked_sub(std::time::Duration::from_millis(750));

        let snap = ns.sync_snapshot().unwrap();
        let st = snap.staleness_ms.expect("staleness_ms must be present");
        // Should be approximately 750 ms — give a generous ±500 ms tolerance
        // for any test-runner scheduling delay between checked_sub() and
        // elapsed() within sync_snapshot.
        assert!(
            st >= 740 && st < 1250,
            "expected ~750 ms staleness, got {} ms",
            st
        );
    }

    #[test]
    fn fleet_role_counts_classifies_correctly() {
        // Iter 37 — verify the leader/follower split that drives the
        // Prometheus `wifi_densepose_mesh_node_total{state=...}` gauge.
        // Local fixture rather than reaching across test modules.
        fn snap(is_leader: bool) -> NodeSyncSnapshot {
            NodeSyncSnapshot {
                offset_us: 0,
                is_leader,
                is_valid: true,
                smoothed: true,
                sequence: 0,
                csi_fps_ema: 10.0,
                csi_fps_samples: 10,
                staleness_ms: Some(0),
            }
        }
        assert_eq!(super::fleet_role_counts(&[]), (0, 0));
        let snaps = vec![(12u8, snap(true)), (9, snap(false)), (3, snap(false))];
        assert_eq!(super::fleet_role_counts(&snaps), (1, 2));
        // Edge: all leaders (election would prevent this but gauge math must hold).
        assert_eq!(
            super::fleet_role_counts(&[(1u8, snap(true)), (2, snap(true))]),
            (2, 0)
        );
    }

    #[test]
    fn bool_metric_returns_zero_or_one_as_text() {
        // Locks the Prometheus exposition convention: gauges holding a
        // boolean state MUST emit literal "0" or "1", never "false"/"true".
        // If anyone changes the helper to format!("{}", b), Prometheus will
        // 400-reject the scrape — catch it here instead of in production.
        assert_eq!(super::bool_metric(true), "1");
        assert_eq!(super::bool_metric(false), "0");
    }

    #[test]
    fn mesh_aligned_us_honors_9s_staleness_gate() {
        // The receive helper stores latest_sync_at = Instant::now() each
        // beacon. mesh_aligned_us_for_csi_frame returns None once that
        // Instant is older than 9 s (3 × VALID_WINDOW_MS). Verify both
        // sides of that boundary without sleeping — set latest_sync_at
        // to past instants directly.
        let mut ns = NodeState::new();
        let now = std::time::Instant::now();
        ns.latest_sync = Some(populated_sync(9));
        ns.csi_fps_ema = 20.0;
        ns.csi_fps_samples = 1;

        // Fresh: 1 s old → should return Some.
        ns.latest_sync_at = now.checked_sub(std::time::Duration::from_secs(1));
        assert!(
            ns.mesh_aligned_us_for_csi_frame(20).is_some(),
            "1 s old sync must produce a mesh-aligned timestamp"
        );

        // Just inside the gate: 8 s old → should still return Some.
        ns.latest_sync_at = now.checked_sub(std::time::Duration::from_secs(8));
        assert!(
            ns.mesh_aligned_us_for_csi_frame(20).is_some(),
            "8 s old sync must still be inside the 9 s gate"
        );

        // Just outside the gate: 10 s old → must return None.
        ns.latest_sync_at = now.checked_sub(std::time::Duration::from_secs(10));
        assert!(
            ns.mesh_aligned_us_for_csi_frame(20).is_none(),
            "10 s old sync must trigger the 9 s staleness gate"
        );
    }

    #[test]
    fn snapshot_reflects_leader_state() {
        // Same data shape that /api/v1/mesh emits for a leader node.
        let mut ns = NodeState::new();
        let mut s = populated_sync(12);
        s.flags = SyncPacketFlags {
            is_leader: true,
            is_valid: true,
            smoothed_used: false,
        };
        s.local_us = 28_864_932;
        s.epoch_us = 28_864_939; // -7 µs delta on the leader
        ns.latest_sync = Some(s);
        ns.latest_sync_at = Some(std::time::Instant::now());

        let snap = ns.sync_snapshot().unwrap();
        assert!(snap.is_leader);
        assert_eq!(snap.offset_us, -7); // call-stack µs only
        assert!(!snap.smoothed);
    }
}

#[cfg(test)]
mod novelty_tests {
    use super::*;

    /// First call to `update_novelty` must produce *some* score
    /// (`Some(_)` not `None`) — proves the per-node sketch bank is
    /// initialised by `NodeState::new()` and the novelty path is
    /// actually being exercised. With an empty bank the score is 1.0
    /// (max novelty).
    #[test]
    fn first_frame_yields_max_novelty_then_zero_on_repeat() {
        let mut ns = NodeState::new();
        let amplitudes: Vec<f64> = (0..NOVELTY_VECTOR_DIM).map(|i| (i as f64).sin()).collect();

        ns.update_novelty(&amplitudes);
        let first = ns.last_novelty_score.expect("sketch bank initialised");
        assert!(
            (first - 1.0).abs() < 1e-6,
            "empty bank → max novelty 1.0, got {first}"
        );

        // Repeat the exact same frame — bank now contains it, so the
        // novelty score must be 0.0 (the score is computed before the
        // second insert, against the post-first-insert bank).
        ns.update_novelty(&amplitudes);
        let second = ns.last_novelty_score.expect("score stays Some");
        assert_eq!(second, 0.0, "exact-repeat frame → novelty 0.0");
    }

    /// `update_novelty` must tolerate amplitude vectors of unexpected
    /// length — short ones zero-padded, long ones truncated — without
    /// panicking. ESP32-S3 boards report 56 subcarriers but other
    /// hardware variants ship 52 or 64; the schema-locked sketch bank
    /// requires exactly NOVELTY_VECTOR_DIM.
    #[test]
    fn handles_short_and_long_amplitude_vectors() {
        let mut ns = NodeState::new();
        ns.update_novelty(&[1.0, 2.0]); // way short
        assert!(ns.last_novelty_score.is_some());

        let too_long: Vec<f64> = (0..NOVELTY_VECTOR_DIM * 2).map(|i| i as f64).collect();
        ns.update_novelty(&too_long); // way long
        assert!(ns.last_novelty_score.is_some());
    }
}

// ── Unit tests: RollingP95 ─────────────────────────────────────────────────────

#[cfg(test)]
mod rolling_p95_tests {
    use super::RollingP95;

    #[test]
    fn cold_start_returns_none() {
        let p = RollingP95::new(100, 10);
        assert!(p.current().is_none(), "empty buffer must return None");
    }

    #[test]
    fn below_min_samples_returns_none() {
        let mut p = RollingP95::new(100, 10);
        for i in 1..=9 {
            p.push(i as f64);
        }
        assert!(
            p.current().is_none(),
            "fewer than min_samples must return None"
        );
    }

    #[test]
    fn p95_of_ramp_is_near_95() {
        let mut p = RollingP95::new(100, 10);
        for i in 1..=100 {
            p.push(i as f64);
        }
        let p95 = p.current().expect("should have value after 100 samples");
        assert!(
            (94.0..=96.0).contains(&p95),
            "P95 of 1..=100 should be ~95, got {p95}"
        );
    }

    #[test]
    fn window_slides_evicts_oldest() {
        let mut p = RollingP95::new(5, 3);
        // Push 1..=5, then 100 — oldest (1) is evicted.
        for i in 1..=5 {
            p.push(i as f64);
        }
        p.push(100.0); // evicts 1; buf = [2, 3, 4, 5, 100]
        let p95 = p.current().expect("6 pushes, window=5 → 5 samples");
        // P95 of [2,3,4,5,100]: idx = ceil(5*0.95)=5 → sorted[4]=100
        assert_eq!(
            p95, 100.0,
            "largest value should dominate p95 after eviction"
        );
    }

    #[test]
    fn len_reports_buffer_size() {
        let mut p = RollingP95::new(10, 5);
        assert_eq!(p.len(), 0);
        p.push(1.0);
        assert_eq!(p.len(), 1);
    }
}

#[cfg(all(test, feature = "mqtt"))]
mod mqtt_bridge_tests {
    use super::vitals_snapshots_from_sensing_json;
    use serde_json::json;

    /// Regression for the per-node presence bug (#872/#898): each node must
    /// surface its OWN classification, not the room-level aggregate. Node 1 is
    /// present+moving; node 2 is absent — node 2 must NOT inherit node 1's
    /// "present".
    #[test]
    fn per_node_presence_uses_each_nodes_own_classification() {
        let v = json!({
            "timestamp": 1.0,
            "classification": { "presence": true, "motion_level": "walking", "confidence": 0.9 },
            "vital_signs": { "breathing_rate_bpm": 14.0, "heart_rate_bpm": 60.0 },
            "persons": [{}, {}],
            "nodes": [
                { "node_id": 1, "rssi_dbm": -40.0,
                  "classification": { "presence": true, "motion_level": "walking", "confidence": 0.8 } },
                { "node_id": 2, "rssi_dbm": -70.0,
                  "classification": { "presence": false, "motion_level": "absent", "confidence": 0.1 } }
            ]
        });
        let snaps = vitals_snapshots_from_sensing_json(&v, "ruview");
        assert_eq!(snaps.len(), 2, "one snapshot per node");

        let n1 = snaps.iter().find(|s| s.node_id == "ruview-node1").unwrap();
        let n2 = snaps.iter().find(|s| s.node_id == "ruview-node2").unwrap();

        assert!(n1.presence && n1.motion > 0.0, "node1 present + moving");
        assert!(
            !n2.presence && n2.motion == 0.0,
            "node2 must be absent — not inherit the room aggregate"
        );
        // Per-node RSSI preserved.
        assert_eq!(n1.rssi_dbm, Some(-40.0));
        assert_eq!(n2.rssi_dbm, Some(-70.0));
        // Vitals + person count are room-level, shared across node devices.
        assert_eq!(n1.n_persons, 2);
        assert_eq!(n2.n_persons, 2);
        assert_eq!(n1.breathing_rate_bpm, Some(14.0));
        assert_eq!(n2.heartrate_bpm, Some(60.0));
        // presence_score is gated on presence.
        assert!(n1.presence_score > 0.0);
        assert_eq!(n2.presence_score, 0.0);
    }

    /// A node that omits a classification field defers to the room aggregate
    /// rather than silently reading false/0.
    #[test]
    fn per_node_missing_fields_fall_back_to_aggregate() {
        let v = json!({
            "timestamp": 1.0,
            "classification": { "presence": true, "motion_level": "still", "confidence": 0.7 },
            "vital_signs": {},
            "nodes": [ { "node_id": 3, "rssi_dbm": -55.0 } ]  // no per-node classification
        });
        let snaps = vitals_snapshots_from_sensing_json(&v, "n");
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].node_id, "n-node3");
        assert!(snaps[0].presence, "defers to aggregate presence");
        assert_eq!(snaps[0].motion, 0.0, "aggregate 'still' => no motion");
    }

    /// No `nodes` array (wifi / simulate sources): single aggregate snapshot
    /// keyed by the base id.
    #[test]
    fn falls_back_to_single_aggregate_when_no_nodes() {
        let v = json!({
            "timestamp": 2.0,
            "classification": { "presence": true, "motion_level": "idle", "confidence": 0.6 },
            "vital_signs": { "breathing_rate_bpm": 12.0 },
            "persons": [{}]
        });
        let snaps = vitals_snapshots_from_sensing_json(&v, "ruview");
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].node_id, "ruview");
        assert!(snaps[0].presence);
        assert_eq!(snaps[0].motion, 0.0, "idle => no motion");
        assert_eq!(snaps[0].n_persons, 1);
    }

    /// `motion_level: "absent"` must map to zero motion (the old aggregate
    /// match fell through to `Some(_) => 1.0`, treating absent as full motion).
    #[test]
    fn absent_motion_level_is_zero_motion() {
        let v = json!({
            "timestamp": 0.0,
            "classification": { "presence": false, "motion_level": "absent", "confidence": 0.0 },
            "vital_signs": {}
        });
        let snaps = vitals_snapshots_from_sensing_json(&v, "x");
        assert_eq!(snaps[0].motion, 0.0);
        assert!(!snaps[0].presence);
    }
}

#[cfg(test)]
mod model_load_diagnostic_tests {
    use super::diagnose_model_load_error;
    use std::path::Path;

    #[test]
    fn safetensors_is_named_and_points_at_894() {
        // 8-byte LE header length then '{' — the safetensors signature.
        let data = [0x10, 0, 0, 0, 0, 0, 0, 0, b'{', b'"'];
        let msg = diagnose_model_load_error(
            Path::new("models/wifi-densepose-pretrained/model.safetensors"),
            &data,
            "invalid magic at offset 0",
        );
        assert!(msg.contains("safetensors"), "{msg}");
        assert!(msg.contains("#894"), "{msg}");
        assert!(msg.contains("signal heuristics"), "{msg}");
    }

    #[test]
    fn quantized_bin_is_identified() {
        let data = [0x35, 0x57, 0x45, 0x77]; // the 0x77455735 the loader reports
        let msg = diagnose_model_load_error(Path::new("model-q4.bin"), &data, "bad magic");
        assert!(msg.contains("quantized weight blob"), "{msg}");
        assert!(msg.contains("RVFS") || msg.contains("0x52564653"), "{msg}");
    }

    #[test]
    fn jsonl_manifest_is_identified() {
        let data = *b"{\"seg\":0}";
        let msg = diagnose_model_load_error(Path::new("model.rvf.jsonl"), &data, "x");
        assert!(msg.contains("JSONL manifest"), "{msg}");
    }

    #[test]
    fn unknown_format_still_gives_guidance() {
        let data = [0u8, 1, 2, 3];
        let msg = diagnose_model_load_error(Path::new("weird.dat"), &data, "x");
        assert!(msg.contains("RVF binary container"), "{msg}");
        assert!(msg.contains("wifi-densepose-train"), "{msg}");
    }
}

#[cfg(test)]
mod export_rvf_mode_tests {
    use super::export_emits_placeholder_demo;

    #[test]
    fn standalone_export_emits_placeholder() {
        // --export-rvf alone → the container-format demo (placeholder weights).
        assert!(export_emits_placeholder_demo(true, false, false));
    }

    #[test]
    fn export_with_train_does_not_short_circuit() {
        // #894: `--train --export-rvf` must NOT emit a placeholder + skip
        // training — it must fall through to the real training pipeline.
        assert!(!export_emits_placeholder_demo(true, true, false));
        assert!(!export_emits_placeholder_demo(true, false, true));
        assert!(!export_emits_placeholder_demo(true, true, true));
    }

    #[test]
    fn no_export_flag_never_emits() {
        assert!(!export_emits_placeholder_demo(false, false, false));
        assert!(!export_emits_placeholder_demo(false, true, false));
    }
}

#[cfg(test)]
mod observatory_persons_field_position_tests {
    //! Issue #1050 — the Observatory 3D figure animates from per-person
    //! `position` / `motion_score` / `pose` carried on `sensing_update.persons`.
    //!
    //! These tests pin the public WS contract: a frame that detects a person on
    //! a known signal_field peak must emit a `persons` array whose first entry
    //! carries a `position` derived from that peak (matching the Observatory's
    //! cell→world transform), a real `motion_score`, and a serialized frame
    //! that round-trips. An empty / no-presence field must emit `persons: []`
    //! (or no person), never a phantom person at a fabricated origin.

    use super::*;

    /// Build a 20×20 signal_field that is background everywhere except a single
    /// strong normalized peak at grid cell `(ix, iz)`.
    fn field_with_peak(ix: usize, iz: usize) -> SignalField {
        let nx = 20usize;
        let nz = 20usize;
        let mut values = vec![0.05f64; nx * nz];
        values[iz * nx + ix] = 1.0;
        SignalField {
            grid_size: [nx, 1, nz],
            values,
        }
    }

    /// Build an all-background (below-threshold) 20×20 field — no localizable
    /// hotspot, modelling an empty / no-presence room.
    fn empty_field() -> SignalField {
        SignalField {
            grid_size: [20, 1, 20],
            values: vec![0.05f64; 20 * 20],
        }
    }

    fn base_update(
        signal_field: SignalField,
        presence: bool,
        motion_band_power: f64,
    ) -> SensingUpdate {
        SensingUpdate {
            msg_type: "sensing_update".to_string(),
            timestamp: 1.0,
            source: "test".to_string(),
            tick: 1,
            tx_position: None,
            room_dimensions: None,
            nodes: vec![],
            features: FeatureInfo {
                mean_rssi: -60.0,
                variance: 48.6,
                motion_band_power,
                breathing_band_power: 0.0,
                dominant_freq_hz: 1.0,
                change_points: 0,
                spectral_power: 0.0,
            },
            classification: ClassificationInfo {
                motion_level: if presence {
                    "present_moving".to_string()
                } else {
                    "absent".to_string()
                },
                presence,
                confidence: 0.8,
            },
            signal_field,
            localization: None,
            position_estimate: None,
            vital_signs: None,
            enhanced_motion: None,
            enhanced_breathing: None,
            posture: None,
            signal_quality_score: None,
            quality_verdict: None,
            bssid_count: None,
            pose_keypoints: None,
            model_status: None,
            persons: None,
            estimated_persons: Some(1),
            node_features: None,
        }
    }

    #[test]
    fn offline_public_update_clears_stale_detection_and_localization() {
        let mut update = base_update(field_with_peak(15, 4), true, 63.3);
        update.source = "esp32".to_string();
        update.nodes.push(NodeInfo {
            node_id: 1,
            rssi_dbm: -48.0,
            position: [1.0, 0.5, 2.0],
            amplitude: vec![1.0],
            subcarrier_count: 1,
            sync: None,
        });
        update.localization = Some(coarse_localization::CoarseLocalizationEstimate {
            status: coarse_localization::CoarseLocalizationStatus::Coarse,
            position: Some(coarse_localization::FloorPoint { x: 1.5, z: 2.5 }),
            confidence: 0.75,
            uncertainty: None,
            geometry_links: 4,
            calibrated_links: 4,
            usable_links: 4,
            active_links: 3,
            probability_map: None,
        });
        update.persons = Some(derive_pose_from_sensing(&update));
        update.vital_signs = Some(VitalSigns::default());

        let public = public_sensing_update(&update, "esp32:offline");

        assert_eq!(public.source, "esp32:offline");
        assert!(public.nodes.is_empty());
        assert!(!public.classification.presence);
        assert_eq!(public.classification.motion_level, "unknown");
        assert_eq!(public.classification.confidence, 0.0);
        assert!(public.signal_field.values.iter().all(|value| *value == 0.0));
        assert_eq!(
            public.localization.as_ref().map(|estimate| estimate.status),
            Some(coarse_localization::CoarseLocalizationStatus::Unavailable)
        );
        assert!(public
            .localization
            .as_ref()
            .and_then(|estimate| estimate.position)
            .is_none());
        assert_eq!(
            public.position_estimate,
            Some(position_live::LivePositionState::Stale)
        );
        assert!(public.persons.is_none());
        assert!(public.estimated_persons.is_none());
        assert!(public.vital_signs.is_none());

        let wifi_public = public_sensing_update(&update, "wifi:offline");
        assert_eq!(wifi_public.source, "wifi:offline");
        assert!(wifi_public.nodes.is_empty());
        assert!(!wifi_public.classification.presence);
        assert!(wifi_public.persons.is_none());
    }

    #[test]
    fn esp32_position_fail_states_are_explicit_and_coordinate_free() {
        for state in [
            position_live::LivePositionState::Unknown,
            position_live::LivePositionState::Ambiguous,
            position_live::LivePositionState::Insufficient,
            position_live::LivePositionState::Uncalibrated,
            position_live::LivePositionState::Stale,
        ] {
            let mut update = base_update(empty_field(), false, 0.0);
            update.source = "esp32".to_string();
            update.position_estimate = Some(state);
            let encoded = serde_json::to_value(update).unwrap();
            let estimate = encoded
                .get("position_estimate")
                .expect("ESP32 update must expose an explicit position state");
            assert!(estimate.get("state").is_some());
            assert!(estimate.get("point_id").is_none());
            assert!(estimate.get("coordinates_m").is_none());
        }
    }

    #[test]
    fn esp32_position_fail_states_clear_every_public_and_pose_cache() {
        let mut template = base_update(field_with_peak(15, 4), true, 63.3);
        template.source = "esp32".to_string();
        template.room_dimensions = Some([4.02, 2.59, 3.44]);
        template.position_estimate = Some(position_live::LivePositionState::Position {
            point_id: "P05".to_string(),
            coordinates_m: [2.01, 0.0, 1.72],
        });
        template.persons = Some(derive_pose_from_sensing(&template));
        template.estimated_persons = Some(1);
        assert!(template
            .persons
            .as_ref()
            .is_some_and(|persons| !persons.is_empty()));

        for fail_state in [
            position_live::LivePositionState::Unknown,
            position_live::LivePositionState::Ambiguous,
            position_live::LivePositionState::Insufficient,
            position_live::LivePositionState::Uncalibrated,
            position_live::LivePositionState::Stale,
        ] {
            let mut update = template.clone();

            assert!(apply_esp32_position_estimate_contract(
                &mut update,
                fail_state
            ));
            assert!(update.persons.is_none());
            assert!(update.estimated_persons.is_none());
        }

        let mut pose_tracker = PoseTracker::new();
        pose_tracker.create_track(&[[0.0; 3]; 17], 1);
        let mut last_tracker_instant = Some(std::time::Instant::now());
        assert_eq!(pose_tracker.active_count(), 1);

        clear_esp32_pose_cache(&mut pose_tracker, &mut last_tracker_instant);

        assert_eq!(pose_tracker.active_count(), 0);
        assert!(last_tracker_instant.is_none());
    }

    #[test]
    fn accepted_position_keeps_public_person_marker() {
        let mut update = base_update(field_with_peak(15, 4), true, 63.3);
        update.source = "esp32".to_string();
        update.room_dimensions = Some([4.02, 2.59, 3.44]);
        update.persons = Some(vec![]);
        update.estimated_persons = Some(1);

        assert!(!apply_esp32_position_estimate_contract(
            &mut update,
            position_live::LivePositionState::Position {
                point_id: "P05".to_string(),
                coordinates_m: [2.01, 0.0, 1.72],
            },
        ));
        assert!(update.persons.is_some());
        assert_eq!(update.estimated_persons, Some(1));
    }

    #[test]
    fn raw_position_input_expires_at_one_second() {
        let seen = std::time::Instant::now();

        assert!(!position_raw_input_is_stale(
            Some(seen),
            seen + POSITION_RAW_STALE_TIMEOUT - std::time::Duration::from_nanos(1),
        ));
        assert!(position_raw_input_is_stale(
            Some(seen),
            seen + POSITION_RAW_STALE_TIMEOUT,
        ));
        assert!(position_raw_input_is_stale(None, seen));
    }

    #[test]
    fn esp32_coarse_localization_cannot_emit_a_person_without_discrete_position() {
        let mut update = base_update(field_with_peak(15, 4), true, 63.3);
        update.source = "esp32".to_string();
        update.room_dimensions = Some([4.02, 2.59, 3.44]);
        update.localization = Some(coarse_localization::CoarseLocalizationEstimate {
            status: coarse_localization::CoarseLocalizationStatus::Coarse,
            position: Some(coarse_localization::FloorPoint { x: 1.5, z: 2.5 }),
            confidence: 0.99,
            uncertainty: None,
            geometry_links: 4,
            calibrated_links: 4,
            usable_links: 4,
            active_links: 4,
            probability_map: None,
        });
        update.position_estimate = Some(position_live::LivePositionState::Uncalibrated);

        assert!(derive_pose_from_sensing(&update).is_empty());
    }

    #[test]
    fn esp32_person_is_an_exact_discrete_marker_without_synthetic_pose() {
        let mut update = base_update(field_with_peak(15, 4), true, 63.3);
        update.source = "esp32".to_string();
        update.room_dimensions = Some([4.02, 2.59, 3.44]);
        update.position_estimate = Some(position_live::LivePositionState::Position {
            point_id: "P05".to_string(),
            coordinates_m: [2.01, 0.0, 1.72],
        });
        update.persons = Some(derive_pose_from_sensing(&update));
        attach_field_positions(&mut update);

        let persons = update.persons.as_ref().unwrap();
        assert_eq!(persons.len(), 1);
        assert_eq!(persons[0].zone, "P05");
        assert_eq!(persons[0].position, [2.01, 0.0, 1.72]);
        assert!(persons[0].keypoints.is_empty());
        assert!(persons[0].pose.is_none());

        update.classification.presence = false;
        assert!(derive_pose_from_sensing(&update).is_empty());
    }

    #[test]
    fn esp32_invalid_discrete_coordinates_fail_closed() {
        let mut update = base_update(field_with_peak(15, 4), true, 63.3);
        update.source = "esp32".to_string();
        update.room_dimensions = Some([4.02, 2.59, 3.44]);
        update.position_estimate = Some(position_live::LivePositionState::Position {
            point_id: "P05".to_string(),
            coordinates_m: [4.03, 0.0, 1.72],
        });

        assert!(derive_pose_from_sensing(&update).is_empty());
    }

    #[test]
    fn sensing_update_emits_persons_with_field_derived_position() {
        // Person present, motion energy 63.3, a hotspot at cell (15, 4).
        let peak_ix = 15;
        let peak_iz = 4;
        let mut update = base_update(field_with_peak(peak_ix, peak_iz), true, 63.3);

        // Pipeline order: derive raw skeleton, then attach real field positions.
        update.persons = Some(derive_pose_from_sensing(&update));
        attach_field_positions(&mut update);

        let persons = update.persons.as_ref().expect("persons should be Some");
        assert!(!persons.is_empty(), "a present person must be emitted");

        // Position must match the Observatory cell→world transform for (15, 4):
        // x = (15-10)*0.6 = 3.0 ; z = (4-10)*0.5 = -3.0 ; y = 0.
        let p0 = &persons[0];
        assert!((p0.position[0] - 3.0).abs() < 1e-6, "x={}", p0.position[0]);
        assert!((p0.position[1] - 0.0).abs() < 1e-9);
        assert!(
            (p0.position[2] - (-3.0)).abs() < 1e-6,
            "z={}",
            p0.position[2]
        );

        // motion_score is the measured motion_band_power passed through (≤100).
        assert!(
            (p0.motion_score - 63.3).abs() < 1e-6,
            "motion_score={}",
            p0.motion_score
        );

        // The serialized WS frame must carry the new fields by their exact
        // contract names the Observatory UI reads.
        let v = serde_json::to_value(&update).unwrap();
        let arr = v["persons"]
            .as_array()
            .expect("persons must be a JSON array");
        assert_eq!(arr.len(), persons.len());
        let pj = &arr[0];
        assert!(
            pj.get("position").is_some(),
            "person.position missing from WS frame"
        );
        assert!(
            pj.get("motion_score").is_some(),
            "person.motion_score missing from WS frame"
        );
        assert!((pj["position"][0].as_f64().unwrap() - 3.0).abs() < 1e-6);
        assert!((pj["position"][2].as_f64().unwrap() - (-3.0)).abs() < 1e-6);
        assert!((pj["motion_score"].as_f64().unwrap() - 63.3).abs() < 1e-6);
    }

    #[test]
    fn pose_is_real_when_posture_present_and_absent_otherwise() {
        // No aggregate posture estimate → pose is None (never fabricated).
        let mut no_posture = base_update(field_with_peak(10, 10), true, 40.0);
        no_posture.persons = Some(derive_pose_from_sensing(&no_posture));
        attach_field_positions(&mut no_posture);
        let p = &no_posture.persons.as_ref().unwrap()[0];
        assert!(
            p.pose.is_none(),
            "pose must stay None when no real posture exists"
        );
        // skip_serializing_if drops the key entirely (UI defaults to 'standing').
        let v = serde_json::to_value(&no_posture).unwrap();
        assert!(v["persons"][0].get("pose").is_none());

        // Real aggregate posture present → pose is carried through verbatim.
        let mut with_posture = base_update(field_with_peak(10, 10), true, 40.0);
        with_posture.posture = Some("lying".to_string());
        with_posture.persons = Some(derive_pose_from_sensing(&with_posture));
        attach_field_positions(&mut with_posture);
        let p2 = &with_posture.persons.as_ref().unwrap()[0];
        assert_eq!(p2.pose.as_deref(), Some("lying"));
        let v2 = serde_json::to_value(&with_posture).unwrap();
        assert_eq!(v2["persons"][0]["pose"], "lying");
    }

    #[test]
    fn empty_room_yields_no_phantom_person() {
        // No presence → derive_pose_from_sensing returns no persons at all.
        let mut update = base_update(empty_field(), false, 2.0);
        update.persons = Some(derive_pose_from_sensing(&update));
        attach_field_positions(&mut update);

        let persons = update.persons.as_ref().unwrap();
        assert!(
            persons.is_empty(),
            "no-presence frame must not emit a phantom person, got {} persons",
            persons.len()
        );

        // And in the serialized frame the array is empty (no fake origin person).
        let v = serde_json::to_value(&update).unwrap();
        assert_eq!(v["persons"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn present_but_below_threshold_field_keeps_position_at_origin_not_fabricated() {
        // Presence is true but the field has no peak above PEAK_THRESHOLD — we
        // must NOT invent a position; it stays at the [0,0,0] default while
        // motion_score still reflects the real measured motion power. This is
        // the honest degenerate case (no localizable hotspot to report).
        let mut update = base_update(empty_field(), true, 55.0);
        update.persons = Some(derive_pose_from_sensing(&update));
        attach_field_positions(&mut update);

        let p = &update.persons.as_ref().unwrap()[0];
        assert_eq!(
            p.position,
            [0.0, 0.0, 0.0],
            "no peak → default origin, not fabricated coords"
        );
        assert!(
            (p.motion_score - 55.0).abs() < 1e-6,
            "motion_score stays real"
        );
    }
}
