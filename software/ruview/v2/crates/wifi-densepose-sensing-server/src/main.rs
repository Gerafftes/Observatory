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
mod classification_evaluation;
pub mod cli;
mod coarse_localization;
pub mod csi;
mod d5_presence;
mod d6_fingerprint;
mod engine_bridge;
mod experiment;
mod experiment_evaluation;
mod field_bridge;
mod field_localize;
mod mmwave_calibration;
mod mmwave_position_index;
mod model_format;
mod multistatic_bridge;
pub mod pose;
mod position_artifact;
mod position_capture;
mod position_evaluation;
mod position_fingerprint;
mod position_live;
mod position_offline;
mod position_setup;
mod raw_csi_recording;
mod raw_csi_replay;
mod rvf_container;
mod rvf_pipeline;
mod server_clock;
mod torso;
mod tracker_bridge;
pub mod types;
mod vital_signs;

// Training pipeline modules (exposed via lib.rs)
use wifi_densepose_sensing_server::{
    dataset, embedding, error_response, graph_transformer, rufield_surface, trainer,
};

use ruvector_mincut::{DynamicMinCut, MinCutBuilder};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{delete, get, post, put},
    Extension, Router,
};
use clap::{Parser, ValueEnum};

use axum::http::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, Mutex, RwLock};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{debug, error, info, warn};

use rvf_container::{RvfBuilder, RvfContainerInfo, RvfReader, VitalSignConfig};
use rvf_pipeline::ProgressiveLoader;
use vital_signs::{VitalSignDetector, VitalSigns};

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
}

// ── Data types ───────────────────────────────────────────────────────────────

/// ADR-018 ESP32 CSI binary frame header (20 bytes)
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Esp32Frame {
    magic: u32,
    node_id: u8,
    n_antennas: u8,
    /// u16 since ADR-110 / issue #1005: ESP32-C6 HE-SU frames carry 256
    /// subcarrier bins (242 active HE20 tones). HT frames stay ≤128.
    n_subcarriers: u16,
    freq_mhz: u16,
    sequence: u32,
    rssi: i8,
    noise_floor: i8,
    /// ADR-110 byte 18: PPDU type the CSI was sampled from. Pre-ADR-110
    /// firmware sends 0 ⇒ `PpduType::HtLegacy`.
    ppdu_type: wifi_densepose_hardware::PpduType,
    amplitudes: Vec<f64>,
    phases: Vec<f64>,
}

/// CSI fingerprint identity. Equal bin counts alone are not comparable across
/// channels, antenna layouts, or PPDU training fields.
type CsiGridKey = (u16, u8, u16, wifi_densepose_hardware::PpduType);

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
    /// Per-node measured CSI frame rate (iter 18 EMA). 20.0 until the
    /// EMA has at least 5 samples; the actually-observed rate after that.
    csi_fps_ema: f64,
    /// How many CSI frames have contributed to `csi_fps_ema`. Clients can
    /// treat <5 as "not yet trustworthy" and fall back to 20 Hz.
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

struct NodeState {
    pub(crate) frame_history: VecDeque<Vec<f64>>,
    smoothed_person_score: f64,
    pub(crate) prev_person_count: usize,
    smoothed_motion: f64,
    latest_raw_motion: f64,
    motion_confidence: f64,
    current_motion_level: String,
    debounce_counter: u32,
    debounce_candidate: String,
    baseline_motion: f64,
    baseline_frames: u64,
    /// Experimental D5 empty-room reference and rolling still-presence evidence.
    d5_presence: d5_presence::NodePresenceState,
    /// D6 compares the gain-normalized CSI shape with the empty-room
    /// fingerprint. Unlike D5's motion score it remains informative after a
    /// person has stopped moving and also provides the per-link anomaly weight
    /// used by coarse localization.
    d6_fingerprint: d6_fingerprint::NodeFingerprintState,
    /// Frames rejected from the current empty-room calibration because D4
    /// observed movement.
    calibration_motion_rejected_frames: u64,
    smoothed_hr: f64,
    smoothed_br: f64,
    smoothed_hr_conf: f64,
    smoothed_br_conf: f64,
    hr_buffer: VecDeque<f64>,
    br_buffer: VecDeque<f64>,
    rssi_history: VecDeque<f64>,
    vital_detector: VitalSignDetector,
    latest_vitals: VitalSigns,
    pub(crate) last_frame_time: Option<std::time::Instant>,
    /// Most recent semantically valid TX-source trailer. The private identity
    /// is retained only to compare RX1-RX4 during discovery and is never
    /// serialized or logged.
    source_binding_observation: Option<SourceBindingObservation>,
    /// Structurally valid, source-attested CSI frames excluded because they did
    /// not use this receiver's selected fingerprint grid.
    skipped_grid_frames: u64,
    /// Mesh-aligned timestamp for the latest accepted CSI frame, recovered from
    /// ADR-110 sync packets when available. Multistatic fusion uses this instead
    /// of UDP arrival time so host/network jitter does not look like sensor
    /// clock spread.
    pub(crate) latest_frame_mesh_time_us: Option<u64>,
    edge_vitals: Option<Esp32VitalsPacket>,
    /// ADR-110 §A0.12: Latest sync packet received from this node. When a
    /// CSI frame arrives with byte 19 bit 4 set (`adr018_flags.ieee802154_sync_valid`),
    /// the host can recover a mesh-aligned timestamp via
    /// `latest_sync.epoch_us + (now_local - latest_sync.local_us)`.
    latest_sync: Option<wifi_densepose_hardware::SyncPacket>,
    /// Last time a sync packet from this node was received (for staleness).
    latest_sync_at: Option<std::time::Instant>,
    /// ADR-110 iter 18: EMA-tracked CSI frame rate for this node.
    /// Replaces the hardcoded 20 Hz fallback in
    /// `mesh_aligned_us_for_csi_frame` once `csi_fps_samples ≥ 5`.
    csi_fps_ema: f64,
    /// Number of inter-frame deltas observed (need ≥5 before trusting EMA).
    csi_fps_samples: u32,
    /// Latest accepted CSI sequence and diagnostic gap counters.
    latest_sequence: Option<u32>,
    inferred_lost_frames: u64,
    sequence_observations: u64,
    /// Latest extracted features for cross-node fusion.
    latest_features: Option<FeatureInfo>,
    // ── RuVector Phase 2: Temporal smoothing & coherence gating ──
    /// Previous frame's smoothed keypoint positions for EMA temporal smoothing.
    prev_keypoints: Option<Vec<[f64; 3]>>,
    /// Rolling buffer of motion_energy values for coherence scoring (last 20 frames).
    motion_energy_history: VecDeque<f64>,
    /// Coherence score [0.0, 1.0]: low variance in motion_energy = high coherence.
    coherence_score: f64,
    /// ADR-084 Pass 3 cluster-Pi novelty sensor — per-node sketch bank of
    /// recent CSI feature vectors. Populated by `update_novelty` on each
    /// frame; left `None` to disable the sensor on a per-node basis.
    feature_history: Option<wifi_densepose_signal::ruvsense::longitudinal::EmbeddingHistory>,
    /// Most recent novelty score in [0.0, 1.0] (0 = exact-match in bank,
    /// 1 = no overlap). Consumed by the model-wake gate downstream.
    pub(crate) last_novelty_score: Option<f32>,
    /// Full CSI identity used by this node's rolling windows and D6 reference.
    active_grid: Option<CsiGridKey>,
    /// A replacement grid must remain stable for several consecutive frames
    /// before it can replace `active_grid`.
    candidate_grid: Option<CsiGridKey>,
    candidate_grid_hits: u8,
}

/// Default EMA alpha for temporal keypoint smoothing (RuVector Phase 2).
/// Lower = smoother (more history, less jitter). 0.15 balances responsiveness
/// with stability for WiFi CSI where per-frame noise is high.
const TEMPORAL_EMA_ALPHA_DEFAULT: f64 = 0.15;
/// Reduced EMA alpha when coherence is low (trust measurements less).
const TEMPORAL_EMA_ALPHA_LOW_COHERENCE: f64 = 0.05;
/// Coherence threshold below which we reduce EMA alpha.
const COHERENCE_LOW_THRESHOLD: f64 = 0.3;
/// Maximum allowed bone-length change ratio between frames (20%).
const MAX_BONE_CHANGE_RATIO: f64 = 0.20;
/// Number of motion_energy frames to track for coherence scoring.
const COHERENCE_WINDOW: usize = 20;
/// ADR-084 Pass 3 — per-node novelty sketch dimension (56 subcarriers,
/// the dominant ESP32-S3 capture configuration).
const NOVELTY_VECTOR_DIM: usize = 56;
/// ADR-084 Pass 3 — number of past sketches retained per-node for
/// novelty comparison. 64 frames ≈ 6.4 s at 10 Hz.
const NOVELTY_HISTORY_CAPACITY: usize = 64;
/// ADR-084 Pass 3 — feature-vector schema version. Bump on changes to
/// subcarrier ordering / normalisation so banks reject stale data.
const NOVELTY_SKETCH_VERSION: u16 = 1;
/// Consecutive frames required before switching a node to another CSI grid.
const GRID_SWITCH_CONFIRMATIONS: u8 = 8;

/// ADR-110 iter 18 — EMA update for per-node CSI fps tracking.
///
/// Returns the new EMA value, or `None` if the delta is implausible
/// (≤ 0, or > 1 second — likely a connection gap, not a real frame
/// rate sample). α = 1/8 fixed shift, ~8-sample effective window,
/// matching the firmware-side ESP-NOW offset smoother in §A0.10.
///
/// Free function for testability — every transformation that doesn't
/// touch the rest of `NodeState` lives outside the `impl` block.
pub(crate) fn update_csi_fps_ema(prev_fps: f64, dt_sec: f64) -> Option<f64> {
    if !(dt_sec > 0.0 && dt_sec < 1.0) {
        return None;
    }
    let instantaneous = 1.0 / dt_sec;
    // y[n] = y[n-1] + (x - y[n-1]) / 8
    Some(prev_fps + (instantaneous - prev_fps) / 8.0)
}

#[cfg(test)]
mod fps_ema_tests {
    use super::update_csi_fps_ema;

    #[test]
    fn steady_10hz_converges_toward_10() {
        let mut fps = 20.0;
        for _ in 0..40 {
            fps = update_csi_fps_ema(fps, 0.100).unwrap();
        }
        assert!(
            (fps - 10.0).abs() < 0.1,
            "expected ~10 Hz after 40 samples at 100 ms intervals, got {fps}"
        );
    }

    #[test]
    fn steady_20hz_stays_near_20() {
        let mut fps = 20.0;
        for _ in 0..20 {
            fps = update_csi_fps_ema(fps, 0.050).unwrap();
        }
        assert!((fps - 20.0).abs() < 0.05, "expected ~20 Hz, got {fps}");
    }

    #[test]
    fn nonpositive_dt_rejected() {
        assert!(update_csi_fps_ema(15.0, 0.0).is_none());
        assert!(update_csi_fps_ema(15.0, -0.1).is_none());
    }

    #[test]
    fn long_gap_rejected_as_implausible() {
        assert!(update_csi_fps_ema(20.0, 2.0).is_none());
    }
}

impl NodeState {
    /// ADR-110 §A0.12 timestamp recovery: given a CSI frame's node-local
    /// `esp_timer_get_time()` snapshot, return the mesh-aligned epoch
    /// computed from this node's most recent sync packet — or `None`
    /// if no sync has been received yet, or the last one is too stale
    /// (older than 3 × VALID_WINDOW_MS = 9 s, matching the firmware's own
    /// staleness gate).
    pub(crate) fn mesh_aligned_us(&self, local_at_frame_us: u64) -> Option<u64> {
        let sync = self.latest_sync.as_ref()?;
        let seen_at = self.latest_sync_at?;
        // Drop stale syncs — firmware emits at ~0.5 Hz default, anything
        // older than 9 s likely means the mesh transport dropped.
        if seen_at.elapsed() > std::time::Duration::from_secs(9) {
            return None;
        }
        Some(sync.apply_to_local(local_at_frame_us))
    }

    /// ADR-110 §A0.12 sequence-based mesh-time recovery for an in-flight
    /// ADR-018 CSI frame. The frame carries no `local_us` (the wire
    /// format has no slot), but it carries a sequence number that the
    /// sync packet's `sequence` high-water can be paired against. Uses
    /// 20 Hz as the default CSI rate (the firmware's
    /// `CSI_MIN_SEND_INTERVAL_US`-implied ceiling). Returns `None` if
    /// no fresh sync has been observed for this node.
    pub(crate) fn mesh_aligned_us_for_csi_frame(&self, frame_sequence: u32) -> Option<u64> {
        let sync = self.latest_sync.as_ref()?;
        let seen_at = self.latest_sync_at?;
        if seen_at.elapsed() > std::time::Duration::from_secs(9) {
            return None;
        }
        // Iter 18: use the measured per-node fps once we have ≥5 inter-frame
        // samples; until then fall back to the 20 Hz firmware ceiling. The
        // §A0.12 capture showed real bench fps ≈ 10, so the measured value
        // is significantly more accurate than the constant fallback.
        let fps = if self.csi_fps_samples >= 5 {
            self.csi_fps_ema
        } else {
            20.0
        };
        Some(sync.mesh_aligned_us_for_sequence(frame_sequence, fps))
    }

    /// ADR-110 iter 18 — update the per-node observed-fps EMA from a fresh
    /// CSI frame arrival. Call once per accepted CSI frame from
    /// `udp_receiver_task`. Uses `last_frame_time` as the previous-frame
    /// anchor; the first frame after init seeds the timer without producing
    /// a sample (no prior dt to measure).
    /// ADR-110 iter 32 — apply a freshly-decoded sync packet to this node.
    /// Overwrites `latest_sync` with the new packet and stamps
    /// `latest_sync_at` so the staleness gate in `mesh_aligned_us_for_csi_frame`
    /// can age it out after 9 s. Used by `udp_receiver_task` on every
    /// successful magic-dispatched sync datagram; extracted so the dispatch
    /// path is testable without spinning up the tokio UDP socket.
    pub(crate) fn apply_sync_packet(
        &mut self,
        pkt: wifi_densepose_hardware::SyncPacket,
        now: std::time::Instant,
    ) {
        self.latest_sync = Some(pkt);
        self.latest_sync_at = Some(now);
    }

    /// ADR-110 iter 30 — pure snapshot of this node's mesh-sync state.
    /// Returns `None` when no sync packet has been observed. Used by both
    /// the WebSocket broadcaster (iter 23) and the REST handlers (iter 29);
    /// extracted here so tests can build a `NodeState`, populate
    /// `latest_sync`, and assert the snapshot shape without spinning up
    /// the axum router.
    pub(crate) fn sync_snapshot(&self) -> Option<NodeSyncSnapshot> {
        let sync = self.latest_sync.as_ref()?;
        Some(NodeSyncSnapshot {
            offset_us: sync.local_minus_epoch_us(),
            is_leader: sync.flags.is_leader,
            is_valid: sync.flags.is_valid,
            smoothed: sync.flags.smoothed_used,
            sequence: sync.sequence,
            csi_fps_ema: self.csi_fps_ema,
            csi_fps_samples: self.csi_fps_samples,
            staleness_ms: self.latest_sync_at.map(|t| t.elapsed().as_millis() as u64),
        })
    }

    pub(crate) fn observe_csi_frame_arrival(&mut self, now: std::time::Instant) {
        if let Some(prev) = self.last_frame_time {
            let dt = now.duration_since(prev).as_secs_f64();
            if let Some(new_ema) = update_csi_fps_ema(self.csi_fps_ema, dt) {
                self.csi_fps_ema = new_ema;
                self.csi_fps_samples = self.csi_fps_samples.saturating_add(1);
            }
        }
        self.last_frame_time = Some(now);
    }

    pub(crate) fn observe_accepted_csi_frame(
        &mut self,
        frame_sequence: u32,
        now: std::time::Instant,
    ) {
        if let Some(previous) = self.latest_sequence {
            let delta = frame_sequence.wrapping_sub(previous);
            if (1..0x8000_0000).contains(&delta) {
                self.inferred_lost_frames = self
                    .inferred_lost_frames
                    .saturating_add(u64::from(delta.saturating_sub(1)));
                self.sequence_observations = self.sequence_observations.saturating_add(1);
            }
        }
        self.latest_sequence = Some(frame_sequence);
        self.observe_csi_frame_arrival(now);
        self.latest_frame_mesh_time_us = self.mesh_aligned_us_for_csi_frame(frame_sequence);
    }

    fn observe_source_binding(&mut self, observation: Option<SourceBindingObservation>) {
        self.source_binding_observation = observation;
    }

    fn invalidate_source_binding_attestation(&mut self) {
        self.source_binding_observation = None;
    }

    pub(crate) fn new() -> Self {
        Self {
            frame_history: VecDeque::new(),
            smoothed_person_score: 0.0,
            prev_person_count: 0,
            smoothed_motion: 0.0,
            latest_raw_motion: 0.0,
            motion_confidence: 0.0,
            current_motion_level: "absent".to_string(),
            debounce_counter: 0,
            debounce_candidate: "absent".to_string(),
            baseline_motion: 0.0,
            baseline_frames: 0,
            d5_presence: d5_presence::NodePresenceState::default(),
            d6_fingerprint: d6_fingerprint::NodeFingerprintState::default(),
            calibration_motion_rejected_frames: 0,
            smoothed_hr: 0.0,
            smoothed_br: 0.0,
            smoothed_hr_conf: 0.0,
            smoothed_br_conf: 0.0,
            hr_buffer: VecDeque::with_capacity(8),
            br_buffer: VecDeque::with_capacity(8),
            rssi_history: VecDeque::new(),
            vital_detector: VitalSignDetector::new(10.0),
            latest_vitals: VitalSigns::default(),
            last_frame_time: None,
            source_binding_observation: None,
            skipped_grid_frames: 0,
            latest_frame_mesh_time_us: None,
            edge_vitals: None,
            latest_sync: None,
            latest_sync_at: None,
            csi_fps_ema: 20.0,
            csi_fps_samples: 0,
            latest_sequence: None,
            inferred_lost_frames: 0,
            sequence_observations: 0,
            latest_features: None,
            prev_keypoints: None,
            motion_energy_history: VecDeque::with_capacity(COHERENCE_WINDOW),
            coherence_score: 1.0, // assume stable initially
            feature_history: Some(
                wifi_densepose_signal::ruvsense::longitudinal::EmbeddingHistory::with_sketch(
                    NOVELTY_VECTOR_DIM,
                    NOVELTY_HISTORY_CAPACITY,
                    NOVELTY_SKETCH_VERSION,
                ),
            ),
            last_novelty_score: None,
            active_grid: None,
            candidate_grid: None,
            candidate_grid_hits: 0,
        }
    }

    /// ADR-110 / issue #1005 grid gate: decide whether a frame on `grid`
    /// may enter this node's feature path, and update `active_grid`.
    ///
    /// Returns `true` to accept. A different grid must be observed for
    /// `GRID_SWITCH_CONFIRMATIONS` consecutive frames before it replaces the
    /// active grid. The rolling history and motion baseline are then cleared,
    /// so symbol grids are never mixed while occasional outliers cannot poison
    /// a stable ESP32-S3 or ESP32-C6 stream. Rejected arrivals still count for
    /// fps/liveness in the caller.
    fn accept_grid(&mut self, grid: CsiGridKey) -> bool {
        match self.active_grid {
            None => {
                self.active_grid = Some(grid);
                self.candidate_grid = None;
                self.candidate_grid_hits = 0;
                true
            }
            Some(active) if active == grid => {
                self.candidate_grid = None;
                self.candidate_grid_hits = 0;
                true
            }
            Some(_) => {
                if self.candidate_grid == Some(grid) {
                    self.candidate_grid_hits = self.candidate_grid_hits.saturating_add(1);
                } else {
                    self.candidate_grid = Some(grid);
                    self.candidate_grid_hits = 1;
                }

                if self.candidate_grid_hits < GRID_SWITCH_CONFIRMATIONS {
                    return false;
                }

                self.active_grid = Some(grid);
                self.candidate_grid = None;
                self.candidate_grid_hits = 0;
                self.frame_history.clear();
                self.smoothed_motion = 0.0;
                self.latest_raw_motion = 0.0;
                self.motion_confidence = 0.0;
                self.current_motion_level = "absent".to_string();
                self.debounce_counter = 0;
                self.debounce_candidate = "absent".to_string();
                self.baseline_motion = 0.0;
                self.baseline_frames = 0;
                self.d5_presence.invalidate_reference();
                self.d6_fingerprint.invalidate_reference();
                true
            }
        }
    }

    /// Preserve fresh source attestation for every complete controlled-TX
    /// frame, while admitting only the selected CSI grid to sensing.
    fn observe_validated_grid(
        &mut self,
        observation: SourceBindingObservation,
        grid: CsiGridKey,
        matches_sealed_grid: bool,
    ) -> bool {
        self.observe_source_binding(Some(observation));
        if matches_sealed_grid && self.accept_grid(grid) {
            return true;
        }
        self.skipped_grid_frames = self.skipped_grid_frames.saturating_add(1);
        false
    }

    /// ADR-084 cluster-Pi novelty step. Truncates / zero-pads the
    /// incoming amplitude vector to `NOVELTY_VECTOR_DIM`, scores its
    /// novelty against the per-node bank, then inserts it. The novelty
    /// score is computed *before* the insert so a frame doesn't see
    /// itself in the bank.
    pub(crate) fn update_novelty(&mut self, amplitudes: &[f64]) {
        let history = match &mut self.feature_history {
            Some(h) => h,
            None => return,
        };
        let mut feature: Vec<f32> = amplitudes
            .iter()
            .take(NOVELTY_VECTOR_DIM)
            .map(|&v| v as f32)
            .collect();
        feature.resize(NOVELTY_VECTOR_DIM, 0.0);

        // Score before insert so a query doesn't see itself.
        self.last_novelty_score = history.novelty(&feature);

        let _ = history.push(
            wifi_densepose_signal::ruvsense::longitudinal::EmbeddingEntry {
                person_id: 0,
                day_us: 0,
                embedding: feature,
            },
        );
    }

    /// Update the coherence score from the latest motion_energy value.
    ///
    /// Coherence is computed as 1.0 / (1.0 + running_variance) so that
    /// low motion-energy variance maps to high coherence ([0, 1]).
    fn update_coherence(&mut self, motion_energy: f64) {
        if self.motion_energy_history.len() >= COHERENCE_WINDOW {
            self.motion_energy_history.pop_front();
        }
        self.motion_energy_history.push_back(motion_energy);

        let n = self.motion_energy_history.len();
        if n < 2 {
            self.coherence_score = 1.0;
            return;
        }

        let mean: f64 = self.motion_energy_history.iter().sum::<f64>() / n as f64;
        let variance: f64 = self
            .motion_energy_history
            .iter()
            .map(|v| (v - mean) * (v - mean))
            .sum::<f64>()
            / (n - 1) as f64;

        // Map variance to [0, 1] coherence: higher variance = lower coherence.
        self.coherence_score = (1.0 / (1.0 + variance)).clamp(0.0, 1.0);
    }

    /// Choose the EMA alpha based on current coherence score.
    fn ema_alpha(&self) -> f64 {
        if self.coherence_score < COHERENCE_LOW_THRESHOLD {
            TEMPORAL_EMA_ALPHA_LOW_COHERENCE
        } else {
            TEMPORAL_EMA_ALPHA_DEFAULT
        }
    }
}

#[cfg(test)]
mod grid_gate_tests {
    use super::*;
    use wifi_densepose_hardware::PpduType;

    const STABLE_GRID: CsiGridKey = (2437, 1, 128, PpduType::HtLegacy);
    const REPLACEMENT_GRID: CsiGridKey = (2437, 1, 192, PpduType::HtLegacy);

    fn complete_binding(now: std::time::Instant) -> SourceBindingObservation {
        SourceBindingObservation::validated(
            &raw_csi_recording::SourceBinding {
                trailer_version: raw_csi_recording::TX_SOURCE_BINDING_VERSION,
                flags: raw_csi_recording::SOURCE_BINDING_REQUIRED_FLAGS,
                scheme: raw_csi_recording::TX_SOURCE_BINDING_SCHEME.to_string(),
                tx_filter_sha256: "a".repeat(64),
            },
            now,
            false,
        )
        .unwrap()
    }

    fn source_binding(flags: u8, digest_character: char) -> raw_csi_recording::SourceBinding {
        raw_csi_recording::SourceBinding {
            trailer_version: raw_csi_recording::TX_SOURCE_BINDING_VERSION,
            flags,
            scheme: raw_csi_recording::TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: digest_character.to_string().repeat(64),
        }
    }

    #[test]
    fn accepts_the_active_grid_continuously() {
        let mut state = NodeState::new();

        for _ in 0..20 {
            assert!(state.accept_grid(STABLE_GRID));
        }
        assert_eq!(state.active_grid, Some(STABLE_GRID));
        assert_eq!(state.candidate_grid, None);
    }

    #[test]
    fn rejects_a_single_grid_outlier_without_poisoning_the_stream() {
        let mut state = NodeState::new();
        assert!(state.accept_grid(STABLE_GRID));

        assert!(!state.accept_grid(REPLACEMENT_GRID));
        assert_eq!(state.active_grid, Some(STABLE_GRID));
        assert!(state.accept_grid(STABLE_GRID));
        assert_eq!(state.candidate_grid, None);
        assert_eq!(state.candidate_grid_hits, 0);
    }

    #[test]
    fn valid_binding_stays_attested_when_an_off_grid_frame_is_filtered() {
        let now = std::time::Instant::now();
        let later = now + std::time::Duration::from_millis(50);
        let mut state = NodeState::new();

        assert!(state.observe_validated_grid(complete_binding(now), STABLE_GRID, true));
        assert!(!state.observe_validated_grid(complete_binding(later), REPLACEMENT_GRID, true,));

        let binding = state.source_binding_observation.as_ref().unwrap();
        assert!(binding.complete);
        assert_eq!(binding.observed_at, later);
        assert_eq!(state.active_grid, Some(STABLE_GRID));
        assert_eq!(state.skipped_grid_frames, 1);
    }

    #[test]
    fn sealed_off_grid_frame_refreshes_binding_without_selecting_its_grid() {
        let now = std::time::Instant::now();
        let mut state = NodeState::new();

        assert!(!state.observe_validated_grid(complete_binding(now), REPLACEMENT_GRID, false,));

        assert!(state.source_binding_observation.is_some());
        assert_eq!(state.active_grid, None);
        assert_eq!(state.skipped_grid_frames, 1);
    }

    #[test]
    fn missing_incomplete_and_malformed_bindings_remain_fatal() {
        let now = std::time::Instant::now();
        assert!(validated_complete_source_binding(None, now, false).is_err());

        let incomplete = source_binding(0, '0');
        assert!(validated_complete_source_binding(Some(&incomplete), now, false).is_err());

        let malformed = source_binding(
            raw_csi_recording::SOURCE_BINDING_FLAG_FILTER_CONFIGURED,
            'a',
        );
        assert!(validated_complete_source_binding(Some(&malformed), now, false).is_err());

        let complete = source_binding(raw_csi_recording::SOURCE_BINDING_REQUIRED_FLAGS, 'a');
        assert!(validated_complete_source_binding(Some(&complete), now, false).is_ok());
    }

    #[test]
    fn switches_after_a_sustained_replacement_grid() {
        let mut state = NodeState::new();
        assert!(state.accept_grid(STABLE_GRID));
        state.frame_history.push_back(vec![1.0, 2.0]);
        state.baseline_motion = 4.2;
        state.baseline_frames = 17;
        state
            .d5_presence
            .install_reference_for_test(0.02, d5_presence::ROBUST_SCALE_FLOOR);
        state
            .d6_fingerprint
            .install_reference_for_test(&[1.0, 2.0, 1.0, 2.0])
            .unwrap();

        for _ in 1..GRID_SWITCH_CONFIRMATIONS {
            assert!(!state.accept_grid(REPLACEMENT_GRID));
        }
        assert!(state.accept_grid(REPLACEMENT_GRID));

        assert_eq!(state.active_grid, Some(REPLACEMENT_GRID));
        assert!(state.frame_history.is_empty());
        assert_eq!(state.baseline_motion, 0.0);
        assert_eq!(state.baseline_frames, 0);
        assert!(!state.d5_presence.reference_ready());
        assert!(!state.d6_fingerprint.reference_ready());
    }

    #[test]
    fn active_grid_resets_a_partial_replacement_candidate() {
        let mut state = NodeState::new();
        assert!(state.accept_grid(STABLE_GRID));
        for _ in 0..3 {
            assert!(!state.accept_grid(REPLACEMENT_GRID));
        }

        assert!(state.accept_grid(STABLE_GRID));
        assert_eq!(state.candidate_grid, None);
        assert_eq!(state.candidate_grid_hits, 0);
    }

    #[test]
    fn frequency_and_antenna_layout_are_part_of_the_grid_identity() {
        let mut channel_change = NodeState::new();
        assert!(channel_change.accept_grid(STABLE_GRID));
        assert!(!channel_change.accept_grid((2462, STABLE_GRID.1, STABLE_GRID.2, STABLE_GRID.3,)));

        let mut antenna_change = NodeState::new();
        assert!(antenna_change.accept_grid(STABLE_GRID));
        assert!(!antenna_change.accept_grid((STABLE_GRID.0, 2, STABLE_GRID.2, STABLE_GRID.3,)));
    }
}

/// Per-node feature info for WebSocket broadcasts (multi-node support).
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
                frame_rate_hz: if ns.csi_fps_samples >= 5 {
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

struct AppStateInner {
    latest_update: Option<SensingUpdate>,
    rssi_history: VecDeque<f64>,
    /// Circular buffer of recent CSI amplitude vectors for temporal analysis.
    /// Each entry is the full subcarrier amplitude vector for one frame.
    /// Capacity: FRAME_HISTORY_CAPACITY frames.
    frame_history: VecDeque<Vec<f64>>,
    tick: u64,
    source: String,
    /// Optional transmitter marker position for the live room visualization.
    tx_position: Option<[f64; 3]>,
    /// Optional physical room dimensions `[length, height, width]`.
    room_dimensions: Option<[f64; 3]>,
    /// Validated setup seal governing runtime geometry and raw recordings.
    position_setup: Option<Arc<position_setup::SealedPositionSetup>>,
    /// Independent mmWave teacher/reference state. It is never read by the
    /// WiFi position predictor.
    mmwave: mmwave_calibration::MmwaveManager,
    /// Fail-closed discrete live-position inference and temporal consensus.
    live_position_tracker: position_live::LivePositionTracker,
    /// Instant of the last ESP32 UDP frame received (for offline detection).
    last_esp32_frame: Option<std::time::Instant>,
    /// Instant of the last raw CSI frame accepted by the discrete position path.
    last_raw_csi_frame: Option<std::time::Instant>,
    tx: broadcast::Sender<String>,
    /// Private, server-local stream of lossless accepted ESP32 CSI frames.
    /// This is intentionally separate from the privacy-gated WebSocket output:
    /// recording and later offline validation need the exact I/Q samples.
    raw_csi_tx: broadcast::Sender<RawCsiIngress>,
    // ADR-099 D2/D3/D4: real-time CSI introspection tap. Per-frame state +
    // a parallel broadcast topic (`/ws/introspection`) running alongside
    // (not replacing) the window-aggregated `tx` / `/ws/sensing` pipeline.
    intro: wifi_densepose_sensing_server::introspection::IntrospectionState,
    intro_tx: broadcast::Sender<String>,
    total_detections: u64,
    start_time: std::time::Instant,
    /// Vital sign detector (processes CSI frames to estimate HR/RR).
    vital_detector: VitalSignDetector,
    /// Most recent vital sign reading for the REST endpoint.
    latest_vitals: VitalSigns,
    /// RVF container info if a model was loaded via `--load-rvf`.
    rvf_info: Option<RvfContainerInfo>,
    /// Path to save RVF container on shutdown (set via `--save-rvf`).
    save_rvf_path: Option<PathBuf>,
    /// Progressive loader for a trained model (set via `--model`).
    progressive_loader: Option<ProgressiveLoader>,
    /// Active SONA profile name.
    active_sona_profile: Option<String>,
    /// Whether a trained model is loaded.
    model_loaded: bool,
    /// Smoothed person count (EMA) for hysteresis — prevents frame-to-frame jumping.
    smoothed_person_score: f64,
    /// Previous person count for hysteresis (asymmetric up/down thresholds).
    prev_person_count: usize,
    // ── Motion smoothing & adaptive baseline (ADR-047 tuning) ────────────
    /// EMA-smoothed motion score (alpha ~0.15 for ~10 FPS → ~1s time constant).
    smoothed_motion: f64,
    /// Current classification state for hysteresis debounce.
    current_motion_level: String,
    /// How many consecutive frames the *raw* classification has agreed with a
    /// *candidate* new level.  State only changes after DEBOUNCE_FRAMES.
    debounce_counter: u32,
    /// The candidate motion level that the debounce counter is tracking.
    debounce_candidate: String,
    /// Adaptive baseline: EMA of motion score when room is "quiet" (low motion).
    /// Subtracted from raw score so slow environmental drift doesn't inflate readings.
    baseline_motion: f64,
    /// Number of frames processed so far (for baseline warm-up).
    baseline_frames: u64,
    // ── Vital signs smoothing ────────────────────────────────────────────
    /// EMA-smoothed heart rate (BPM).
    smoothed_hr: f64,
    /// EMA-smoothed breathing rate (BPM).
    smoothed_br: f64,
    /// EMA-smoothed HR confidence.
    smoothed_hr_conf: f64,
    /// EMA-smoothed BR confidence.
    smoothed_br_conf: f64,
    /// Median filter buffer for HR (last N raw values for outlier rejection).
    hr_buffer: VecDeque<f64>,
    /// Median filter buffer for BR.
    br_buffer: VecDeque<f64>,
    /// ADR-039: Latest edge vitals packet from ESP32.
    edge_vitals: Option<Esp32VitalsPacket>,
    /// ADR-040: Latest WASM output packet from ESP32.
    latest_wasm_events: Option<WasmOutputPacket>,
    // ── Model management fields ─────────────────────────────────────────────
    /// Discovered RVF model files from `data/models/`.
    discovered_models: Vec<serde_json::Value>,
    /// ID of the currently loaded model, if any.
    active_model_id: Option<String>,
    // ── Recording fields ────────────────────────────────────────────────────
    /// Metadata for recorded CSI data files.
    recordings: Vec<serde_json::Value>,
    /// Serializes file creation, finalization, and deletion so a recording ID
    /// cannot be deleted while another request is starting or finalizing it.
    recording_lifecycle: Arc<Mutex<()>>,
    /// Explicit recorder lifecycle. `recording_active` remains the hot-path
    /// producer gate; this phase also represents the finalization interval.
    recording_phase: RecordingLifecyclePhase,
    /// Whether CSI recording is currently in progress.
    recording_active: bool,
    /// When the current recording started.
    recording_start_time: Option<std::time::Instant>,
    /// ID of the current recording (used for filename).
    recording_current_id: Option<String>,
    /// Shutdown signal for the recording writer task.
    recording_stop_tx: Option<tokio::sync::watch::Sender<bool>>,
    /// Completion result from the writer. Stop and shutdown must await this
    /// before reporting the recording as durable or allowing delete/reuse.
    recording_done_rx: Option<tokio::sync::oneshot::Receiver<RecordingWriterResult>>,
    // ── Training fields ─────────────────────────────────────────────────────
    /// Training status: "idle", "running", "completed", "failed".
    training_status: String,
    /// Training configuration, if any.
    training_config: Option<serde_json::Value>,
    // ── Adaptive classifier (environment-tuned) ──────────────────────────
    /// Trained adaptive model (loaded from data/adaptive_model.json or trained at runtime).
    adaptive_model: Option<adaptive_classifier::AdaptiveModel>,
    // ── Per-node state (issue #249) ─────────────────────────────────────
    /// Per-node sensing state for multi-node deployments.
    /// Keyed by `node_id` from the ESP32 frame header.
    node_states: HashMap<u8, NodeState>,
    /// Experimental D5 calibration/fusion state. It remains uncalibrated until
    /// explicitly activated through the classification-calibration API.
    d5_presence: d5_presence::PresenceFusionState,
    // ── Accuracy sprint: Kalman tracker, multistatic fusion, eigenvalue counting ──
    /// Global Kalman-based pose tracker for stable person IDs and smoothed keypoints.
    pose_tracker: PoseTracker,
    /// Instant of last tracker update (for computing dt).
    last_tracker_instant: Option<std::time::Instant>,
    /// Attention-weighted multi-node CSI fusion engine.
    multistatic_fuser: MultistaticFuser,
    /// Governed trust-path bridge (ADR-135..146): runs the same live frames
    /// through the privacy/provenance/witness control plane. Does not alter
    /// person-count behavior; its trust state (witness, effective class,
    /// recalibration flag, error count) is recorded on the bridge itself and
    /// exposed via `GET /api/v1/status`, and a Restricted-class cycle strips
    /// per-node raw amplitudes from the live publish (review finding 1).
    engine_bridge: engine_bridge::EngineBridge,
    /// SVD-based room field model for eigenvalue person counting (None until calibration).
    field_model: Option<FieldModel>,
    // ── ADR-044 §5.2: adaptive rolling-p95 normalization ─────────────────────
    /// Rolling P95 of `FeatureInfo.variance` over the last ~30 s (600 frames @ 20 Hz).
    pub(crate) p95_variance: RollingP95,
    /// Rolling P95 of `FeatureInfo.motion_band_power` over the last ~30 s.
    pub(crate) p95_motion_band_power: RollingP95,
    /// Rolling P95 of `FeatureInfo.spectral_power` over the last ~30 s.
    pub(crate) p95_spectral_power: RollingP95,
    // ── ADR-044 §5.3: runtime-configurable dedup factor ───────────────────────
    /// Divisor for multi-node person-count deduplication (sum / factor).
    /// Default 3.0 (one body visible to ~3 nodes on average).
    /// Configurable at runtime via `POST /api/v1/config/dedup-factor` and
    /// `POST /api/v1/config/ground-truth`. Persisted across restarts.
    pub(crate) dedup_factor: f64,
    /// Data directory for persisting runtime config (parent of `firmware_dir`).
    pub(crate) data_dir: std::path::PathBuf,
    /// Optional local Observatory experiment catalogue. A database failure
    /// must not take the live sensing/read-only surface down with it.
    experiment_store: Option<Arc<experiment::ExperimentStore>>,
    /// ADR-262 P3: the live RuField surface. Holds the dedicated ed25519 signer
    /// + a bounded ring of recent signed `FieldEvent`s + the `/ws/field`
    /// broadcast topic. The governed sensing cycle calls `emit()` on it once per
    /// cycle (joining `SensingUpdate` features/classification/signal_field with
    /// the `TrustedOutput` trust class); `/api/field` + `/ws/field` read it.
    /// Held behind its own `Arc<RwLock<_>>` so the additive field router can
    /// take it as state without re-locking `AppStateInner`.
    field_surface: rufield_surface::FieldState,
}

/// If no ESP32 frame arrives within this duration, source reverts to offline.
const ESP32_OFFLINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// A sealed preflight may only trust TX-source evidence from the current live
/// stream. Kept equal to the capture runner's hard maximum.
const SOURCE_BINDING_FRESHNESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// A measured position must not outlive its raw CSI input.
const POSITION_RAW_STALE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

impl AppStateInner {
    /// Return the effective data source, accounting for ESP32 frame timeout.
    /// If the source is "esp32" but no frame has arrived in 5 seconds, returns
    /// "esp32:offline" so the UI can distinguish active vs stale connections.
    /// Person count: eigenvalue-based if field model is calibrated, else heuristic.
    /// Uses global frame_history if populated, otherwise the freshest per-node history.
    fn person_count(&self) -> usize {
        match self.field_model.as_ref() {
            Some(fm) => {
                // Prefer global frame_history (populated by wifi/simulate paths).
                // Fall back to freshest per-node history (populated by ESP32 paths).
                let history = if !self.frame_history.is_empty() {
                    &self.frame_history
                } else {
                    // Find the node with the most recent frame
                    self.node_states
                        .values()
                        .filter(|ns| !ns.frame_history.is_empty())
                        .max_by_key(|ns| ns.last_frame_time)
                        .map(|ns| &ns.frame_history)
                        .unwrap_or(&self.frame_history)
                };
                field_bridge::occupancy_or_fallback(
                    fm,
                    history,
                    self.smoothed_person_score,
                    self.prev_person_count,
                )
            }
            None => score_to_person_count(self.smoothed_person_score, self.prev_person_count),
        }
    }

    fn effective_source(&self) -> String {
        if self.source == "esp32" {
            if let Some(last) = self.last_esp32_frame {
                if last.elapsed() > ESP32_OFFLINE_TIMEOUT {
                    return "esp32:offline".to_string();
                }
            }
        }
        self.source.clone()
    }
}

/// Number of frames retained in `frame_history` for temporal analysis.
/// At 500 ms ticks this covers ~50 seconds; at 100 ms ticks ~10 seconds.
const FRAME_HISTORY_CAPACITY: usize = 100;

type SharedState = Arc<RwLock<AppStateInner>>;

// ── ESP32 Edge Vitals Packet (ADR-039, magic 0xC511_0002) ────────────────────

/// Decoded vitals packet from ESP32 edge processing pipeline.
#[derive(Debug, Clone, Serialize)]
struct Esp32VitalsPacket {
    node_id: u8,
    presence: bool,
    fall_detected: bool,
    motion: bool,
    breathing_rate_bpm: f64,
    heartrate_bpm: f64,
    rssi: i8,
    n_persons: u8,
    motion_energy: f32,
    presence_score: f32,
    timestamp_ms: u32,
}

/// Parse a 32-byte edge vitals packet (magic 0xC511_0002).
fn parse_esp32_vitals(buf: &[u8]) -> Option<Esp32VitalsPacket> {
    if buf.len() < 32 {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0002 {
        return None;
    }

    let node_id = buf[4];
    let flags = buf[5];
    let breathing_raw = u16::from_le_bytes([buf[6], buf[7]]);
    let heartrate_raw = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let rssi = buf[12] as i8;
    let n_persons = buf[13];
    let motion_energy = f32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let presence_score = f32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
    let timestamp_ms = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);

    Some(Esp32VitalsPacket {
        node_id,
        presence: (flags & 0x01) != 0,
        fall_detected: (flags & 0x02) != 0,
        motion: (flags & 0x04) != 0,
        breathing_rate_bpm: breathing_raw as f64 / 100.0,
        heartrate_bpm: heartrate_raw as f64 / 10000.0,
        rssi,
        n_persons,
        motion_energy,
        presence_score,
        timestamp_ms,
    })
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

// ── ADR-040: WASM Output Packet (magic 0xC511_0007 — reassigned per #928) ─────

/// Single WASM event (type + value).
#[derive(Debug, Clone, Serialize)]
struct WasmEvent {
    event_type: u8,
    value: f32,
}

/// Decoded WASM output packet from ESP32 Tier 3 runtime.
#[derive(Debug, Clone, Serialize)]
struct WasmOutputPacket {
    node_id: u8,
    module_id: u8,
    events: Vec<WasmEvent>,
}

/// Parse a WASM output packet (magic 0xC511_0007 — reassigned per issue #928;
/// the original 0xC511_0004 was a collision with ADR-063 fused vitals).
fn parse_wasm_output(buf: &[u8]) -> Option<WasmOutputPacket> {
    if buf.len() < 8 {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0007 {
        return None;
    }

    let node_id = buf[4];
    let module_id = buf[5];
    let event_count = u16::from_le_bytes([buf[6], buf[7]]) as usize;

    let mut events = Vec::with_capacity(event_count);
    let mut offset = 8;
    for _ in 0..event_count {
        if offset + 5 > buf.len() {
            break;
        }
        let event_type = buf[offset];
        let value = f32::from_le_bytes([
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
        ]);
        events.push(WasmEvent { event_type, value });
        offset += 5;
    }

    Some(WasmOutputPacket {
        node_id,
        module_id,
        events,
    })
}

// ── ADR-063: Edge Fused Vitals Packet (magic 0xC511_0004) ─────────────────────
//
// 48-byte packed struct emitted by the ESP32-C6 + MR60BHA2 mmWave config when
// `mmwave_sensor_get_state().detected` is true. Byte layout from
// `firmware/esp32-csi-node/main/edge_processing.h` line 129 — kept in lockstep
// with the firmware's `_Static_assert(sizeof(edge_fused_vitals_pkt_t) == 48)`.
// Issue #928 surfaced that this magic was being parsed as WASM output and the
// fused vitals were silently lost. Adding the proper parser here.

#[derive(Debug, Clone, Serialize)]
struct EdgeFusedVitalsPacket {
    node_id: u8,
    /// Bit0=presence, Bit1=fall, Bit2=motion, Bit3=mmwave_present.
    flags: u8,
    /// Fused breathing rate in BPM (firmware sends BPM*100; we scale here).
    breathing_rate_bpm: f32,
    /// Fused heartrate in BPM (firmware sends BPM*10000; we scale here).
    heartrate_bpm: f32,
    rssi: i8,
    n_persons: u8,
    /// `mmwave_type_t` enum value from firmware.
    mmwave_type: u8,
    /// 0-100 fusion quality score.
    fusion_confidence: u8,
    motion_energy: f32,
    presence_score: f32,
    timestamp_ms: u32,
    /// Raw mmWave heart rate (BPM).
    mmwave_hr_bpm: f32,
    /// Raw mmWave breathing rate (BPM).
    mmwave_br_bpm: f32,
    /// Distance to nearest target (cm).
    mmwave_distance_cm: f32,
    /// Target count from mmWave.
    mmwave_targets: u8,
    /// mmWave signal quality 0-100.
    mmwave_confidence: u8,
}

/// Parse an ADR-063 edge fused vitals packet (magic 0xC511_0004, 48 bytes).
fn parse_edge_fused_vitals(buf: &[u8]) -> Option<EdgeFusedVitalsPacket> {
    if buf.len() < 48 {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0004 {
        return None;
    }

    let node_id = buf[4];
    let flags = buf[5];
    let breathing_raw = u16::from_le_bytes([buf[6], buf[7]]);
    let heartrate_raw = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let rssi = buf[12] as i8;
    let n_persons = buf[13];
    let mmwave_type = buf[14];
    let fusion_confidence = buf[15];
    let motion_energy = f32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let presence_score = f32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
    let timestamp_ms = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    let mmwave_hr_bpm = f32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
    let mmwave_br_bpm = f32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]);
    let mmwave_distance_cm = f32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]);
    let mmwave_targets = buf[40];
    let mmwave_confidence = buf[41];
    // buf[42..48] are firmware reserved fields (reserved3 u16 + reserved4 u32).

    Some(EdgeFusedVitalsPacket {
        node_id,
        flags,
        breathing_rate_bpm: breathing_raw as f32 / 100.0,
        heartrate_bpm: heartrate_raw as f32 / 10000.0,
        rssi,
        n_persons,
        mmwave_type,
        fusion_confidence,
        motion_energy,
        presence_score,
        timestamp_ms,
        mmwave_hr_bpm,
        mmwave_br_bpm,
        mmwave_distance_cm,
        mmwave_targets,
        mmwave_confidence,
    })
}

#[cfg(test)]
mod issue_928_magic_collision_tests {
    //! Issue #928 — `0xC511_0004` was being parsed as WASM output, eating the
    //! C6+mmWave fused-vitals packets. After this fix, `0xC511_0004` routes to
    //! `parse_edge_fused_vitals` and WASM output owns the freshly-allocated
    //! `0xC511_0007` slot. Tests guard both halves of the swap.
    use super::*;

    /// Build a 48-byte synthetic fused-vitals packet matching the firmware's
    /// `edge_fused_vitals_pkt_t` layout from `edge_processing.h:129`.
    fn build_fused_vitals_packet() -> Vec<u8> {
        let mut buf = vec![0u8; 48];
        buf[0..4].copy_from_slice(&0xC511_0004u32.to_le_bytes());
        buf[4] = 9; // node_id
        buf[5] = 0b0000_1001; // flags: presence | mmwave_present
        buf[6..8].copy_from_slice(&1600u16.to_le_bytes()); // breathing 16.00 BPM
        buf[8..12].copy_from_slice(&720_000u32.to_le_bytes()); // heartrate 72.0 BPM
        buf[12] = (-55i8) as u8; // rssi
        buf[13] = 1; // n_persons
        buf[14] = 2; // mmwave_type
        buf[15] = 85; // fusion_confidence
        buf[16..20].copy_from_slice(&0.42f32.to_le_bytes()); // motion_energy
        buf[20..24].copy_from_slice(&0.95f32.to_le_bytes()); // presence_score
        buf[24..28].copy_from_slice(&1_234_567u32.to_le_bytes()); // timestamp_ms
        buf[28..32].copy_from_slice(&71.5f32.to_le_bytes()); // mmwave_hr_bpm
        buf[32..36].copy_from_slice(&15.8f32.to_le_bytes()); // mmwave_br_bpm
        buf[36..40].copy_from_slice(&182.0f32.to_le_bytes()); // mmwave_distance_cm
        buf[40] = 1; // mmwave_targets
        buf[41] = 90; // mmwave_confidence
                      // bytes 42..48 — firmware reserved fields, left as zero
        buf
    }

    #[test]
    fn parse_edge_fused_vitals_extracts_fields_correctly() {
        let buf = build_fused_vitals_packet();
        let pkt = parse_edge_fused_vitals(&buf).expect("must parse a well-formed packet");
        assert_eq!(pkt.node_id, 9);
        assert_eq!(pkt.flags, 0b0000_1001);
        assert!(
            (pkt.breathing_rate_bpm - 16.0).abs() < 1e-3,
            "breathing scale 100"
        );
        assert!(
            (pkt.heartrate_bpm - 72.0).abs() < 1e-3,
            "heartrate scale 10000"
        );
        assert_eq!(pkt.rssi, -55);
        assert_eq!(pkt.n_persons, 1);
        assert_eq!(pkt.mmwave_type, 2);
        assert_eq!(pkt.fusion_confidence, 85);
        assert!((pkt.motion_energy - 0.42).abs() < 1e-6);
        assert!((pkt.presence_score - 0.95).abs() < 1e-6);
        assert_eq!(pkt.timestamp_ms, 1_234_567);
        assert!((pkt.mmwave_hr_bpm - 71.5).abs() < 1e-6);
        assert!((pkt.mmwave_br_bpm - 15.8).abs() < 1e-3);
        assert!((pkt.mmwave_distance_cm - 182.0).abs() < 1e-6);
        assert_eq!(pkt.mmwave_targets, 1);
        assert_eq!(pkt.mmwave_confidence, 90);
    }

    #[test]
    fn parse_edge_fused_vitals_rejects_short_buffer() {
        let buf = build_fused_vitals_packet();
        // Truncate to 47 bytes — one short of the 48-byte minimum.
        assert!(parse_edge_fused_vitals(&buf[..47]).is_none());
    }

    #[test]
    fn parse_edge_fused_vitals_rejects_wrong_magic() {
        let mut buf = build_fused_vitals_packet();
        buf[0..4].copy_from_slice(&0xC511_0007u32.to_le_bytes()); // WASM magic, not fused
        assert!(parse_edge_fused_vitals(&buf).is_none());
    }

    #[test]
    fn parse_wasm_output_rejects_legacy_0004_magic() {
        // The old WASM magic collided with fused vitals — must no longer be
        // accepted. A real fused-vitals packet starts with 0xC511_0004 and
        // would have been misparsed before this fix.
        let buf = build_fused_vitals_packet();
        assert!(
            parse_wasm_output(&buf).is_none(),
            "issue #928: WASM parser must NOT accept 0xC511_0004"
        );
    }

    #[test]
    fn parse_wasm_output_accepts_new_0007_magic() {
        // Build a tiny well-formed WASM output packet on the new magic.
        let mut buf = vec![0u8; 8];
        buf[0..4].copy_from_slice(&0xC511_0007u32.to_le_bytes());
        buf[4] = 5; // node_id
        buf[5] = 1; // module_id
        buf[6..8].copy_from_slice(&0u16.to_le_bytes()); // event_count = 0
        let pkt = parse_wasm_output(&buf).expect("0xC511_0007 must parse");
        assert_eq!(pkt.node_id, 5);
        assert_eq!(pkt.module_id, 1);
        assert!(pkt.events.is_empty());
    }
}

// ── ESP32 UDP frame parser ───────────────────────────────────────────────────

fn has_esp32_csi_magic(buf: &[u8]) -> bool {
    buf.get(0..4)
        .and_then(|magic| <[u8; 4]>::try_from(magic).ok())
        .is_some_and(|magic| u32::from_le_bytes(magic) == raw_csi_recording::ESP32_CSI_MAGIC)
}

fn esp32_csi_header_rx_id(buf: &[u8]) -> Option<u8> {
    has_esp32_csi_magic(buf)
        .then(|| buf.get(4).copied())
        .flatten()
}

fn parse_esp32_frame(buf: &[u8]) -> Option<Esp32Frame> {
    if buf.len() < 20 {
        return None;
    }

    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0001 {
        return None;
    }

    // Frame layout (must match firmware csi_collector.c):
    //   [0..3]   magic (u32 LE)
    //   [4]      node_id (u8)
    //   [5]      n_antennas (u8)
    //   [6..7]   n_subcarriers (u16 LE)
    //   [8..11]  freq_mhz (u32 LE)
    //   [12..15] sequence (u32 LE)
    //   [16]     rssi (i8)
    //   [17]     noise_floor (i8)
    //   [18..19] reserved
    //   [20..]   I/Q data
    // Issue #1005: until 2026-06 this code read n_subcarriers from byte 6
    // alone (an ESP32-C6 HE-SU frame's 256 = 0x0100 LE decoded as 0 — the
    // frame parsed with zero subcarriers) and read sequence/rssi/noise at
    // stale offsets 10/14/15. Offsets below match the comment (and firmware).
    let node_id = buf[4];
    let n_antennas = buf[5];
    let n_subcarriers = u16::from_le_bytes([buf[6], buf[7]]);
    let freq_mhz =
        u16::try_from(u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]])).unwrap_or(0);
    let sequence = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let rssi_raw = buf[16] as i8;
    // Fix RSSI sign: ensure it's always negative (dBm convention).
    let rssi = if rssi_raw > 0 {
        rssi_raw.saturating_neg()
    } else {
        rssi_raw
    };
    let noise_floor = buf[17] as i8;
    let ppdu_type = wifi_densepose_hardware::PpduType::from_byte(buf[18]);

    let iq_start = 20;
    let n_pairs = n_antennas as usize * n_subcarriers as usize;
    let expected_len = iq_start + n_pairs * 2;

    if buf.len() < expected_len {
        return None;
    }

    let mut amplitudes = Vec::with_capacity(n_pairs);
    let mut phases = Vec::with_capacity(n_pairs);

    for k in 0..n_pairs {
        let i_val = buf[iq_start + k * 2] as i8 as f64;
        let q_val = buf[iq_start + k * 2 + 1] as i8 as f64;
        amplitudes.push((i_val * i_val + q_val * q_val).sqrt());
        phases.push(q_val.atan2(i_val));
    }

    Some(Esp32Frame {
        magic,
        node_id,
        n_antennas,
        n_subcarriers,
        freq_mhz,
        sequence,
        rssi,
        noise_floor,
        ppdu_type,
        amplitudes,
        phases,
    })
}

#[cfg(test)]
mod issue_1009_n_subcarriers_u16_tests {
    //! Issue #1009 §1c — `parse_esp32_frame` must read `n_subcarriers` as a
    //! u16 LE at bytes 6..7 (ADR-018 wire format), not a single byte at 6.
    //!
    //! An ESP32-C6 HE20 frame carries 256 subcarriers → byte 6 = 0x00,
    //! byte 7 = 0x01. The pre-#1005 single-byte read decoded this as 0
    //! subcarriers, silently dropping every real HE20 frame. This was the same
    //! truncation as the CLI parser (`wifi-densepose-cli` calibrate.rs); this
    //! module pins that the sensing-server template stays u16-correct.
    use super::*;

    /// Build an ADR-018 CSI frame (magic 0xC511_0001, 20-byte header).
    fn build_csi_frame(n_subcarriers: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 20 + n_subcarriers as usize * 2];
        buf[0..4].copy_from_slice(&0xC511_0001u32.to_le_bytes());
        buf[4] = 7; // node_id
        buf[5] = 1; // n_antennas
        buf[6..8].copy_from_slice(&n_subcarriers.to_le_bytes()); // u16 LE
        buf[8..12].copy_from_slice(&5180u32.to_le_bytes()); // freq_mhz (5 GHz HE)
        buf[12..16].copy_from_slice(&42u32.to_le_bytes()); // sequence
        buf[16] = (-40i8) as u8; // rssi
        buf[17] = (-90i8) as u8; // noise_floor
        buf[18] = 0; // ppdu_type
        buf[19] = 0;
        for k in 0..n_subcarriers as usize {
            buf[20 + k * 2] = (5 + (k % 40) as i8) as u8; // i
            buf[20 + k * 2 + 1] = (k % 30) as u8; // q
        }
        buf
    }

    #[test]
    fn parse_esp32_frame_he20_256_bins_not_truncated() {
        // 256 = 0x0100 LE: byte6 = 0x00, byte7 = 0x01. A u8 read of byte 6
        // would see 0 subcarriers; a u16 read sees 256.
        let buf = build_csi_frame(256);
        assert_eq!(buf.len(), 532, "256-bin frame wire size = 20 + 256*2");
        let frame = parse_esp32_frame(&buf).expect("256-bin HE20 frame must parse");
        assert_eq!(
            frame.n_subcarriers, 256,
            "n_subcarriers must read as u16 (256), not the byte-6-only 0"
        );
        assert_eq!(frame.amplitudes.len(), 256);
        assert_eq!(frame.node_id, 7);
        assert_eq!(frame.rssi, -40);
        assert_eq!(frame.sequence, 42);
    }

    #[test]
    fn parse_esp32_frame_ht20_64_bins_still_parses() {
        // Regression guard for the common single-byte (≤255) case.
        let buf = build_csi_frame(64);
        let frame = parse_esp32_frame(&buf).expect("64-bin HT20 frame must parse");
        assert_eq!(frame.n_subcarriers, 64);
        assert_eq!(frame.amplitudes.len(), 64);
    }

    #[test]
    fn malformed_csi_magic_still_identifies_safe_rx_header() {
        let mut truncated = build_csi_frame(64);
        truncated.truncate(20);

        assert!(has_esp32_csi_magic(&truncated));
        assert_eq!(esp32_csi_header_rx_id(&truncated), Some(7));
        assert!(parse_esp32_frame(&truncated).is_none());
    }

    #[test]
    fn foreign_or_too_short_datagrams_are_not_csi_ingress() {
        let foreign = 0xC511_0007u32.to_le_bytes();
        assert!(!has_esp32_csi_magic(&foreign));
        assert_eq!(esp32_csi_header_rx_id(&foreign), None);

        let csi_magic_without_rx = raw_csi_recording::ESP32_CSI_MAGIC.to_le_bytes();
        assert!(has_esp32_csi_magic(&csi_magic_without_rx));
        assert_eq!(esp32_csi_header_rx_id(&csi_magic_without_rx), None);
    }
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

/// Clone a sensing update for public delivery and fail closed when the ESP32
/// source has gone offline. Fixed deployment geometry remains configured in
/// `tx_position` / `room_dimensions`, but stale measurements and detections are
/// never rebroadcast as current evidence.
fn public_sensing_update(update: &SensingUpdate, effective_source: &str) -> SensingUpdate {
    let mut public = update.clone();
    public.source = effective_source.to_string();
    if effective_source != "esp32:offline" {
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

async fn windows_wifi_task(state: SharedState, tick_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
    let mut seq: u32 = 0;

    // ADR-022 Phase 3: Multi-BSSID pipeline state (kept across ticks)
    let mut registry = BssidRegistry::new(32, 30);
    let mut pipeline = WindowsWifiPipeline::new();

    info!(
        "Windows WiFi multi-BSSID pipeline active (tick={}ms, max_bssids=32)",
        tick_ms
    );

    loop {
        interval.tick().await;
        seq += 1;

        // ── Step 1: Run multi-BSSID scan via spawn_blocking ──────────
        // NetshBssidScanner is not Send, so we run `netsh` and parse
        // the output inside a blocking closure.
        let bssid_scan_result = tokio::task::spawn_blocking(|| {
            let output = std::process::Command::new("netsh")
                .args(["wlan", "show", "networks", "mode=bssid"])
                .output()
                .map_err(|e| format!("netsh bssid scan failed: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "netsh exited with {}: {}",
                    output.status,
                    stderr.trim()
                ));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_netsh_bssid_output(&stdout).map_err(|e| format!("parse error: {e}"))
        })
        .await;

        // Unwrap the JoinHandle result, then the inner Result.
        let observations = match bssid_scan_result {
            Ok(Ok(obs)) if !obs.is_empty() => obs,
            Ok(Ok(_empty)) => {
                debug!("Multi-BSSID scan returned 0 observations, falling back");
                windows_wifi_fallback_tick(&state, seq).await;
                continue;
            }
            Ok(Err(e)) => {
                warn!("Multi-BSSID scan error: {e}, falling back");
                windows_wifi_fallback_tick(&state, seq).await;
                continue;
            }
            Err(join_err) => {
                error!("spawn_blocking panicked: {join_err}");
                continue;
            }
        };

        let obs_count = observations.len();

        // Derive SSID from the first observation for the source label.
        let ssid = observations
            .first()
            .map(|o| o.ssid.clone())
            .unwrap_or_else(|| "Unknown".into());

        // ── Step 2: Feed observations into registry ──────────────────
        registry.update(&observations);
        let multi_ap_frame = registry.to_multi_ap_frame();

        // ── Step 3: Run enhanced pipeline ────────────────────────────
        let enhanced = pipeline.process(&multi_ap_frame);

        // ── Step 4: Build backward-compatible Esp32Frame ─────────────
        let first_rssi = observations.first().map(|o| o.rssi_dbm).unwrap_or(-80.0);
        let _first_signal_pct = observations.first().map(|o| o.signal_pct).unwrap_or(40.0);

        let frame = Esp32Frame {
            magic: 0xC511_0001,
            node_id: 0,
            n_antennas: 1,
            n_subcarriers: obs_count.min(u16::MAX as usize) as u16,
            freq_mhz: 2437,
            sequence: seq,
            rssi: first_rssi.clamp(-128.0, 127.0) as i8,
            noise_floor: -90,
            ppdu_type: wifi_densepose_hardware::PpduType::HtLegacy,
            amplitudes: multi_ap_frame.amplitudes.clone(),
            phases: multi_ap_frame.phases.clone(),
        };

        // ── Step 4b: Update frame history and extract features ───────
        let mut s_write_pre = state.write().await;
        s_write_pre
            .frame_history
            .push_back(frame.amplitudes.clone());
        if s_write_pre.frame_history.len() > FRAME_HISTORY_CAPACITY {
            s_write_pre.frame_history.pop_front();
        }
        let sample_rate_hz = 1000.0 / tick_ms as f64;
        let (features, mut classification, breathing_rate_hz, sub_variances, raw_motion) =
            extract_features_from_frame(&frame, &s_write_pre.frame_history, sample_rate_hz);
        smooth_and_classify(&mut s_write_pre, &mut classification, raw_motion);
        adaptive_override(&s_write_pre, &features, &mut classification);
        drop(s_write_pre);

        // ── Step 5: Build enhanced fields from pipeline result ───────
        let enhanced_motion = Some(serde_json::json!({
            "score": enhanced.motion.score,
            "level": format!("{:?}", enhanced.motion.level),
            "contributing_bssids": enhanced.motion.contributing_bssids,
        }));

        let enhanced_breathing = enhanced.breathing.as_ref().map(|b| {
            serde_json::json!({
                "rate_bpm": b.rate_bpm,
                "confidence": b.confidence,
                "bssid_count": b.bssid_count,
            })
        });

        let posture_str = enhanced.posture.map(|p| format!("{p:?}"));
        let sig_quality_score = Some(enhanced.signal_quality.score);
        let verdict_str = Some(format!("{:?}", enhanced.verdict));
        let bssid_n = Some(enhanced.bssid_count);

        // ── Step 6: Update shared state ──────────────────────────────
        let mut s = state.write().await;
        s.source = format!("wifi:{ssid}");
        s.rssi_history.push_back(first_rssi);
        if s.rssi_history.len() > 60 {
            s.rssi_history.pop_front();
        }

        s.tick += 1;
        let tick = s.tick;

        let motion_score = motion_score_for_level(&classification.motion_level);

        let raw_vitals = s
            .vital_detector
            .process_frame(&frame.amplitudes, &frame.phases);
        let vitals = smooth_vitals(&mut s, &raw_vitals);
        s.latest_vitals = vitals.clone();

        let feat_variance = features.variance;

        // ADR-044 §5.2: feed raw features into rolling-P95 estimators before scoring.
        s.p95_variance.push(features.variance);
        s.p95_motion_band_power.push(features.motion_band_power);
        s.p95_spectral_power.push(features.spectral_power);

        // Multi-person estimation with temporal smoothing (EMA α=0.10).
        let raw_score = compute_person_score(&s, &features);
        s.smoothed_person_score = s.smoothed_person_score * 0.90 + raw_score * 0.10;
        let est_persons = if classification.presence {
            let count = s.person_count();
            s.prev_person_count = count;
            count
        } else {
            s.prev_person_count = 0;
            0
        };

        let mut update = SensingUpdate {
            msg_type: "sensing_update".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            source: format!("wifi:{ssid}"),
            tick,
            tx_position: s.tx_position,
            room_dimensions: s.room_dimensions,
            nodes: vec![NodeInfo {
                node_id: 0,
                rssi_dbm: first_rssi,
                position: [0.0, 0.0, 0.0],
                amplitude: multi_ap_frame.amplitudes,
                subcarrier_count: obs_count,
                sync: None, // multi-BSSID scan path — no mesh peer
            }],
            features,
            classification,
            signal_field: generate_signal_field(
                first_rssi,
                motion_score,
                breathing_rate_hz,
                feat_variance.min(1.0),
                &sub_variances,
            ),
            localization: None,
            position_estimate: None,
            vital_signs: Some(vitals),
            enhanced_motion,
            enhanced_breathing,
            posture: posture_str,
            signal_quality_score: sig_quality_score,
            quality_verdict: verdict_str,
            bssid_count: bssid_n,
            pose_keypoints: None,
            model_status: None,
            persons: None,
            estimated_persons: if est_persons > 0 {
                Some(est_persons)
            } else {
                None
            },
            node_features: None,
        };

        // Populate persons from the sensing update (Kalman-smoothed via tracker).
        let raw_persons = derive_pose_from_sensing(&update);
        let mut last_tracker_instant = s.last_tracker_instant.take();
        let tracked = tracker_bridge::tracker_update(
            &mut s.pose_tracker,
            &mut last_tracker_instant,
            raw_persons,
        );
        s.last_tracker_instant = last_tracker_instant;
        if !tracked.is_empty() {
            update.persons = Some(tracked);
        }
        // #1050: attach real signal_field-peak positions to each person.
        attach_field_positions(&mut update);

        if let Ok(json) = serde_json::to_string(&update) {
            let _ = s.tx.send(json);
        }
        s.latest_update = Some(update);

        debug!(
            "Multi-BSSID tick #{tick}: {obs_count} BSSIDs, quality={:.2}, verdict={:?}",
            enhanced.signal_quality.score, enhanced.verdict
        );
    }
}

/// Fallback: single-RSSI collection via `netsh wlan show interfaces`.
///
/// Used when the multi-BSSID scan fails or returns 0 observations.
async fn windows_wifi_fallback_tick(state: &SharedState, seq: u32) {
    let output = match tokio::process::Command::new("netsh")
        .args(["wlan", "show", "interfaces"])
        .output()
        .await
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            warn!("netsh interfaces fallback failed: {e}");
            return;
        }
    };

    let (rssi_dbm, signal_pct, ssid) = match parse_netsh_interfaces_output(&output) {
        Some(v) => v,
        None => {
            debug!("Fallback: no WiFi interface connected");
            return;
        }
    };

    let frame = Esp32Frame {
        magic: 0xC511_0001,
        node_id: 0,
        n_antennas: 1,
        n_subcarriers: 1,
        freq_mhz: 2437,
        sequence: seq,
        rssi: rssi_dbm as i8,
        noise_floor: -90,
        ppdu_type: wifi_densepose_hardware::PpduType::HtLegacy,
        amplitudes: vec![signal_pct],
        phases: vec![0.0],
    };

    let mut s = state.write().await;
    // Update frame history before extracting features.
    s.frame_history.push_back(frame.amplitudes.clone());
    if s.frame_history.len() > FRAME_HISTORY_CAPACITY {
        s.frame_history.pop_front();
    }
    let sample_rate_hz = 2.0_f64; // fallback tick ~ 500 ms => 2 Hz
    let (features, mut classification, breathing_rate_hz, sub_variances, raw_motion) =
        extract_features_from_frame(&frame, &s.frame_history, sample_rate_hz);
    smooth_and_classify(&mut s, &mut classification, raw_motion);
    adaptive_override(&s, &features, &mut classification);

    s.source = format!("wifi:{ssid}");
    s.rssi_history.push_back(rssi_dbm);
    if s.rssi_history.len() > 60 {
        s.rssi_history.pop_front();
    }

    s.tick += 1;
    let tick = s.tick;

    let motion_score = motion_score_for_level(&classification.motion_level);

    let raw_vitals = s
        .vital_detector
        .process_frame(&frame.amplitudes, &frame.phases);
    let vitals = smooth_vitals(&mut s, &raw_vitals);
    s.latest_vitals = vitals.clone();

    let feat_variance = features.variance;

    // ADR-044 §5.2: feed raw features into rolling-P95 estimators before scoring.
    s.p95_variance.push(features.variance);
    s.p95_motion_band_power.push(features.motion_band_power);
    s.p95_spectral_power.push(features.spectral_power);

    // Multi-person estimation with temporal smoothing (EMA α=0.10).
    let raw_score = compute_person_score(&s, &features);
    s.smoothed_person_score = s.smoothed_person_score * 0.90 + raw_score * 0.10;
    let est_persons = if classification.presence {
        let count = s.person_count();
        s.prev_person_count = count;
        count
    } else {
        s.prev_person_count = 0;
        0
    };

    let mut update = SensingUpdate {
        msg_type: "sensing_update".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        source: format!("wifi:{ssid}"),
        tick,
        tx_position: s.tx_position,
        room_dimensions: s.room_dimensions,
        nodes: vec![NodeInfo {
            node_id: 0,
            rssi_dbm,
            position: [0.0, 0.0, 0.0],
            amplitude: vec![signal_pct],
            subcarrier_count: 1,
            sync: None, // synthetic-RSSI fallback path — no mesh peer
        }],
        features,
        classification,
        signal_field: generate_signal_field(
            rssi_dbm,
            motion_score,
            breathing_rate_hz,
            feat_variance.min(1.0),
            &sub_variances,
        ),
        localization: None,
        position_estimate: None,
        vital_signs: Some(vitals),
        enhanced_motion: None,
        enhanced_breathing: None,
        posture: None,
        signal_quality_score: None,
        quality_verdict: None,
        bssid_count: None,
        pose_keypoints: None,
        model_status: None,
        persons: None,
        estimated_persons: if est_persons > 0 {
            Some(est_persons)
        } else {
            None
        },
        node_features: None,
    };

    let raw_persons = derive_pose_from_sensing(&update);
    let mut last_tracker_instant = s.last_tracker_instant.take();
    let tracked =
        tracker_bridge::tracker_update(&mut s.pose_tracker, &mut last_tracker_instant, raw_persons);
    s.last_tracker_instant = last_tracker_instant;
    if !tracked.is_empty() {
        update.persons = Some(tracked);
    }
    // #1050: attach real signal_field-peak positions to each person.
    attach_field_positions(&mut update);

    if let Ok(json) = serde_json::to_string(&update) {
        let _ = s.tx.send(json);
    }
    s.latest_update = Some(update);
}

/// Probe if Windows WiFi is connected
async fn probe_windows_wifi() -> bool {
    match tokio::process::Command::new("netsh")
        .args(["wlan", "show", "interfaces"])
        .output()
        .await
    {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout);
            parse_netsh_interfaces_output(&out).is_some()
        }
        Err(_) => false,
    }
}

/// Probe if ESP32 is streaming on UDP port
async fn probe_esp32(port: u16) -> bool {
    let addr = format!("0.0.0.0:{port}");
    match UdpSocket::bind(&addr).await {
        Ok(sock) => {
            // 4096 covers the largest ADR-018 frame plus the optional
            // 40-byte runtime TX-source-binding trailer. On Windows a too-small
            // recv buffer makes recv_from error on the oversized datagram,
            // which made this probe fail against HE-only streams.
            let mut buf = [0u8; 4096];
            match tokio::time::timeout(Duration::from_secs(2), sock.recv_from(&mut buf)).await {
                Ok(Ok((len, _))) => parse_esp32_frame(&buf[..len]).is_some(),
                _ => false,
            }
        }
        Err(_) => false,
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
/// regardless of the boot probe. If no real source is up yet, serve simulated
/// data *and* keep the UDP receiver listening; the receiver promotes
/// `source` → `esp32` the instant the first real frame lands (see
/// `udp_receiver_task`, which sets `s.source = "esp32"`), mirroring the inverse
/// `esp32 → esp32:offline` reversion already in `effective_source()`.
///
/// Explicit `--source simulated` is a hard override for offline demos: it does
/// NOT bind UDP, so no promotion ever happens.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourcePlan {
    /// The `AppStateInner.source` value to start with.
    initial_source: String,
    /// Bind the UDP :5005 receiver (and thus allow simulate→esp32 promotion).
    bind_udp: bool,
    /// Run the simulated-data generator (serves poses until a real frame arrives).
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
                // No real source *yet*. Serve simulated data, but ALSO bind UDP
                // so the receiver can promote to esp32 when the first real
                // frame arrives (issue #1004). Never latch on simulate.
                SourcePlan {
                    initial_source: "simulated".to_string(),
                    bind_udp: true,
                    run_simulator: true,
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
    //! simulated data is served only until the first real frame, then
    //! `udp_receiver_task` promotes `source` → "esp32". These tests pin the
    //! resolution/promotion state machine directly (no sockets bound).
    use super::*;

    // FAILS ON OLD CODE: the old `auto`-with-no-source path bound no UDP
    // receiver (it spawned only `simulated_data_task`, or exited). This asserts
    // UDP IS bound even when the boot probe finds no source.
    #[test]
    fn auto_with_no_boot_source_still_binds_udp_and_simulates() {
        let plan = plan_source("auto", false, false);
        assert!(
            plan.bind_udp,
            "auto must bind UDP :5005 even with no boot source (#1004)"
        );
        assert!(
            plan.run_simulator,
            "auto must serve simulated data until real CSI arrives"
        );
        assert!(!plan.run_wifi);
        assert_eq!(plan.initial_source, "simulated");
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
    fn effective_source_promotes_from_simulated_to_esp32_on_real_frame() {
        // Start as the auto/simulate plan would: source = "simulated".
        let mut src = "simulated".to_string();
        // effective_source() logic for the simulate state: stays "simulated".
        assert_eq!(promote_view(&src, None), "simulated");
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

    /// Mirror of `AppStateInner::effective_source` over just (source, age) so the
    /// promotion/reversion logic is testable without constructing full state.
    fn promote_view(source: &str, last_frame_age: Option<std::time::Duration>) -> String {
        if source == "esp32" {
            if let Some(age) = last_frame_age {
                if age > ESP32_OFFLINE_TIMEOUT {
                    return "esp32:offline".to_string();
                }
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

// ── WebSocket handler ────────────────────────────────────────────────────────

async fn ws_sensing_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_client(socket, state))
}

async fn handle_ws_client(mut socket: WebSocket, state: SharedState) {
    let mut rx = {
        let s = state.read().await;
        s.tx.subscribe()
    };

    info!("WebSocket client connected (sensing)");

    // ADR-044/045: ping/pong keepalive to prevent proxy idle timeouts.
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    // Lagged: client fell behind — skip missed frames, don't disconnect.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("WS client lagged by {n} frames, skipping");
                        continue;
                    }
                    Err(_) => break, // channel closed
                }
            }
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {} // keepalive response
                    _ => {} // ignore other client messages
                }
            }
        }
    }

    info!("WebSocket client disconnected (sensing)");
}

// ── ADR-099: real-time CSI introspection — WS topic + REST snapshot ──────────
//
// Parallel to the window-aggregated `/ws/sensing` topic. Subscribers see a
// fresh `IntrospectionSnapshot` JSON frame on every accepted CSI frame
// (regime / Lyapunov exponent / top-k DTW similarity), no window-close delay.

async fn ws_introspection_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_introspection_client(socket, state))
}

async fn handle_ws_introspection_client(mut socket: WebSocket, state: SharedState) {
    let mut rx = {
        let s = state.read().await;
        s.intro_tx.subscribe()
    };

    info!("WebSocket client connected (introspection)");

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // ignore client messages
                }
            }
        }
    }

    info!("WebSocket client disconnected (introspection)");
}

/// `GET /api/v1/introspection/snapshot` — one-shot poll for the latest
/// per-frame snapshot (regime, Lyapunov, top-k similarity). Mirrors the shape
/// of `/api/v1/sensing/latest` for the dashboard one-shot path.
async fn api_introspection_snapshot(State(state): State<SharedState>) -> impl IntoResponse {
    let s = state.read().await;
    Json(s.intro.snapshot().clone())
}

// ── Pose WebSocket handler (sends pose_data messages for Live Demo) ──────────

async fn ws_pose_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_pose_client(socket, state))
}

fn pose_source_for_frame(model_loaded: bool, model_output_present: bool) -> &'static str {
    if model_loaded && model_output_present {
        "model_inference"
    } else {
        "signal_derived"
    }
}

#[cfg(test)]
mod torso_live_gating_tests {
    use super::pose_source_for_frame;

    #[test]
    fn loaded_model_without_output_never_claims_model_inference() {
        assert_eq!(pose_source_for_frame(true, false), "signal_derived");
        assert_eq!(pose_source_for_frame(false, false), "signal_derived");
        assert_eq!(pose_source_for_frame(false, true), "signal_derived");
        assert_eq!(pose_source_for_frame(true, true), "model_inference");
    }
}

async fn handle_ws_pose_client(mut socket: WebSocket, state: SharedState) {
    let mut rx = {
        let s = state.read().await;
        s.tx.subscribe()
    };

    info!("WebSocket client connected (pose)");

    // Send connection established message
    let conn_msg = serde_json::json!({
        "type": "connection_established",
        "payload": { "status": "connected", "backend": "rust+ruvector" }
    });
    let _ = socket.send(Message::Text(conn_msg.to_string())).await;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        // Parse the sensing update and convert to pose format
                        if let Ok(sensing) = serde_json::from_str::<SensingUpdate>(&json) {
                            if sensing.msg_type == "sensing_update" {
                                // Determine pose estimation mode for the UI indicator.
                                // "model_inference"    — this frame contains model-produced keypoints.
                                // "signal_derived"     — keypoints estimated from raw CSI features.
                                let model_loaded = {
                                    let s = state.read().await;
                                    s.model_loaded
                                };
                                let model_inference = model_loaded && sensing.pose_keypoints.is_some();
                                let pose_source = pose_source_for_frame(
                                    model_loaded,
                                    sensing.pose_keypoints.is_some(),
                                );

                                let persons = if model_inference {
                                    // When a trained model is loaded, prefer its keypoints if present.
                                    sensing.pose_keypoints.as_ref().map(|kps| {
                                        let kp_names = [
                                            "nose","left_eye","right_eye","left_ear","right_ear",
                                            "left_shoulder","right_shoulder","left_elbow","right_elbow",
                                            "left_wrist","right_wrist","left_hip","right_hip",
                                            "left_knee","right_knee","left_ankle","right_ankle",
                                        ];
                                        let keypoints: Vec<PoseKeypoint> = kps.iter()
                                            .enumerate()
                                            .map(|(i, kp)| PoseKeypoint {
                                                name: kp_names.get(i).unwrap_or(&"unknown").to_string(),
                                                x: kp[0], y: kp[1], z: kp[2], confidence: kp[3],
                                            })
                                            .collect();
                                        let [nx, _ny, nz] = sensing.signal_field.grid_size;
                                        let peak = field_localize::extract_peaks(
                                            &sensing.signal_field.values, nx, nz, 1, 3.0,
                                        ).into_iter().next();
                                        vec![PersonDetection {
                                            id: 1,
                                            confidence: sensing.classification.confidence,
                                            bbox: BoundingBox { x: 260.0, y: 150.0, width: 120.0, height: 220.0 },
                                            keypoints,
                                            zone: "zone_1".into(),
                                            position: peak.map_or([0.0, 0.0, 0.0], |p| p.position),
                                            motion_score: field_localize::motion_score_from_power(
                                                sensing.features.motion_band_power,
                                            ),
                                            pose: sensing.posture.clone(),
                                        }]
                                    }).unwrap_or_default()
                                } else {
                                    // Prefer tracked persons from broadcast if available
                                    sensing.persons.clone().unwrap_or_else(|| derive_pose_from_sensing(&sensing))
                                };

                                let pose_msg = serde_json::json!({
                                    "type": "pose_data",
                                    "zone_id": "zone_1",
                                    "timestamp": sensing.timestamp,
                                    "payload": {
                                        "pose": {
                                            "persons": persons,
                                        },
                                        "confidence": if sensing.classification.presence { sensing.classification.confidence } else { 0.0 },
                                        "activity": sensing.classification.motion_level,
                                        // pose_source tells the UI which estimation mode is active.
                                        "pose_source": pose_source,
                                        "metadata": {
                                            "frame_id": format!("rust_frame_{}", sensing.tick),
                                            "processing_time_ms": 1,
                                            "source": sensing.source,
                                            "tick": sensing.tick,
                                            "signal_strength": sensing.features.mean_rssi,
                                            "motion_band_power": sensing.features.motion_band_power,
                                            "breathing_band_power": sensing.features.breathing_band_power,
                                            "estimated_persons": persons.len(),
                                        }
                                    }
                                });
                                if socket.send(Message::Text(pose_msg.to_string())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    // Lagged: skip missed frames, don't disconnect.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("WS pose client lagged by {n} frames, skipping");
                        continue;
                    }
                    Err(_) => break, // channel closed
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle ping/pong
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            if v.get("type").and_then(|t| t.as_str()) == Some("ping") {
                                let pong = serde_json::json!({"type": "pong"});
                                let _ = socket.send(Message::Text(pong.to_string())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {} // keepalive response
                    _ => {}
                }
            }
        }
    }

    info!("WebSocket client disconnected (pose)");
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

async fn latest(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.latest_update {
        Some(update) => {
            let effective_source = s.effective_source();
            let public = public_sensing_update(update, &effective_source);
            Json(serde_json::to_value(public).unwrap_or_default())
        }
        None => Json(serde_json::json!({"status": "no data yet"})),
    }
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
    Json(serde_json::json!({
        "status": "healthy",
        "components": {
            "api": { "status": "healthy", "message": "Rust Axum server" },
            "hardware": {
                "status": if s.effective_source().ends_with(":offline") { "degraded" } else { "healthy" },
                "message": format!("Source: {}", s.effective_source())
            },
            "pose": { "status": "healthy", "message": "WiFi-derived pose estimation" },
            "stream": { "status": if s.tx.receiver_count() > 0 { "healthy" } else { "idle" },
                        "message": format!("{} client(s)", s.tx.receiver_count()) },
        },
        "metrics": {
            "cpu_percent": 2.5,
            "memory_percent": 1.8,
            "disk_percent": 15.0,
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
        "system_metrics": {
            "cpu": { "percent": 2.5 },
            "memory": { "percent": 1.8, "used_mb": 5 },
            "disk": { "percent": 15.0 },
        },
        "tick": s.tick,
    }))
}

async fn api_info(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "environment": "production",
        "backend": "rust",
        "source": s.effective_source(),
        "features": {
            "wifi_sensing": true,
            "pose_estimation": true,
            "signal_processing": true,
            "ruvector": true,
            "streaming": true,
        }
    }))
}

async fn pose_current(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let effective_source = s.effective_source();
    let persons = match &s.latest_update {
        Some(update) => {
            let public = public_sensing_update(update, &effective_source);
            public
                .persons
                .clone()
                .unwrap_or_else(|| derive_pose_from_sensing(&public))
        }
        None => vec![],
    };
    Json(serde_json::json!({
        "timestamp": chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        "persons": persons,
        "total_persons": persons.len(),
        "source": effective_source,
    }))
}

async fn pose_stats(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "total_detections": s.total_detections,
        "average_confidence": 0.87,
        "frames_processed": s.tick,
        "source": s.effective_source(),
    }))
}

async fn pose_zones_summary(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let effective_source = s.effective_source();
    let presence = s
        .latest_update
        .as_ref()
        .map(|update| {
            public_sensing_update(update, &effective_source)
                .classification
                .presence
        })
        .unwrap_or(false);
    Json(serde_json::json!({
        "zones": {
            "zone_1": { "person_count": if presence { 1 } else { 0 }, "status": "monitored" },
            "zone_2": { "person_count": 0, "status": "clear" },
            "zone_3": { "person_count": 0, "status": "clear" },
            "zone_4": { "person_count": 0, "status": "clear" },
        }
    }))
}

async fn stream_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "active": true,
        "clients": s.tx.receiver_count(),
        "fps": if s.tick > 1 { 10u64 } else { 0u64 },
        "source": s.effective_source(),
    }))
}

#[derive(Debug, Deserialize)]
struct CreateExperimentRequest {
    label: Option<String>,
    fixture_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupProfileRequest {
    label: String,
    document: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateWorkflowRequest {
    label: String,
    profile_id: String,
    #[serde(default = "default_dataset_version")]
    dataset_version: String,
    #[serde(default = "default_firmware_version")]
    firmware_version: String,
    #[serde(default)]
    blind_seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowPhaseRequest {
    phase: String,
    status: String,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowArtifactRequest {
    kind: String,
    relative_path: String,
}

fn default_dataset_version() -> String {
    "unassigned".to_string()
}

fn default_firmware_version() -> String {
    "unassigned".to_string()
}

fn experiment_api_error(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": code.into(),
            "message": message.into(),
        })),
    )
        .into_response()
}

async fn experiments_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return Json(serde_json::json!({
            "available": false,
            "status": "PERSISTENCE_UNAVAILABLE",
            "schema_version": experiment::SCHEMA_VERSION,
            "message": "SQLite persistence is unavailable; the Control Center is locked.",
            "control_center_locked": true,
        }));
    };

    match store.run_count().await {
        Ok(run_count) => Json(serde_json::json!({
            "available": true,
            "status": "READY",
            "schema_version": experiment::SCHEMA_VERSION,
            "database_path": store.db_path().display().to_string(),
            "run_count": run_count,
            "supported_fixture_ids": [experiment::SUPPORTED_FIXTURE_ID],
            "control_center_locked": false,
        })),
        Err(error) => Json(serde_json::json!({
            "available": false,
            "status": "PERSISTENCE_UNAVAILABLE",
            "schema_version": experiment::SCHEMA_VERSION,
            "message": error,
            "control_center_locked": true,
        })),
    }
}

async fn experiments_list(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; experiment runs cannot be listed.",
        );
    };

    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(50)
        .clamp(1, 100);
    match store.list_runs(limit).await {
        Ok(runs) => Json(serde_json::json!({ "runs": runs, "limit": limit })).into_response(),
        Err(error) => experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error),
    }
}

async fn experiments_create(
    State(state): State<SharedState>,
    Json(request): Json<CreateExperimentRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; the Control Center is locked.",
        );
    };

    let Some(label) = request.label else {
        return experiment_api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "label is required",
        );
    };
    let Some(fixture_id) = request.fixture_id else {
        return experiment_api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "fixture_id is required",
        );
    };

    match store.create_run(&label, &fixture_id).await {
        Ok(run) => (StatusCode::CREATED, Json(serde_json::json!(run))).into_response(),
        Err(error) if error.starts_with("unsupported fixture_id") => {
            experiment_api_error(StatusCode::BAD_REQUEST, "UNSUPPORTED_FIXTURE", error)
        }
        Err(error) if error.starts_with("label must") => {
            experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_LABEL", error)
        }
        Err(error) => experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_WRITE_FAILED", error),
    }
}

async fn experiment_get(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; the Control Center is locked.",
        );
    };

    match store.get_run(&id).await {
        Ok(Some(run)) => Json(serde_json::json!(run)).into_response(),
        Ok(None) => experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", "experiment run not found"),
        Err(error) => experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error),
    }
}

async fn experiment_replay(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; the Control Center is locked.",
        );
    };

    match store.get_run(&id).await {
        Ok(Some(run)) if run.state == "completed" => experiment_api_error(
            StatusCode::CONFLICT,
            "RUN_COMPLETED",
            "completed experiment runs cannot be replayed again",
        ),
        Ok(Some(run)) if run.state == "running" => experiment_api_error(
            StatusCode::CONFLICT,
            "RUN_RUNNING",
            "experiment run is already running",
        ),
        Ok(Some(_)) => match experiment::replay_run(&store, &id).await {
            Ok(run) => Json(serde_json::json!(run)).into_response(),
            Err(error) => experiment_api_error(StatusCode::INTERNAL_SERVER_ERROR, "REPLAY_FAILED", error),
        },
        Ok(None) => experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", "experiment run not found"),
        Err(error) => experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error),
    }
}

async fn setup_profiles_list(State(state): State<SharedState>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup profiles cannot be listed.",
        );
    };
    match store.list_profiles().await {
        Ok(profiles) => Json(serde_json::json!({
            "profiles": profiles,
            "schema_version": experiment::PROFILE_SCHEMA_VERSION
        }))
        .into_response(),
        Err(error) => experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error),
    }
}

async fn setup_profile_create(
    State(state): State<SharedState>,
    Json(request): Json<SetupProfileRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup profiles cannot be saved.",
        );
    };
    match store.create_profile(&request.label, &request.document).await {
        Ok(profile) => (StatusCode::CREATED, Json(serde_json::json!(profile))).into_response(),
        Err(error) => experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_PROFILE", error),
    }
}

async fn setup_profile_update(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<SetupProfileRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup profiles cannot be saved.",
        );
    };
    match store
        .update_profile(&id, &request.label, &request.document)
        .await
    {
        Ok(profile) => Json(serde_json::json!(profile)).into_response(),
        Err(error) if error == "setup profile not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "PROFILE_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_PROFILE", error),
    }
}

async fn workflow_create(
    State(state): State<SharedState>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; workflow runs cannot be created.",
        );
    };
    let seed = request.blind_seed.unwrap_or_else(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        now.as_nanos() as u64
    });
    match store
        .create_workflow_run(
            &request.label,
            &request.profile_id,
            &request.dataset_version,
            &request.firmware_version,
            seed,
        )
        .await
    {
        Ok(run) => (StatusCode::CREATED, Json(serde_json::json!(run))).into_response(),
        Err(error) if error == "setup profile not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "PROFILE_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_WORKFLOW", error),
    }
}

async fn workflow_advance(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<WorkflowPhaseRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; workflow phases cannot be recorded.",
        );
    };
    match store
        .advance_workflow(&id, &request.phase, &request.status, &request.payload)
        .await
    {
        Ok(run) => Json(serde_json::json!(run)).into_response(),
        Err(error) if error == "workflow run not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::CONFLICT, "INVALID_PHASE", error),
    }
}

async fn workflow_artifact_register(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<WorkflowArtifactRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; artifacts cannot be registered.",
        );
    };
    match store
        .register_workflow_artifact(&id, &request.kind, &request.relative_path)
        .await
    {
        Ok(run) => Json(serde_json::json!(run)).into_response(),
        Err(error) if error == "workflow run not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_ARTIFACT", error),
    }
}

async fn workflow_report(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; reports cannot be written.",
        );
    };
    match store.write_workflow_report(&id).await {
        Ok(run) => Json(serde_json::json!(run)).into_response(),
        Err(error) if error == "workflow run not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::CONFLICT, "REPORT_NOT_READY", error),
    }
}

async fn experiment_report(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; reports cannot be read.",
        );
    };
    match store.report_json(&id).await {
        Ok(Some(report)) => Json(report).into_response(),
        Ok(None) => experiment_api_error(StatusCode::NOT_FOUND, "REPORT_NOT_FOUND", "no report artifact exists for this run"),
        Err(error) => experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "REPORT_READ_FAILED", error),
    }
}

async fn experiment_export(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; exports cannot be generated.",
        );
    };
    let Some(run) = (match store.get_run(&id).await {
        Ok(run) => run,
        Err(error) => return experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error),
    }) else {
        return experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", "experiment run not found");
    };
    if params.get("format").is_some_and(|format| format == "csv") {
        let mut csv = String::from("event_id,phase,status,created_at,payload_json\n");
        if let Some(workflow) = &run.workflow {
            for event in &workflow.events {
                let payload = serde_json::to_string(&event.payload).unwrap_or_else(|_| "{}".to_string());
                csv.push_str(&format!(
                    "{},{},{},{},\"{}\"\n",
                    event.id,
                    event.phase,
                    event.status,
                    event.created_at,
                    payload.replace('"', "\"\"")
                ));
            }
        }
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            csv,
        )
            .into_response();
    }
    let report = store.report_json(&id).await.ok().flatten();
    Json(serde_json::json!({ "run": run, "report": report })).into_response()
}

async fn control_center_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let (nodes, recording, training, calibration, position_setup, active_model, intro, mmwave) = {
        let s = state.read().await;
        let now = std::time::Instant::now();
        let recording = s.recording_current_id.as_ref().map(|id| {
            serde_json::json!({
                "id": id,
                "phase": match s.recording_phase {
                    RecordingLifecyclePhase::Idle => "idle",
                    RecordingLifecyclePhase::Recording => "recording",
                    RecordingLifecyclePhase::Finalizing => "finalizing",
                }
            })
        });
        (
            public_node_summaries(
                &s.node_states,
                now,
                s.d5_presence.phase(),
                s.position_setup.is_some(),
            ),
            recording,
            serde_json::json!({
                "status": s.training_status,
                "config": s.training_config,
            }),
            serde_json::json!({
                "phase": s.d5_presence.phase().as_str(),
                "position_setup_active": s.position_setup.is_some(),
            }),
            s.position_setup.as_ref().map(|setup| serde_json::json!({
                "active": true,
                "setup_id": setup.setup_id(),
                "setup_sha256": setup.setup_sha256(),
                "room_dimensions_m": setup.room_dimensions_m(),
            })),
            s.active_model_id.clone(),
            serde_json::to_value(s.intro.snapshot()).unwrap_or_else(|_| serde_json::json!({})),
            serde_json::to_value(s.mmwave.status(server_clock::now().host_monotonic_ns))
                .unwrap_or_else(|_| serde_json::json!({})),
        )
    };
    Json(serde_json::json!({
        "nodes": nodes,
        "recording": recording,
        "training": training,
        "classification_calibration": calibration,
        "position_setup": position_setup,
        "active_model_id": active_model,
        "signal_diagnostics": intro,
        "mmwave": mmwave,
        "mmwave_control": "read_only_until_sensor_validation"
    }))
}

async fn benchmark_catalog() -> Json<serde_json::Value> {
    Json(benchmark::catalog())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MmwaveModeRequest {
    mode: mmwave_calibration::MeasurementMode,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MmwaveSessionStartRequest {
    kind: mmwave_calibration::SessionKind,
    #[serde(default)]
    policy: Option<mmwave_calibration::CalibrationPolicy>,
}

type MmwaveApiResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

fn mmwave_api_error(
    status: StatusCode,
    message: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": message.into() })))
}

async fn mmwave_status_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let (mut status, control) = {
        let state = state.read().await;
        (
            state.mmwave.status(server_clock::now().host_monotonic_ns),
            state.mmwave.control(),
        )
    };
    if let Some(control) = control {
        let diagnostics =
            tokio::task::spawn_blocking(move || mmwave_calibration::get_node_diagnostics(&control))
                .await
                .unwrap_or_else(|error| Err(format!("mmWave node status task failed: {error}")));
        status.attach_node_diagnostics(diagnostics);
    }
    Json(serde_json::to_value(status).expect("mmWave status is serializable"))
}

async fn mmwave_mode_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<MmwaveModeRequest>,
) -> MmwaveApiResult {
    let control = state.read().await.mmwave.control().ok_or_else(|| {
        mmwave_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mmWave node control is not configured",
        )
    })?;
    tokio::task::spawn_blocking(move || mmwave_calibration::set_node_mode(&control, request.mode))
        .await
        .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
    Ok(Json(serde_json::json!({ "mode": request.mode })))
}

async fn mmwave_transform_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<mmwave_calibration::TransformRequest>,
) -> MmwaveApiResult {
    let control = {
        let state = state.read().await;
        if !state.mmwave.transform_reconfiguration_allowed() {
            return Err(mmwave_api_error(
                StatusCode::CONFLICT,
                "the transform cannot change during a session or after setup sealing",
            ));
        }
        state.mmwave.control().ok_or_else(|| {
            mmwave_api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "mmWave node control is not configured",
            )
        })?
    };
    let response = request.clone();
    tokio::task::spawn_blocking(move || mmwave_calibration::set_node_transform(&control, &request))
        .await
        .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
    state.write().await.mmwave.clear_observed_transform();
    Ok(Json(serde_json::json!({ "transform": response })))
}

async fn mmwave_session_start_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<MmwaveSessionStartRequest>,
) -> MmwaveApiResult {
    let request_time = server_clock::now();
    let policy = request
        .policy
        .unwrap_or_default()
        .validate()
        .map_err(|error| mmwave_api_error(StatusCode::BAD_REQUEST, error))?;
    let (control, data_dir) = {
        let state = state.read().await;
        state
            .mmwave
            .validate_live_session_start(request.kind, request_time.host_monotonic_ns)
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
        let control = state.mmwave.control().ok_or_else(|| {
            mmwave_api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "automatic sessions require configured mmWave node control",
            )
        })?;
        (control, state.data_dir.clone())
    };
    let expected_mode = match request.kind {
        mmwave_calibration::SessionKind::Calibration => {
            mmwave_calibration::MeasurementMode::Calibration
        }
        mmwave_calibration::SessionKind::Blind => mmwave_calibration::MeasurementMode::Reference,
    };
    let diagnostics_control = control.clone();
    let diagnostics = tokio::task::spawn_blocking(move || {
        mmwave_calibration::get_node_diagnostics(&diagnostics_control)
    })
    .await
    .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
    if diagnostics.uart_bytes_received == 0
        || diagnostics.radar_frames_valid == 0
        || diagnostics.udp_packets_sent == 0
        || diagnostics.udp_send_failures != 0
    {
        return Err(mmwave_api_error(
            StatusCode::CONFLICT,
            "mmWave node diagnostics are not loss-free streaming",
        ));
    }
    tokio::task::spawn_blocking(move || mmwave_calibration::set_node_mode(&control, expected_mode))
        .await
        .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;

    let now = server_clock::now();
    let mut state = state.write().await;
    state
        .mmwave
        .start_session(request.kind, &data_dir, now.clone(), policy)
        .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    if request.kind == mmwave_calibration::SessionKind::Calibration {
        if let Err(error) = state
            .d5_presence
            .start_calibration(std::time::Instant::now())
        {
            let _ = state.mmwave.stop_session();
            return Err(mmwave_api_error(StatusCode::CONFLICT, error));
        }
        for node in state.node_states.values_mut() {
            node.d5_presence.reset_for_calibration();
            node.d6_fingerprint.reset_for_calibration();
            node.calibration_motion_rejected_frames = 0;
        }
    }
    let status = state.mmwave.status(now.host_monotonic_ns);
    Ok(Json(
        serde_json::to_value(status).expect("mmWave status is serializable"),
    ))
}

async fn mmwave_session_stop_endpoint(State(state): State<SharedState>) -> MmwaveApiResult {
    let mut state = state.write().await;
    state
        .mmwave
        .stop_session()
        .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    let status = state.mmwave.status(server_clock::now().host_monotonic_ns);
    Ok(Json(
        serde_json::to_value(status).expect("mmWave status is serializable"),
    ))
}

async fn mmwave_receiver_task(state: SharedState, port: u16) {
    let address = format!("0.0.0.0:{port}");
    let socket = match UdpSocket::bind(&address).await {
        Ok(socket) => socket,
        Err(error) => {
            error!("Could not bind mmWave UDP receiver to {address}: {error}");
            return;
        }
    };
    info!("mmWave UDP receiver listening on {address}");
    let mut buffer = [0_u8; 2048];
    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((length, source)) => {
                let now = server_clock::now();
                let mut state = state.write().await;
                let wifi_prediction = state.live_position_tracker.current().clone();
                state.mmwave.observe_wifi_prediction(&wifi_prediction);
                if let Err(reason) = state.mmwave.ingest_json(&buffer[..length], now) {
                    debug!("Rejected mmWave packet from {source}: {reason}");
                }
                if let Some((index_path, index_sha256)) = state.mmwave.take_pending_index() {
                    let setup_identity = state.position_setup.as_ref().map(|setup| {
                        (
                            setup.setup_id().to_string(),
                            setup.setup_sha256().to_string(),
                        )
                    });
                    if let Some((setup_id, setup_sha256)) = setup_identity {
                        match position_live::PositionIndexRuntime::load(
                            &index_path,
                            &setup_id,
                            &setup_sha256,
                            Some(&index_sha256),
                        ) {
                            Ok(runtime) => {
                                state.live_position_tracker.install_runtime(Some(runtime));
                                info!("Installed mmWave-gated position index {}", index_sha256);
                            }
                            Err(error) => error!(
                                "Generated mmWave position index failed runtime validation: {error}"
                            ),
                        }
                    }
                }
            }
            Err(error) => warn!("mmWave UDP receive failed: {error}"),
        }
    }
}

// ── Model Management Endpoints ──────────────────────────────────────────────

/// GET /api/v1/models — list discovered RVF model files.
async fn list_models(State(state): State<SharedState>) -> Json<serde_json::Value> {
    // Re-scan directory each call so newly-added files are visible.
    let models = scan_model_files();
    let total = models.len();
    {
        let mut s = state.write().await;
        s.discovered_models = models.clone();
    }
    Json(serde_json::json!({ "models": models, "total": total }))
}

/// GET /api/v1/models/active — return currently loaded model or null.
async fn get_active_model(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.active_model_id {
        Some(id) => {
            let model = s
                .discovered_models
                .iter()
                .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(id.as_str()));
            Json(serde_json::json!({
                "active": model.cloned().unwrap_or_else(|| serde_json::json!({ "id": id })),
            }))
        }
        None => Json(serde_json::json!({ "active": serde_json::Value::Null })),
    }
}

/// POST /api/v1/models/load — load a model by ID.
async fn load_model(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let model_id = body
        .get("id")
        .or_else(|| body.get("model_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if model_id.is_empty() {
        return Json(serde_json::json!({ "error": "missing 'id' field", "success": false }));
    }
    let mut s = state.write().await;
    s.active_model_id = Some(model_id.clone());
    s.model_loaded = true;
    info!("Model loaded: {model_id}");
    Json(serde_json::json!({ "success": true, "model_id": model_id }))
}

/// POST /api/v1/models/unload — unload the current model.
async fn unload_model(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    let prev = s.active_model_id.take();
    s.model_loaded = false;
    info!("Model unloaded (was: {:?})", prev);
    Json(serde_json::json!({ "success": true, "previous": prev }))
}

/// DELETE /api/v1/models/:id — delete a model file.
async fn delete_model(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    // ADR-166: Sanitize path to prevent directory traversal
    let safe_id = std::path::Path::new(&id)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    if safe_id.is_empty() || safe_id != id {
        return Json(serde_json::json!({ "error": "invalid model id", "success": false }));
    }
    let path = effective_models_dir().join(format!("{}.rvf", safe_id));
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            // ADR-080 #2: log the OS error (incl. path) server-side only; the
            // client gets a generic body + correlation id, no leaked path.
            return error_response::internal_error_json("model delete", e);
        }
        // If this was the active model, unload it
        let mut s = state.write().await;
        if s.active_model_id.as_deref() == Some(id.as_str()) {
            s.active_model_id = None;
            s.model_loaded = false;
        }
        s.discovered_models
            .retain(|m| m.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
        info!("Model deleted: {id}");
        Json(serde_json::json!({ "success": true, "deleted": id }))
    } else {
        Json(serde_json::json!({ "error": "model not found", "success": false }))
    }
}

/// GET /api/v1/models/lora/profiles — list LoRA adapter profiles.
async fn list_lora_profiles() -> Json<serde_json::Value> {
    // LoRA profiles are discovered from data/models/*.lora.json
    let profiles = scan_lora_profiles();
    Json(serde_json::json!({ "profiles": profiles }))
}

/// POST /api/v1/models/lora/activate — activate a LoRA adapter profile.
async fn activate_lora_profile(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let profile = body
        .get("profile")
        .or_else(|| body.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if profile.is_empty() {
        return Json(serde_json::json!({ "error": "missing 'profile' field", "success": false }));
    }
    info!("LoRA profile activated: {profile}");
    Json(serde_json::json!({ "success": true, "profile": profile }))
}

/// Return the effective models directory, respecting the `MODELS_DIR`
/// environment variable.  Defaults to `data/models`.
fn effective_models_dir() -> PathBuf {
    PathBuf::from(std::env::var("MODELS_DIR").unwrap_or_else(|_| "data/models".to_string()))
}

/// Scan the models directory for `.rvf` files and return metadata.
/// Respects the `MODELS_DIR` environment variable.
fn scan_model_files() -> Vec<serde_json::Value> {
    let dir = effective_models_dir();
    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rvf") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                models.push(serde_json::json!({
                    "id": name,
                    "name": name,
                    "path": path.display().to_string(),
                    "size_bytes": size,
                    "format": "rvf",
                    "modified_epoch": modified,
                }));
            }
        }
    }
    models
}

/// Scan the models directory for `.lora.json` LoRA profile files.
/// Respects the `MODELS_DIR` environment variable.
fn scan_lora_profiles() -> Vec<serde_json::Value> {
    let dir = effective_models_dir();
    let mut profiles = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".lora.json") {
                let profile_name = name.trim_end_matches(".lora.json").to_string();
                // Try to read the profile JSON
                let config = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                profiles.push(serde_json::json!({
                    "name": profile_name,
                    "path": path.display().to_string(),
                    "config": config,
                }));
            }
        }
    }
    profiles
}

// ── Recording Endpoints ─────────────────────────────────────────────────────

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

async fn finalize_active_recording(
    state: &SharedState,
) -> Result<(String, u64, RecordingWriterResult), String> {
    let lifecycle = {
        let s = state.read().await;
        s.recording_lifecycle.clone()
    };
    let lifecycle_guard = lifecycle.lock_owned().await;

    let (recording_id, duration_secs, stop_tx, done_rx) = {
        let mut s = state.write().await;
        if s.recording_phase != RecordingLifecyclePhase::Recording || !s.recording_active {
            return Err("no recording in progress".to_string());
        }

        let recording_id = s
            .recording_current_id
            .clone()
            .ok_or_else(|| "active recording has no recording ID".to_string())?;
        let duration_secs = s
            .recording_start_time
            .map(|started| started.elapsed().as_secs())
            .unwrap_or(0);

        // This flag is also the producer gate. Flipping it while holding the
        // same state lock as the UDP loop creates a precise last-frame boundary:
        // no new raw frame can be sent after this point.
        s.recording_active = false;
        s.recording_phase = RecordingLifecyclePhase::Finalizing;
        let stop_tx = s.recording_stop_tx.take();
        let done_rx = s.recording_done_rx.take();
        for recording in s.recordings.iter_mut() {
            if recording.get("id").and_then(|value| value.as_str()) == Some(recording_id.as_str()) {
                recording["status"] = serde_json::json!("finalizing");
            }
        }
        (recording_id, duration_secs, stop_tx, done_rx)
    };

    // The owned task is deliberately detached from the HTTP request future.
    // Dropping a JoinHandle does not cancel its task, so a disconnected client
    // cannot strand the recorder in `Finalizing` after the producer gate closed.
    let finalizer_state = state.clone();
    let finalizer = tokio::spawn(async move {
        let _lifecycle_guard = lifecycle_guard;

        if let Some(stop_tx) = stop_tx {
            let _ = stop_tx.send(true);
        }

        let mut result = match done_rx {
            Some(done_rx) => done_rx.await.unwrap_or_else(|_| RecordingWriterResult {
                error: Some("recording writer ended without a completion result".to_string()),
                ..Default::default()
            }),
            None => RecordingWriterResult {
                error: Some("recording writer completion channel is missing".to_string()),
                ..Default::default()
            },
        };

        if let Err(error) = finalize_recording_metadata(&recording_id, duration_secs, &result) {
            append_recording_writer_error(&mut result, error);
        }

        let mut s = finalizer_state.write().await;
        s.recording_current_id = None;
        s.recording_start_time = None;
        s.recording_phase = RecordingLifecyclePhase::Idle;
        for recording in s.recordings.iter_mut() {
            if recording.get("id").and_then(|value| value.as_str()) == Some(recording_id.as_str()) {
                recording["status"] = serde_json::json!(if result.incomplete() {
                    "incomplete"
                } else {
                    "completed"
                });
                recording["duration_secs"] = serde_json::json!(duration_secs);
                recording["frames"] = serde_json::json!(result.frames_written);
                recording["frame_count"] = serde_json::json!(result.frames_written);
                recording["rx_summaries"] =
                    serde_json::json!(result.rx_summaries.values().collect::<Vec<_>>());
                recording["dropped_frames"] = serde_json::json!(result.dropped_frames);
            }
        }
        drop(s);

        (recording_id, duration_secs, result)
    });

    finalizer
        .await
        .map_err(|error| format!("recording finalizer task failed: {error}"))
}

async fn settle_recording_on_shutdown(
    state: &SharedState,
) -> Result<Option<(String, u64, RecordingWriterResult)>, String> {
    loop {
        let (phase, lifecycle) = {
            let s = state.read().await;
            (s.recording_phase, s.recording_lifecycle.clone())
        };
        match phase {
            RecordingLifecyclePhase::Idle => return Ok(None),
            RecordingLifecyclePhase::Recording => {
                return finalize_active_recording(state).await.map(Some);
            }
            RecordingLifecyclePhase::Finalizing => {
                // The owned finalizer holds this mutex through metadata commit
                // and state cleanup. Acquiring it therefore waits for a
                // cancellation-detached finalization already in progress.
                let guard = lifecycle.lock().await;
                drop(guard);
                if state.read().await.recording_phase == RecordingLifecyclePhase::Finalizing {
                    return Err(
                        "recording finalizer ended without clearing finalizing state".to_string(),
                    );
                }
            }
        }
    }
}

/// GET /api/v1/recording/list — list CSI recordings.
async fn list_recordings(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let current = {
        let s = state.read().await;
        s.recording_current_id
            .clone()
            .map(|id| (id, s.recording_phase))
    };
    let mut recordings = scan_recording_files();
    if let Some((current_id, phase)) = current {
        let live_status = match phase {
            RecordingLifecyclePhase::Recording => "recording",
            RecordingLifecyclePhase::Finalizing => "finalizing",
            RecordingLifecyclePhase::Idle => "incomplete",
        };
        for recording in &mut recordings {
            if recording.get("id").and_then(serde_json::Value::as_str) == Some(current_id.as_str())
            {
                recording["status"] = serde_json::json!(live_status);
                recording["incomplete"] = serde_json::json!(phase == RecordingLifecyclePhase::Idle);
            }
        }
    }
    Json(serde_json::json!({ "recordings": recordings }))
}

/// POST /api/v1/recording/start — start recording CSI data.
async fn start_recording(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let lifecycle = {
        let s = state.read().await;
        s.recording_lifecycle.clone()
    };
    let _lifecycle_guard = lifecycle.lock().await;
    let mut s = state.write().await;
    if s.recording_phase != RecordingLifecyclePhase::Idle
        || s.recording_active
        || s.recording_current_id.is_some()
    {
        return Json(serde_json::json!({
            "error": "recording already in progress or finalizing",
            "success": false,
            "recording_id": s.recording_current_id,
        }));
    }
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("session_name").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("rec_{}", chrono_timestamp()));
    if let Err(error) = raw_csi_recording::validate_recording_id(&id) {
        return Json(serde_json::json!({
            "success": false,
            "error": error.to_string(),
        }));
    }
    let max_duration_seconds = match body.get("max_duration_seconds") {
        None => None,
        Some(value) => match value.as_u64() {
            Some(seconds @ 1..=3_600) => Some(seconds),
            _ => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "max_duration_seconds must be an integer from 1 to 3600",
                }));
            }
        },
    };
    let label = body
        .get("label")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let ground_truth = match body.get("ground_truth").cloned() {
        Some(value) => match serde_json::from_value::<raw_csi_recording::GroundTruth>(value) {
            Ok(ground_truth) => Some(ground_truth),
            Err(error) => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": format!("invalid ground_truth: {error}"),
                }));
            }
        },
        None => None,
    };
    let loaded_setup_identity = s
        .position_setup
        .as_deref()
        .map(|setup| (setup.setup_id(), setup.setup_sha256()));
    let (setup_id, setup_sha256) =
        match resolve_recording_setup_identity(&body, loaded_setup_identity) {
            Ok(identity) => identity,
            Err(error) => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": error,
                }));
            }
        };

    // Create the lossless server-local recording and an immutable setup
    // sidecar. Raw I/Q is never routed through the privacy-gated UI payload.
    let recordings_dir = PathBuf::from("data/recordings");
    if let Err(error) = std::fs::create_dir_all(&recordings_dir) {
        return error_response::internal_error_json("recording directory create", error);
    }
    let rec_path = match raw_csi_recording::recording_path(&recordings_dir, &id) {
        Ok(path) => path,
        Err(error) => {
            return Json(serde_json::json!({
                "success": false,
                "error": error.to_string(),
            }));
        }
    };
    let file = match create_private_recording_file(&rec_path) {
        Ok(f) => f,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Json(serde_json::json!({
                "success": false,
                "error": "recording ID already exists; choose a new ID",
                "recording_id": id,
            }));
        }
        Err(e) => {
            // ADR-080 #2: the OS error can carry the recordings path; log it
            // server-side only and return a generic body + correlation id.
            return error_response::internal_error_json("recording create", e);
        }
    };
    let metadata_path = recordings_dir.join(format!("{id}.raw-csi.v1.meta.json"));
    let started_at_unix_ns = match raw_csi_recording::now_unix_ns() {
        Ok(timestamp) => timestamp,
        Err(error) => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording start timestamp", error);
        }
    };
    let recording_rx_positions: Vec<[f64; 3]> = s
        .position_setup
        .as_deref()
        .map(|setup| setup.receiver_positions_m().into_iter().collect())
        .unwrap_or_else(|| {
            s.multistatic_fuser
                .node_positions()
                .iter()
                .map(|position| position.map(f64::from))
                .collect()
        });
    let metadata = serde_json::json!({
        "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
        "recording_id": id,
        "label": label.clone(),
        "ground_truth": ground_truth.clone(),
        "setup_id": setup_id,
        "setup_sha256": setup_sha256,
        "server_version": env!("CARGO_PKG_VERSION"),
        "started_at_unix_seconds": chrono_timestamp(),
        "started_at_unix_ns": started_at_unix_ns,
        "tx_position": s.tx_position,
        "rx_positions": recording_rx_positions,
        "room_dimensions": s.room_dimensions,
        "capture_scope": "validated_udp_csi_all_grids",
        "max_duration_seconds": max_duration_seconds,
        "status": "recording",
    });
    let metadata_bytes = match serde_json::to_vec_pretty(&metadata) {
        Ok(bytes) => bytes,
        Err(error) => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording metadata encode", error);
        }
    };
    let mut metadata_file = match create_private_recording_file(&metadata_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return Json(serde_json::json!({
                "success": false,
                "error": "recording metadata already exists; choose a new ID",
                "recording_id": id,
            }));
        }
        Err(error) => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording metadata create", error);
        }
    };
    {
        use std::io::Write;
        if let Err(error) = metadata_file
            .write_all(&metadata_bytes)
            .and_then(|_| metadata_file.flush())
            .and_then(|_| metadata_file.sync_all())
        {
            drop(metadata_file);
            drop(file);
            let _ = std::fs::remove_file(&metadata_path);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording metadata write", error);
        }
    }
    drop(metadata_file);

    // Create a stop signal channel
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    s.recording_phase = RecordingLifecyclePhase::Recording;
    s.recording_active = true;
    s.recording_start_time = Some(std::time::Instant::now());
    s.recording_current_id = Some(id.clone());
    s.recording_stop_tx = Some(stop_tx);
    s.recording_done_rx = Some(done_rx);

    // Subscribe to the private lossless CSI channel, not the derived public
    // sensing-update channel.
    let mut rx = s.raw_csi_tx.subscribe();
    let recording_position_setup = s.position_setup.clone();

    // Add initial recording entry
    s.recordings.push(serde_json::json!({
        "id": id,
        "path": rec_path.display().to_string(),
        "metadata_path": metadata_path.display().to_string(),
        "format": "raw-csi-v1-jsonl",
        "label": label.clone(),
        "status": "recording",
        "started_at": chrono_timestamp(),
        "frames": 0,
        "frame_count": 0,
        "setup_id": setup_id,
        "setup_sha256": setup_sha256,
        "rx_summaries": [],
    }));

    let rec_id = id.clone();
    let session_id = Some(id.clone());
    let writer_state = state.clone();
    let watchdog_state = state.clone();
    let watchdog_recording_id = id.clone();

    // Spawn writer task in background
    tokio::spawn(async move {
        use std::io::Write;
        let mut writer = std::io::BufWriter::new(file);
        let mut result = RecordingWriterResult::default();
        let mut stop_requested = false;

        while !stop_requested && result.error.is_none() {
            tokio::select! {
                received = rx.recv() => {
                    match received {
                        Ok(RawCsiIngress::Frame(raw_frame)) => {
                            if let Err(error) = append_raw_recording_frame(
                                &mut writer,
                                raw_frame,
                                recording_position_setup.as_deref(),
                                &session_id,
                                &label,
                                &ground_truth,
                                &mut result,
                            ) {
                                warn!("Recording {rec_id}: {error}");
                                result.error = Some(error);
                            }
                        }
                        Ok(RawCsiIngress::Rejected { rx_id, reason }) => {
                            mark_recording_rejected(&mut result, rx_id, &reason);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            result.dropped_frames =
                                result.dropped_frames.saturating_add(n);
                            warn!(
                                "Recording {rec_id}: lagged {n} frames; capture will be marked incomplete"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            result.error =
                                Some("raw CSI broadcast closed before stop".to_string());
                        }
                    }
                }
                changed = stop_rx.changed() => {
                    match changed {
                        Ok(()) if *stop_rx.borrow() => {
                            stop_requested = true;
                            info!(
                                "Recording {rec_id}: stop signal received ({} frames)",
                                result.frames_written
                            );
                        }
                        Ok(()) => {}
                        Err(_) => {
                            result.error =
                                Some("recording stop channel closed unexpectedly".to_string());
                        }
                    }
                }
            }
        }

        // The producer gate was closed under AppState's write lock before the
        // stop signal was sent. The queue is therefore finite and can be
        // drained to establish a durable, exact stop boundary.
        if stop_requested && result.error.is_none() {
            loop {
                match rx.try_recv() {
                    Ok(RawCsiIngress::Frame(raw_frame)) => {
                        if let Err(error) = append_raw_recording_frame(
                            &mut writer,
                            raw_frame,
                            recording_position_setup.as_deref(),
                            &session_id,
                            &label,
                            &ground_truth,
                            &mut result,
                        ) {
                            warn!("Recording {rec_id}: {error}");
                            result.error = Some(error);
                            break;
                        }
                    }
                    Ok(RawCsiIngress::Rejected { rx_id, reason }) => {
                        mark_recording_rejected(&mut result, rx_id, &reason);
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        result.dropped_frames = result.dropped_frames.saturating_add(n);
                        warn!(
                            "Recording {rec_id}: lagged {n} queued frames during drain; capture is incomplete"
                        );
                    }
                    Err(broadcast::error::TryRecvError::Empty)
                    | Err(broadcast::error::TryRecvError::Closed) => break,
                }
            }
        }

        match writer.flush() {
            Ok(()) => {
                if let Err(error) = writer.get_ref().sync_all() {
                    let sync_error = format!("raw recording sync error: {error}");
                    warn!("Recording {rec_id}: {sync_error}");
                    append_recording_writer_error(&mut result, sync_error);
                }
            }
            Err(error) => {
                let flush_error = format!("final flush error: {error}");
                warn!("Recording {rec_id}: {flush_error}");
                append_recording_writer_error(&mut result, flush_error);
            }
        }
        info!(
            "Recording {rec_id} finished: {} frames written, {} dropped, incomplete={}",
            result.frames_written,
            result.dropped_frames,
            result.incomplete()
        );
        let writer_failed = result.error.is_some();
        let _ = done_tx.send(result);
        if writer_failed {
            match finalize_active_recording(&writer_state).await {
                Ok((recording_id, _, result)) => {
                    warn!(
                        "Recording {recording_id} was stopped automatically after a writer error: {:?}",
                        result.error
                    );
                }
                Err(error) => {
                    // A concurrent explicit stop may have completed first.
                    debug!("Recording {rec_id} auto-finalization skipped: {error}");
                }
            }
        }
    });

    if let Some(max_duration_seconds) = max_duration_seconds {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(max_duration_seconds)).await;
            let should_finalize = {
                let state = watchdog_state.read().await;
                state.recording_phase == RecordingLifecyclePhase::Recording
                    && state.recording_current_id.as_deref() == Some(watchdog_recording_id.as_str())
            };
            if should_finalize {
                match finalize_active_recording(&watchdog_state).await {
                    Ok((recording_id, _, result)) => {
                        warn!(
                            "Recording {recording_id} reached its server-side maximum duration; \
                             finalized with incomplete={}",
                            result.incomplete()
                        );
                    }
                    Err(error) => {
                        debug!(
                            "Recording {watchdog_recording_id} watchdog finalization skipped: {error}"
                        );
                    }
                }
            }
        });
    }

    info!("Recording started: {id}");
    Json(serde_json::json!({
        "success": true,
        "recording_id": id,
        "max_duration_seconds": max_duration_seconds,
    }))
}

/// POST /api/v1/recording/stop — stop recording CSI data.
async fn stop_recording(State(state): State<SharedState>) -> Json<serde_json::Value> {
    match finalize_active_recording(&state).await {
        Ok((recording_id, duration_secs, result)) => {
            let incomplete = result.incomplete();
            if incomplete {
                warn!(
                    "Recording {recording_id} stopped incomplete: dropped={}, error={:?}",
                    result.dropped_frames, result.error
                );
            } else {
                info!("Recording stopped: {recording_id} ({duration_secs}s)");
            }
            Json(serde_json::json!({
                "success": !incomplete,
                "stopped": true,
                "recording_id": recording_id,
                "duration_secs": duration_secs,
                "frames_written": result.frames_written,
                "rx_summaries": result.rx_summaries.values().collect::<Vec<_>>(),
                "dropped_frames": result.dropped_frames,
                "incomplete": incomplete,
                "writer_error": result.error,
            }))
        }
        Err(error) => Json(serde_json::json!({
            "error": error,
            "success": false,
        })),
    }
}

/// DELETE /api/v1/recording/:id — delete a recording file.
async fn delete_recording(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    if let Err(error) = raw_csi_recording::validate_recording_id(&id) {
        return Json(serde_json::json!({
            "error": error.to_string(),
            "success": false
        }));
    }
    let lifecycle = {
        let s = state.read().await;
        s.recording_lifecycle.clone()
    };
    let _lifecycle_guard = lifecycle.lock().await;
    {
        let s = state.read().await;
        if s.recording_phase != RecordingLifecyclePhase::Idle
            && s.recording_current_id.as_deref() == Some(id.as_str())
        {
            return Json(serde_json::json!({
                "error": "active or finalizing recording cannot be deleted",
                "success": false,
                "recording_id": id,
            }));
        }
    }

    let recordings_dir = PathBuf::from("data/recordings");
    let raw_path = raw_csi_recording::recording_path(&recordings_dir, &id)
        .expect("recording ID was validated above");
    let data_paths = [
        raw_path,
        recordings_dir.join(format!("{id}.csi.jsonl")),
        recordings_dir.join(format!("{id}.jsonl")),
    ];
    let metadata_paths = [
        recordings_dir.join(format!("{id}.raw-csi.v1.meta.json")),
        recordings_dir.join(format!("{id}.csi.meta.json")),
        recordings_dir.join(format!("{id}.meta.json")),
    ];

    let mut removed_any = false;
    for path in data_paths.iter().chain(metadata_paths.iter()) {
        if !path.exists() {
            continue;
        }
        if let Err(error) = std::fs::remove_file(path) {
            // ADR-080 #2: log the OS error (incl. path) server-side only.
            return error_response::internal_error_json("recording delete", error);
        }
        removed_any = true;
    }

    if removed_any {
        let mut s = state.write().await;
        s.recordings
            .retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
        info!("Recording deleted: {id}");
        Json(serde_json::json!({ "success": true, "deleted": id }))
    } else {
        Json(serde_json::json!({ "error": "recording not found", "success": false }))
    }
}

/// Scan `data/recordings/` for lossless raw recordings and legacy derived
/// `.jsonl` files.
#[derive(Debug, PartialEq, Eq)]
struct ScannedRecordingIntegrity {
    status: String,
    incomplete: bool,
    dropped_frames: u64,
    metadata_valid: bool,
}

fn scanned_recording_integrity(
    is_raw: bool,
    recording_id: &str,
    metadata: Option<&serde_json::Value>,
) -> ScannedRecordingIntegrity {
    let raw_metadata_valid = !is_raw
        || metadata.is_some_and(|metadata| {
            metadata
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                == Some(u64::from(raw_csi_recording::RAW_CSI_SCHEMA_VERSION))
                && metadata
                    .get("recording_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(recording_id)
                && matches!(
                    metadata.get("status").and_then(serde_json::Value::as_str),
                    Some("recording" | "completed" | "incomplete")
                )
        });
    let stored_status = metadata
        .and_then(|metadata| metadata.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if is_raw { "incomplete" } else { "completed" });
    let dropped_frames = metadata
        .and_then(|metadata| metadata.get("dropped_frames"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let writer_error = metadata
        .and_then(|metadata| metadata.get("writer_error"))
        .is_some_and(|error| !error.is_null());
    let incomplete = !raw_metadata_valid
        || matches!(stored_status, "recording" | "incomplete")
        || metadata
            .and_then(|metadata| metadata.get("incomplete"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        || dropped_frames > 0
        || writer_error;

    ScannedRecordingIntegrity {
        status: if incomplete {
            "incomplete".to_string()
        } else {
            stored_status.to_string()
        },
        incomplete,
        dropped_frames,
        metadata_valid: raw_metadata_valid,
    }
}

fn scan_recording_files() -> Vec<serde_json::Value> {
    let dir = PathBuf::from("data/recordings");
    let mut recordings = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let (name, format, trainable, is_raw, metadata_path) = if let Some(name) =
                file_name.strip_suffix(raw_csi_recording::RAW_CSI_FILE_SUFFIX)
            {
                (
                    name.to_string(),
                    "raw-csi-v1-jsonl",
                    false,
                    true,
                    dir.join(format!("{name}.raw-csi.v1.meta.json")),
                )
            } else if let Some(name) = file_name.strip_suffix(".csi.jsonl") {
                (
                    name.to_string(),
                    "legacy-derived-jsonl",
                    true,
                    false,
                    dir.join(format!("{name}.csi.meta.json")),
                )
            } else if let Some(name) = file_name.strip_suffix(".jsonl") {
                (
                    name.to_string(),
                    "legacy-derived-jsonl",
                    true,
                    false,
                    dir.join(format!("{name}.meta.json")),
                )
            } else {
                continue;
            };
            {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                // Stream the count so a large raw capture is not read into RAM.
                let frame_count = std::fs::File::open(&path)
                    .map(|file| {
                        use std::io::BufRead;
                        std::io::BufReader::new(file).lines().count()
                    })
                    .unwrap_or(0);
                let metadata = std::fs::read(&metadata_path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                // Only the live-state overlay in list_recordings can confirm
                // that a writer really is active. Raw data without a valid
                // matching sidecar must fail closed after a crash or write
                // error; it is never silently promoted to "completed".
                let integrity = scanned_recording_integrity(is_raw, &name, metadata.as_ref());
                let empty_metadata = serde_json::json!({});
                let metadata = metadata.as_ref().unwrap_or(&empty_metadata);
                recordings.push(serde_json::json!({
                    "id": name,
                    "name": name,
                    "path": path.display().to_string(),
                    "format": format,
                    "trainable": trainable,
                    "size_bytes": size,
                    "file_size_bytes": size,
                    "frames": frame_count,
                    "frame_count": frame_count,
                    "frames_written": metadata.get("frames_written"),
                    "modified_epoch": modified,
                    "status": integrity.status,
                    "label": metadata.get("label"),
                    "started_at": metadata
                        .get("started_at")
                        .or_else(|| metadata.get("started_at_unix_seconds")),
                    "ended_at": metadata
                        .get("ended_at")
                        .or_else(|| metadata.get("ended_at_unix_seconds")),
                    "dropped_frames": integrity.dropped_frames,
                    "incomplete": integrity.incomplete,
                    "integrity_error": if integrity.metadata_valid {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!("missing_or_invalid_raw_metadata")
                    },
                    "capture_scope": metadata.get("capture_scope"),
                    "setup_id": metadata.get("setup_id"),
                    "setup_sha256": metadata.get("setup_sha256"),
                    "duration_secs": metadata.get("duration_secs"),
                    "rx_summaries": metadata.get("rx_summaries"),
                }));
            }
        }
    }
    recordings
}

#[cfg(test)]
mod raw_recording_lifecycle_tests {
    use super::*;
    use raw_csi_recording::{IqPair, RawCsiFrame, RAW_CSI_SCHEMA_VERSION};

    fn valid_metadata(recording_id: &str, status: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
            "recording_id": recording_id,
            "status": status,
            "incomplete": false,
            "dropped_frames": 0,
            "writer_error": null,
        })
    }

    fn live_raw_frame(timestamp: u64, sequence: u32) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: timestamp,
            host_monotonic_ns: Some(timestamp),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: None,
            label: None,
            ground_truth: None,
            rx_id: 1,
            antenna_count: 1,
            subcarrier_count: 64,
            center_frequency_mhz: 2437,
            sequence,
            rssi_dbm: -50,
            noise_floor_dbm: -90,
            ppdu_type: 0,
            flags: 0,
            mesh_timestamp_us: None,
            source_binding: None,
            iq_pairs: vec![IqPair { i: 20, q: 0 }; 64],
        }
    }

    #[test]
    fn raw_recording_without_valid_matching_sidecar_fails_closed() {
        let missing = scanned_recording_integrity(true, "capture-a", None);
        assert_eq!(missing.status, "incomplete");
        assert!(missing.incomplete);
        assert!(!missing.metadata_valid);

        let mismatched = valid_metadata("capture-b", "completed");
        let mismatched = scanned_recording_integrity(true, "capture-a", Some(&mismatched));
        assert_eq!(mismatched.status, "incomplete");
        assert!(mismatched.incomplete);
        assert!(!mismatched.metadata_valid);

        let malformed = serde_json::json!({"recording_id": "capture-a"});
        let malformed = scanned_recording_integrity(true, "capture-a", Some(&malformed));
        assert_eq!(malformed.status, "incomplete");
        assert!(malformed.incomplete);
        assert!(!malformed.metadata_valid);
    }

    #[test]
    fn raw_recording_only_reports_completed_with_clean_terminal_metadata() {
        let completed = valid_metadata("capture-a", "completed");
        let completed = scanned_recording_integrity(true, "capture-a", Some(&completed));
        assert_eq!(completed.status, "completed");
        assert!(!completed.incomplete);
        assert!(completed.metadata_valid);

        let mut dropped = valid_metadata("capture-a", "completed");
        dropped["dropped_frames"] = serde_json::json!(3);
        let dropped = scanned_recording_integrity(true, "capture-a", Some(&dropped));
        assert_eq!(dropped.status, "incomplete");
        assert!(dropped.incomplete);

        let stale = valid_metadata("capture-a", "recording");
        let stale = scanned_recording_integrity(true, "capture-a", Some(&stale));
        assert_eq!(stale.status, "incomplete");
        assert!(stale.incomplete);
    }

    #[test]
    fn legacy_recording_without_sidecar_keeps_legacy_completed_default() {
        let legacy = scanned_recording_integrity(false, "legacy", None);
        assert_eq!(legacy.status, "completed");
        assert!(!legacy.incomplete);
        assert!(legacy.metadata_valid);
    }

    #[test]
    fn loaded_setup_is_auto_bound_or_must_be_repeated_exactly() {
        let expected_id = "setup-0123456789abcdef";
        let expected_sha256 = "a".repeat(64);
        let loaded = Some((expected_id, expected_sha256.as_str()));

        assert_eq!(
            resolve_recording_setup_identity(&serde_json::json!({}), loaded).unwrap(),
            (Some(expected_id.to_string()), Some(expected_sha256.clone()))
        );
        assert_eq!(
            resolve_recording_setup_identity(
                &serde_json::json!({
                    "setup_id": expected_id,
                    "setup_sha256": expected_sha256,
                }),
                loaded,
            )
            .unwrap(),
            (Some(expected_id.to_string()), Some("a".repeat(64)))
        );

        assert!(resolve_recording_setup_identity(
            &serde_json::json!({
                "setup_id": expected_id,
                "setup_sha256": "b".repeat(64),
            }),
            loaded,
        )
        .is_err());
        assert!(resolve_recording_setup_identity(
            &serde_json::json!({"setup_id": expected_id}),
            loaded,
        )
        .is_err());
    }

    #[test]
    fn recording_cannot_claim_a_setup_when_server_loaded_none() {
        assert_eq!(
            resolve_recording_setup_identity(&serde_json::json!({}), None).unwrap(),
            (None, None)
        );
        assert!(resolve_recording_setup_identity(
            &serde_json::json!({
                "setup_id": "setup-0123456789abcdef",
                "setup_sha256": "a".repeat(64),
            }),
            None,
        )
        .is_err());
        assert!(
            resolve_recording_setup_identity(&serde_json::json!({"setup_id": null}), None,)
                .is_err()
        );
    }

    #[test]
    fn rejected_setup_bound_ingress_marks_active_recording_incomplete() {
        let mut result = RecordingWriterResult::default();

        mark_recording_rejected(&mut result, Some(3), "sealed position setup rejected frame");

        assert!(result.incomplete());
        assert_eq!(result.frames_written, 0);
        assert_eq!(result.dropped_frames, 0);
        assert_eq!(
            result.error.as_deref(),
            Some(
                "RX3 sent raw CSI rejected before recording: sealed position setup rejected frame"
            )
        );
    }

    #[test]
    fn live_raw_input_is_independent_of_recorder_and_grid_failure_starts_a_new_epoch() {
        let mut tracker = position_live::LivePositionTracker::new(None);
        route_raw_frame_to_live_position(
            &mut tracker,
            None,
            true,
            live_raw_frame(10_000_000_000, 1),
        )
        .unwrap();
        assert_eq!(
            tracker.buffered_frame_count(),
            1,
            "live tracking must ingest raw CSI with no active recorder"
        );

        assert!(route_raw_frame_to_live_position(
            &mut tracker,
            None,
            false,
            live_raw_frame(10_100_000_000, 2),
        )
        .is_err());
        assert_eq!(
            tracker.buffered_frame_count(),
            0,
            "an invalid grid must not leave pre-transition frames reusable"
        );
        assert_eq!(
            tracker.current(),
            &position_live::LivePositionState::Uncalibrated
        );
    }

    #[test]
    fn recording_tracks_each_rx_and_rejects_a_mid_capture_grid_change() {
        let unique = raw_csi_recording::now_unix_ns().expect("test timestamp");
        let path = std::env::temp_dir().join(format!(
            "ruview-raw-rx-summary-test-{}-{unique}.jsonl",
            std::process::id()
        ));
        let file = create_private_recording_file(&path).expect("create test recording");
        let mut writer = std::io::BufWriter::new(file);
        let mut result = RecordingWriterResult::default();
        let session_id = Some("capture-a".to_string());

        let first_rx1 = live_raw_frame(10_000_000_000, 1);
        append_raw_recording_frame(
            &mut writer,
            first_rx1,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect("write first RX1 frame");

        let mut first_rx2 = live_raw_frame(10_100_000_000, 1);
        first_rx2.rx_id = 2;
        append_raw_recording_frame(
            &mut writer,
            first_rx2,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect("write first RX2 frame");

        let mut second_rx1 = live_raw_frame(10_200_000_000, 2);
        second_rx1.flags = raw_csi_recording::TRANSIENT_SYNC_FLAG;
        append_raw_recording_frame(
            &mut writer,
            second_rx1,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect("transient sync flag must not change the RX grid");

        assert_eq!(result.frames_written, 3);
        assert_eq!(result.rx_summaries.len(), 2);
        assert_eq!(result.rx_summaries[&1].frames_written, 2);
        assert_eq!(result.rx_summaries[&2].frames_written, 1);

        let mut changed_grid = live_raw_frame(10_300_000_000, 3);
        changed_grid.subcarrier_count = 63;
        changed_grid.iq_pairs.truncate(63);
        let error = append_raw_recording_frame(
            &mut writer,
            changed_grid,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect_err("a real RX grid change must fail closed");
        assert!(error.contains("changed CSI grid"));
        assert_eq!(result.frames_written, 3);
        assert_eq!(result.rx_summaries[&1].frames_written, 2);

        drop(writer);
        std::fs::remove_file(path).expect("remove test recording");
    }

    #[test]
    fn final_metadata_replace_is_private_and_complete() {
        use std::io::Write;

        let unique = raw_csi_recording::now_unix_ns().expect("test timestamp");
        let dir = std::env::temp_dir().join(format!(
            "ruview-raw-metadata-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("create test directory");
        let recording_id = "capture-a";
        let metadata_path = dir.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        let mut metadata_file =
            create_private_recording_file(&metadata_path).expect("create initial metadata");
        let initial = valid_metadata(recording_id, "recording");
        metadata_file
            .write_all(&serde_json::to_vec_pretty(&initial).expect("serialize initial metadata"))
            .expect("write initial metadata");
        metadata_file.sync_all().expect("sync initial metadata");
        drop(metadata_file);

        let result = RecordingWriterResult {
            frames_written: 42,
            dropped_frames: 0,
            error: None,
            rx_summaries: BTreeMap::new(),
        };
        finalize_recording_metadata_in_dir(&dir, recording_id, 12, &result)
            .expect("atomically finalize metadata");

        let finalized: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&metadata_path).expect("read finalized metadata"),
        )
        .expect("decode finalized metadata");
        assert_eq!(finalized["status"], "completed");
        assert_eq!(finalized["frames_written"], 42);
        assert_eq!(finalized["duration_secs"], 12);
        assert_eq!(finalized["incomplete"], false);
        assert!(finalized["ended_at_unix_ns"].as_u64().is_some());
        assert!(
            std::fs::read_dir(&dir)
                .expect("list test directory")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp")),
            "atomic replace must not leave a temporary sidecar"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&metadata_path)
                .expect("metadata permissions")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        std::fs::remove_dir_all(&dir).expect("remove test directory");
    }

    #[test]
    fn zero_frame_recording_finalizes_as_incomplete() {
        use std::io::Write;

        let unique = raw_csi_recording::now_unix_ns().expect("test timestamp");
        let dir = std::env::temp_dir().join(format!(
            "ruview-zero-frame-metadata-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("create test directory");
        let recording_id = "zero-frame-capture";
        let metadata_path = dir.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        let mut metadata_file =
            create_private_recording_file(&metadata_path).expect("create initial metadata");
        metadata_file
            .write_all(
                &serde_json::to_vec_pretty(&valid_metadata(recording_id, "recording"))
                    .expect("serialize initial metadata"),
            )
            .expect("write initial metadata");
        metadata_file.sync_all().expect("sync initial metadata");
        drop(metadata_file);

        let result = RecordingWriterResult::default();
        finalize_recording_metadata_in_dir(&dir, recording_id, 0, &result)
            .expect("finalize zero-frame metadata");

        let finalized: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&metadata_path).expect("read finalized metadata"),
        )
        .expect("decode finalized metadata");
        assert_eq!(finalized["status"], "incomplete");
        assert_eq!(finalized["frames_written"], 0);
        assert_eq!(finalized["incomplete"], true);

        std::fs::remove_dir_all(&dir).expect("remove test directory");
    }
}

// ── Training Endpoints ──────────────────────────────────────────────────────

/// GET /api/v1/train/status — get training status.
async fn train_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "status": s.training_status,
        "config": s.training_config,
    }))
}

/// POST /api/v1/train/start — start a training run.
async fn train_start(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    if s.training_status == "running" {
        return Json(serde_json::json!({
            "error": "training already running",
            "success": false,
        }));
    }
    s.training_status = "running".to_string();
    s.training_config = Some(body.clone());
    info!("Training started with config: {}", body);
    Json(serde_json::json!({
        "success": true,
        "status": "running",
        "message": "Training pipeline started. Use GET /api/v1/train/status to monitor.",
    }))
}

/// POST /api/v1/train/stop — stop the current training run.
async fn train_stop(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    if s.training_status != "running" {
        return Json(serde_json::json!({
            "error": "no training in progress",
            "success": false,
        }));
    }
    s.training_status = "idle".to_string();
    info!("Training stopped");
    Json(serde_json::json!({
        "success": true,
        "status": "idle",
    }))
}

// ── Adaptive classifier endpoints ────────────────────────────────────────────

/// POST /api/v1/adaptive/train — train the adaptive classifier from recordings.
async fn adaptive_train(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let rec_dir = PathBuf::from("data/recordings");
    eprintln!("=== Adaptive Classifier Training ===");
    match adaptive_classifier::train_from_recordings(&rec_dir) {
        Ok(model) => {
            let accuracy = model.training_accuracy;
            let frames = model.trained_frames;
            let stats: Vec<_> = model
                .class_stats
                .iter()
                .map(|cs| {
                    serde_json::json!({
                        "class": cs.label,
                        "samples": cs.count,
                        "feature_means": cs.mean,
                    })
                })
                .collect();

            // Save to disk.
            if let Err(e) = model.save(&adaptive_classifier::model_path()) {
                warn!("Failed to save adaptive model: {e}");
            } else {
                info!(
                    "Adaptive model saved to {}",
                    adaptive_classifier::model_path().display()
                );
            }

            // Load into runtime state.
            let mut s = state.write().await;
            s.adaptive_model = Some(model);

            Json(serde_json::json!({
                "success": true,
                "trained_frames": frames,
                "accuracy": accuracy,
                "class_stats": stats,
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e,
        })),
    }
}

/// GET /api/v1/adaptive/status — check adaptive model status.
async fn adaptive_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.adaptive_model {
        Some(model) => Json(serde_json::json!({
            "loaded": true,
            "trained_frames": model.trained_frames,
            "accuracy": model.training_accuracy,
            "version": model.version,
            "classes": model.class_names,
            "class_stats": model.class_stats,
        })),
        None => Json(serde_json::json!({
            "loaded": false,
            "message": "No adaptive model. POST /api/v1/adaptive/train to train one.",
        })),
    }
}

/// POST /api/v1/adaptive/unload — unload the adaptive model (revert to thresholds).
async fn adaptive_unload(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    s.adaptive_model = None;
    Json(serde_json::json!({ "success": true, "message": "Adaptive model unloaded." }))
}

// ── D5 classification calibration (multi-RX still presence) ─────────────────

async fn classification_calibration_start(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    let now = std::time::Instant::now();
    if let Err(error) = s.d5_presence.start_calibration(now) {
        return Json(serde_json::json!({
            "success": false,
            "error": error,
        }));
    }

    for node in s.node_states.values_mut() {
        node.d5_presence.reset_for_calibration();
        node.d6_fingerprint.reset_for_calibration();
        node.calibration_motion_rejected_frames = 0;
    }

    Json(serde_json::json!({
        "success": true,
        "status": "collecting",
        "message": "D5/D6 empty-room calibration started. Keep the room empty and keep the final physical setup unchanged.",
        "recommended_seconds": d5_presence::RECOMMENDED_CALIBRATION_SECONDS,
        "minimum_complete_blocks": d5_presence::MIN_CALIBRATION_BLOCKS,
        "minimum_samples_per_block": d5_presence::MIN_CALIBRATION_SAMPLES_PER_BLOCK,
        "block_seconds": d5_presence::CALIBRATION_BLOCK.as_secs(),
    }))
}

async fn classification_calibration_stop(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    if s.d5_presence.phase() != d5_presence::CalibrationPhase::Collecting {
        return Json(serde_json::json!({
            "success": false,
            "error": "No D5/D6 classification calibration is collecting.",
        }));
    }
    let Some(started_at) = s.d5_presence.calibration_started_at() else {
        return Json(serde_json::json!({
            "success": false,
            "error": "D5/D6 calibration start time is missing.",
        }));
    };
    let now = std::time::Instant::now();

    let mut candidates: Vec<(
        u8,
        Result<d5_presence::PresenceReference, String>,
        Result<d6_fingerprint::FingerprintReference, String>,
    )> = s
        .node_states
        .iter()
        .map(|(&node_id, node)| {
            let d5_reference = if node.d5_presence.observation_ready(now) {
                node.d5_presence.build_reference(started_at, now)
            } else {
                Err(format!(
                    "accepted D5 calibration input is stale or below {:.1} Hz",
                    d5_presence::MIN_FRAME_RATE_HZ
                ))
            };
            let d6_reference = if node.d6_fingerprint.observation_ready(now) {
                node.d6_fingerprint.build_reference(started_at, now)
            } else {
                Err(format!(
                    "accepted D6 fingerprint input is stale or below {:.1} Hz",
                    d6_fingerprint::MIN_FRAME_RATE_HZ
                ))
            };
            (node_id, d5_reference, d6_reference)
        })
        .collect();
    candidates.sort_by_key(|(node_id, _, _)| *node_id);

    let ready_count = candidates
        .iter()
        .filter(|(_, _, reference)| reference.is_ok())
        .count();
    let node_results: Vec<serde_json::Value> = candidates
        .iter()
        .map(|(node_id, d5_result, d6_result)| {
            serde_json::json!({
                "node_id": node_id,
                "ready": d6_result.is_ok(),
                "d5_ready": d5_result.is_ok(),
                "d5_error": d5_result.as_ref().err(),
                "d6_ready": d6_result.is_ok(),
                "d6_error": d6_result.as_ref().err(),
            })
        })
        .collect();

    if ready_count < d5_presence::MIN_FRESH_REFERENCES {
        return Json(serde_json::json!({
            "success": false,
            "status": "collecting",
            "error": format!(
                "Only {ready_count} D6 RX fingerprints are usable; at least {} are required. Keep the room empty and continue collecting.",
                d5_presence::MIN_FRESH_REFERENCES
            ),
            "elapsed_seconds": now.saturating_duration_since(started_at).as_secs_f64(),
            "nodes": node_results,
        }));
    }

    for (node_id, d5_result, d6_result) in candidates {
        let node = s
            .node_states
            .get_mut(&node_id)
            .expect("candidate node must still exist while state is write-locked");
        match d5_result {
            Ok(reference) => node.d5_presence.install_reference(reference),
            Err(_) => node.d5_presence.invalidate_reference(),
        }
        match d6_result {
            Ok(reference) => node.d6_fingerprint.install_reference(reference),
            Err(_) => node.d6_fingerprint.invalidate_reference(),
        }
    }
    s.d5_presence.finish_calibration(now);

    Json(serde_json::json!({
        "success": true,
        "status": "ready",
        "message": "D5 diagnostics and D6 static CSI fingerprints installed. The first D6 live decision needs a complete 3-second window.",
        "elapsed_seconds": now.saturating_duration_since(started_at).as_secs_f64(),
        "ready_nodes": ready_count,
        "required_votes": d5_presence::REQUIRED_VOTES,
        "minimum_fresh_references": d5_presence::MIN_FRESH_REFERENCES,
        "minimum_frame_rate_hz": d5_presence::MIN_FRAME_RATE_HZ,
        "nodes": node_results,
    }))
}

async fn classification_calibration_status(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let s = state.read().await;
    let now = std::time::Instant::now();
    let phase = s.d5_presence.phase();
    let mut nodes: Vec<serde_json::Value> = s
        .node_states
        .iter()
        .map(|(&node_id, node)| {
            let last_seen_ms = node
                .last_frame_time
                .map(|seen| now.saturating_duration_since(seen).as_millis() as u64);
            let fresh = node
                .last_frame_time
                .is_some_and(|seen| now.saturating_duration_since(seen) <= ESP32_OFFLINE_TIMEOUT);
            let d5_snapshot = node.d5_presence.snapshot(now);
            let d6_snapshot = node.d6_fingerprint.snapshot(now);
            serde_json::json!({
                "node_id": node_id,
                "fresh": fresh,
                "last_seen_ms": last_seen_ms,
                "frame_rate_hz": if node.csi_fps_samples >= 5 {
                    node.csi_fps_ema
                } else {
                    0.0
                },
                "d5": d5_snapshot,
                "d6": d6_snapshot,
                "motion_rejected_frames": node.calibration_motion_rejected_frames,
            })
        })
        .collect();
    nodes.sort_by_key(|node| {
        node.get("node_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(u64::MAX)
    });

    let fresh_reference_nodes = s
        .node_states
        .values()
        .filter(|node| {
            node.d6_fingerprint.reference_ready() && node.d6_fingerprint.observation_fresh(now)
        })
        .count();
    let usable_live_nodes = s
        .node_states
        .values()
        .filter(|node| node.d6_fingerprint.evidence_ready(now))
        .count();
    let votes = s
        .node_states
        .values()
        .filter(|node| node.d6_fingerprint.evidence_ready(now) && node.d6_fingerprint.vote())
        .count();
    let decision_status =
        classification_decision_status(phase, s.position_setup.is_some(), usable_live_nodes);
    let operational = phase == d5_presence::CalibrationPhase::Ready
        && usable_live_nodes >= d5_presence::MIN_FRESH_REFERENCES;

    Json(serde_json::json!({
        "success": true,
        "phase": phase.as_str(),
        "decision_status": decision_status,
        "position_setup_active": s.position_setup.is_some(),
        "collecting_seconds": s
            .d5_presence
            .calibration_started_at()
            .map(|started| now.saturating_duration_since(started).as_secs_f64()),
        "calibrated_seconds_ago": s
            .d5_presence
            .calibrated_at()
            .map(|calibrated| now.saturating_duration_since(calibrated).as_secs_f64()),
        "present": operational && s.d5_presence.present(),
        "detector": "d6_static_csi_fingerprint",
        "fresh_reference_nodes": fresh_reference_nodes,
        "usable_live_nodes": usable_live_nodes,
        "votes": votes,
        "required_votes": d5_presence::REQUIRED_VOTES,
        "minimum_fresh_references": d5_presence::MIN_FRESH_REFERENCES,
        "minimum_frame_rate_hz": d5_presence::MIN_FRAME_RATE_HZ,
        "operational": operational,
        "nodes": nodes,
    }))
}

fn classification_decision_status(
    phase: d5_presence::CalibrationPhase,
    position_setup_active: bool,
    usable_live_nodes: usize,
) -> &'static str {
    match phase {
        d5_presence::CalibrationPhase::Uncalibrated if position_setup_active => "uncalibrated",
        d5_presence::CalibrationPhase::Uncalibrated => "legacy_d4",
        d5_presence::CalibrationPhase::Collecting => "calibrating",
        d5_presence::CalibrationPhase::Ready
            if usable_live_nodes >= d5_presence::MIN_FRESH_REFERENCES =>
        {
            "operational"
        }
        d5_presence::CalibrationPhase::Ready => "degraded_unknown",
    }
}

// ── Field model calibration endpoints (eigenvalue person counting) ──────────

async fn calibration_start(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    // Guard: don't discard an in-progress or fresh calibration
    if let Some(ref fm) = s.field_model {
        match fm.status() {
            CalibrationStatus::Collecting => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "Calibration already in progress. Call /calibration/stop first.",
                    "frame_count": fm.calibration_frame_count(),
                }));
            }
            CalibrationStatus::Fresh => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "A fresh calibration already exists. Call /calibration/stop or wait for expiry.",
                }));
            }
            _ => {} // Stale/Expired/Uncalibrated — ok to recalibrate
        }
    }
    match FieldModel::new(field_bridge::single_link_config()) {
        Ok(fm) => {
            s.field_model = Some(fm);
            Json(serde_json::json!({
                "success": true,
                "message": "Calibration started — keep room empty while frames accumulate.",
            }))
        }
        // ADR-080 #2: FieldModel init error chain stays server-side only.
        Err(e) => error_response::internal_error_json("calibration start", e),
    }
}

async fn calibration_stop(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    if let Some(ref mut fm) = s.field_model {
        let ts = chrono::Utc::now().timestamp_micros() as u64;
        match fm.finalize_calibration(ts, 0) {
            Ok(modes) => {
                let baseline = modes.baseline_eigenvalue_count;
                let variance_explained = modes.variance_explained;
                info!("Field model calibrated: baseline_eigenvalues={baseline}, variance_explained={variance_explained:.2}");
                Json(serde_json::json!({
                    "success": true,
                    "baseline_eigenvalue_count": baseline,
                    "variance_explained": variance_explained,
                    "frame_count": fm.calibration_frame_count(),
                }))
            }
            // ADR-080 #2: finalize error chain stays server-side only.
            Err(e) => error_response::internal_error_json("calibration stop", e),
        }
    } else {
        Json(serde_json::json!({
            "success": false,
            "error": "No field model active — call /calibration/start first.",
        }))
    }
}

async fn calibration_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match s.field_model.as_ref() {
        Some(fm) => Json(serde_json::json!({
            "active": true,
            "status": format!("{:?}", fm.status()),
            "frame_count": fm.calibration_frame_count(),
        })),
        None => Json(serde_json::json!({
            "active": false,
            "status": "none",
        })),
    }
}

/// Generate a simple timestamp string (epoch seconds) for recording IDs.
fn chrono_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn vital_signs_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let vs = &s.latest_vitals;
    let (br_len, br_cap, hb_len, hb_cap) = s.vital_detector.buffer_status();
    Json(serde_json::json!({
        "vital_signs": {
            "breathing_rate_bpm": vs.breathing_rate_bpm,
            "heart_rate_bpm": vs.heart_rate_bpm,
            "breathing_confidence": vs.breathing_confidence,
            "heartbeat_confidence": vs.heartbeat_confidence,
            "signal_quality": vs.signal_quality,
        },
        "buffer_status": {
            "breathing_samples": br_len,
            "breathing_capacity": br_cap,
            "heartbeat_samples": hb_len,
            "heartbeat_capacity": hb_cap,
        },
        "source": s.effective_source(),
        "tick": s.tick,
    }))
}

/// Query params for `GET /api/v1/edge/registry`.
#[derive(Debug, Deserialize)]
struct EdgeRegistryParams {
    /// `?refresh=1` bypasses the in-process cache. Logged at debug for
    /// abuse visibility. ADR-102 §"Cache semantics".
    #[serde(default)]
    refresh: Option<String>,
}

/// GET /api/v1/edge/registry — surfaces the canonical Cognitum cog catalog.
///
/// See ADR-102 (`docs/adr/ADR-102-edge-module-registry.md`) for the design
/// + trust model + security review.
async fn edge_registry_endpoint(
    Extension(reg): Extension<
        Option<Arc<wifi_densepose_sensing_server::edge_registry::EdgeRegistry>>,
    >,
    Query(params): Query<EdgeRegistryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let Some(reg) = reg else {
        // --no-edge-registry, or upstream URL empty.
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "edge_registry_disabled",
                "detail": "This sensing-server was started with --no-edge-registry."
            })),
        ));
    };
    let force_refresh = matches!(params.refresh.as_deref(), Some("1") | Some("true"));
    if force_refresh {
        tracing::debug!(
            event = "edge_registry.refresh_requested",
            "?refresh=1 bypassed the cache; verify this isn't being abused"
        );
    }
    match tokio::task::spawn_blocking(move || reg.get(force_refresh)).await {
        Ok(Ok(resp)) => Ok(Json(
            serde_json::to_value(resp).unwrap_or(serde_json::json!({})),
        )),
        // ADR-080 #2: the upstream error can carry an internal URL/connection
        // detail — log it server-side only and return a generic 503.
        Ok(Err(err)) => Err(error_response::upstream_unavailable("edge_registry", err)),
        // ADR-080 #2: a panicked spawn_blocking surfaces "task … panicked" via
        // JoinError::Display — never ship that to the client. Generic 500 +
        // correlation id; the panic detail is logged server-side.
        Err(join_err) => Err(error_response::internal_error("edge_registry", join_err)),
    }
}

/// GET /api/v1/edge-vitals — latest edge vitals from ESP32 (ADR-039).
async fn edge_vitals_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.edge_vitals {
        Some(vitals) => {
            let classification = s
                .latest_update
                .as_ref()
                .filter(|update| update.source == "esp32")
                .map(|update| public_sensing_update(update, &s.effective_source()).classification)
                .unwrap_or_else(|| {
                    apply_position_setup_classification_gate(
                        s.position_setup.is_some(),
                        s.d5_presence.phase(),
                        edge_vitals_classification(vitals),
                    )
                });
            let public_vitals = public_edge_vitals_packet(vitals, &classification);
            Json(serde_json::json!({
                "status": "ok",
                "edge_vitals": public_vitals,
            }))
        }
        None => Json(serde_json::json!({
            "status": "no_data",
            "edge_vitals": null,
            "message": "No edge vitals packet received yet. Ensure ESP32 edge_tier >= 1.",
        })),
    }
}

/// GET /api/v1/wasm-events — latest WASM events from ESP32 (ADR-040).
async fn wasm_events_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.latest_wasm_events {
        Some(w) => Json(serde_json::json!({
            "status": "ok",
            "wasm_events": w,
        })),
        None => Json(serde_json::json!({
            "status": "no_data",
            "wasm_events": null,
            "message": "No WASM output packet received yet. Upload and start a .wasm module on the ESP32.",
        })),
    }
}

async fn model_info(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.rvf_info {
        Some(info) => Json(serde_json::json!({
            "status": "loaded",
            "container": info,
        })),
        None => Json(serde_json::json!({
            "status": "no_model",
            "message": "No RVF container loaded. Use --load-rvf <path> to load one.",
        })),
    }
}

async fn model_layers(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.progressive_loader {
        Some(loader) => {
            let (a, b, c) = loader.layer_status();
            Json(serde_json::json!({
                "layer_a": a,
                "layer_b": b,
                "layer_c": c,
                "progress": loader.loading_progress(),
            }))
        }
        None => Json(serde_json::json!({
            "layer_a": false,
            "layer_b": false,
            "layer_c": false,
            "progress": 0.0,
            "message": "No model loaded with progressive loading",
        })),
    }
}

async fn model_segments(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.progressive_loader {
        Some(loader) => Json(serde_json::json!({ "segments": loader.segment_list() })),
        None => Json(serde_json::json!({ "segments": [] })),
    }
}

async fn sona_profiles(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let names = s
        .progressive_loader
        .as_ref()
        .map(|l| l.sona_profile_names())
        .unwrap_or_default();
    let active = s.active_sona_profile.clone().unwrap_or_default();
    Json(serde_json::json!({ "profiles": names, "active": active }))
}

async fn sona_activate(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let profile = body
        .get("profile")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();

    let mut s = state.write().await;
    let available = s
        .progressive_loader
        .as_ref()
        .map(|l| l.sona_profile_names())
        .unwrap_or_default();

    if available.contains(&profile) {
        s.active_sona_profile = Some(profile.clone());
        Json(serde_json::json!({ "status": "activated", "profile": profile }))
    } else {
        Json(serde_json::json!({
            "status": "error",
            "message": format!("Profile '{}' not found. Available: {:?}", profile, available),
        }))
    }
}

/// GET /api/v1/nodes — per-node health and feature info.
/// ADR-110 iter 29 — per-node mesh sync snapshot via HTTP.
///
/// GET /api/v1/nodes/:id/sync
///   200 → Json(NodeSyncSnapshot) when latest_sync is present
///   404 → {"error": "no_sync", "node_id": N} otherwise
///
/// Complements the WebSocket `sync` field (iter 23) for clients that
/// can't hold a streaming connection (curl scripts, Home Assistant REST
/// sensors, automation rule probes).
async fn node_sync_endpoint(
    State(state): State<SharedState>,
    Path(id): Path<u8>,
) -> Result<Json<NodeSyncSnapshot>, (StatusCode, Json<serde_json::Value>)> {
    let s = state.read().await;
    let ns = s.node_states.get(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "unknown_node", "node_id": id,
            })),
        )
    })?;
    ns.sync_snapshot().map(Json).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no_sync", "node_id": id,
                "hint": "node hasn't emitted a sync packet yet (no mesh peer or not v0.6.9+)",
            })),
        )
    })
}

/// ADR-110 iter 29 — fleet-wide mesh state via HTTP.
///
/// GET /api/v1/mesh
///   200 → { "nodes": { "<id>": NodeSyncSnapshot, ... }, "total": N }
///   Nodes without a recent sync are omitted from the map; an empty
///   `nodes` object means no mesh peers reachable.
/// ADR-110 iter 36 — Prometheus exposition format for mesh state.
///
/// GET /api/v1/mesh/metrics → text/plain
///   wifi_densepose_mesh_offset_us{node="N"} <signed-int>
///   wifi_densepose_mesh_is_leader{node="N"} 0|1
///   wifi_densepose_mesh_is_valid{node="N"} 0|1
///   wifi_densepose_mesh_smoothed{node="N"} 0|1
///   wifi_densepose_mesh_sequence{node="N"} <u32>
///   wifi_densepose_mesh_csi_fps{node="N"} <float>
///   wifi_densepose_mesh_csi_fps_samples{node="N"} <u32>
///   wifi_densepose_mesh_staleness_ms{node="N"} <u64>
///
/// Spec: <https://prometheus.io/docs/instrumenting/exposition_formats/>.
/// Each metric is a gauge labeled by node_id. Nodes without a fresh sync
/// are simply absent from the output (Prometheus handles missing series
/// natively — the scrape just reports them as stale after the configured
/// staleness duration).
async fn mesh_metrics_endpoint(State(state): State<SharedState>) -> impl IntoResponse {
    use std::fmt::Write;
    let s = state.read().await;
    let mut body = String::with_capacity(1024);

    // Each metric: HELP + TYPE header + one line per node that has a snapshot.
    let metrics: &[(&str, &str, &str)] = &[
        (
            "wifi_densepose_mesh_offset_us",
            "Cross-board mesh-aligned offset, microseconds (signed)",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_is_leader",
            "1 if this node is the elected mesh leader, else 0",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_is_valid",
            "1 if this node has heard a fresh leader beacon, else 0",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_smoothed",
            "1 once the firmware-side EMA filter has seeded, else 0",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_sequence",
            "High-water CSI sequence at sync emit time",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_csi_fps",
            "Per-node measured CSI frame rate (Hz)",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_csi_fps_samples",
            "How many inter-frame deltas the fps EMA has seen",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_staleness_ms",
            "Milliseconds since the host last received this node's sync packet",
            "gauge",
        ),
    ];

    // Collect (id, snapshot) pairs once so each metric loop reads the same set.
    let snaps: Vec<(u8, NodeSyncSnapshot)> = s
        .node_states
        .iter()
        .filter_map(|(&id, ns)| ns.sync_snapshot().map(|snap| (id, snap)))
        .collect();

    // Iter 37: fleet cardinality summary — Ops dashboards want the
    // "how many leaders / followers / no-sync" tally at a glance
    // without scraping every per-node series and counting.
    let (leaders, followers) = fleet_role_counts(&snaps);
    let no_sync = s.node_states.len().saturating_sub(snaps.len()) as u64;
    let _ = writeln!(
        body,
        "# HELP wifi_densepose_mesh_node_total Per-state node count across the fleet"
    );
    let _ = writeln!(body, "# TYPE wifi_densepose_mesh_node_total gauge");
    let _ = writeln!(
        body,
        "wifi_densepose_mesh_node_total{{state=\"leader\"}} {leaders}"
    );
    let _ = writeln!(
        body,
        "wifi_densepose_mesh_node_total{{state=\"follower\"}} {followers}"
    );
    let _ = writeln!(
        body,
        "wifi_densepose_mesh_node_total{{state=\"no_sync\"}} {no_sync}"
    );

    for (name, help, kind) in metrics {
        let _ = writeln!(body, "# HELP {name} {help}");
        let _ = writeln!(body, "# TYPE {name} {kind}");
        for (id, snap) in &snaps {
            let value = match *name {
                "wifi_densepose_mesh_offset_us" => snap.offset_us.to_string(),
                "wifi_densepose_mesh_is_leader" => bool_metric(snap.is_leader),
                "wifi_densepose_mesh_is_valid" => bool_metric(snap.is_valid),
                "wifi_densepose_mesh_smoothed" => bool_metric(snap.smoothed),
                "wifi_densepose_mesh_sequence" => snap.sequence.to_string(),
                "wifi_densepose_mesh_csi_fps" => format!("{:.3}", snap.csi_fps_ema),
                "wifi_densepose_mesh_csi_fps_samples" => snap.csi_fps_samples.to_string(),
                "wifi_densepose_mesh_staleness_ms" => snap
                    .staleness_ms
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "0".into()),
                _ => continue,
            };
            let _ = writeln!(body, "{name}{{node=\"{id}\"}} {value}");
        }
    }
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

fn bool_metric(b: bool) -> String {
    (if b { 1 } else { 0 }).to_string()
}

/// ADR-110 iter 37 — count (leaders, followers) in a populated snapshot set.
/// Free function for testability — same pattern as iter 18's `update_csi_fps_ema`.
pub(crate) fn fleet_role_counts(snaps: &[(u8, NodeSyncSnapshot)]) -> (u64, u64) {
    let leaders = snaps.iter().filter(|(_, s)| s.is_leader).count() as u64;
    let followers = (snaps.len() as u64).saturating_sub(leaders);
    (leaders, followers)
}

async fn mesh_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let mut nodes = serde_json::Map::new();
    for (&id, ns) in s.node_states.iter() {
        if let Some(snap) = ns.sync_snapshot() {
            nodes.insert(id.to_string(), serde_json::to_value(snap).unwrap());
        }
    }
    let total = nodes.len();
    Json(serde_json::json!({
        "nodes": serde_json::Value::Object(nodes),
        "total": total,
    }))
}

fn source_binding_consistent_across_nodes(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
) -> bool {
    const EXPECTED_RX_IDS: [u8; 4] = [1, 2, 3, 4];
    let mut active: Vec<(u8, &NodeState)> = node_states
        .iter()
        .filter(|(_, node)| {
            node.last_frame_time
                .is_some_and(|seen| now.saturating_duration_since(seen) <= ESP32_OFFLINE_TIMEOUT)
        })
        .map(|(&id, node)| (id, node))
        .collect();
    active.sort_by_key(|(id, _)| *id);
    if active.iter().map(|(id, _)| *id).collect::<Vec<_>>() != EXPECTED_RX_IDS {
        return false;
    }

    let Some(expected_identity) = active.first().and_then(|(_, node)| {
        node.source_binding_observation
            .as_ref()
            .filter(|binding| binding.complete && binding.is_fresh(now))
            .map(|binding| binding.tx_filter_identity.as_str())
    }) else {
        return false;
    };
    active.iter().all(|(_, node)| {
        node.source_binding_observation
            .as_ref()
            .is_some_and(|binding| {
                binding.complete
                    && binding.is_fresh(now)
                    && binding.tx_filter_identity == expected_identity
            })
    })
}

async fn nodes_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let now = std::time::Instant::now();
    let nodes = public_node_summaries(
        &s.node_states,
        now,
        s.d5_presence.phase(),
        s.position_setup.is_some(),
    );
    Json(serde_json::json!({
        "nodes": nodes,
        "total": nodes.len(),
        "source_binding_consistent_across_nodes": source_binding_consistent_across_nodes(
            &s.node_states,
            now,
        ),
    }))
}

fn public_node_summaries(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
    phase: d5_presence::CalibrationPhase,
    position_setup_active: bool,
) -> Vec<serde_json::Value> {
    let classifications: HashMap<u8, ClassificationInfo> =
        build_node_features(node_states, now, phase, position_setup_active)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| (entry.node_id, entry.classification))
            .collect();
    let mut nodes: Vec<serde_json::Value> = node_states
        .iter()
        .map(|(&id, ns)| {
            let elapsed_ms = ns
                .last_frame_time
                .map(|t| now.saturating_duration_since(t).as_millis() as u64)
                .unwrap_or(999999);
            let stale = elapsed_ms > 5000;
            let status = if stale { "stale" } else { "active" };
            let binding_last_seen_ms = ns.source_binding_observation.as_ref().map(|binding| {
                now.saturating_duration_since(binding.observed_at)
                    .as_millis() as u64
            });
            // Source attestation is independent of the selected CSI grid. A
            // complete identity-valid off-grid frame refreshes this evidence
            // but is still excluded from every sensing and recording path.
            // These booleans expose no configured MAC or private digest.
            let source_binding_attested = ns
                .source_binding_observation
                .as_ref()
                .is_some_and(|binding| binding.complete && binding.is_fresh(now));
            let identity_matches_setup = source_binding_attested
                && position_setup_active
                && ns
                    .source_binding_observation
                    .as_ref()
                    .is_some_and(|binding| binding.matches_setup);
            let rssi = ns.rssi_history.back().copied().unwrap_or(-90.0);
            let classification = classifications.get(&id);
            let motion_level = classification
                .map(|classification| classification.motion_level.as_str())
                .unwrap_or("unknown");
            let person_count =
                if classification.is_some_and(|classification| classification.presence) {
                    ns.prev_person_count
                } else {
                    0
                };
            let sequence_total = ns
                .sequence_observations
                .saturating_add(ns.inferred_lost_frames);
            serde_json::json!({
                "node_id": id,
                "display_name": format!("RX{id}"),
                "role": "receiver",
                "status": status,
                "last_seen_ms": elapsed_ms,
                "rssi_dbm": rssi,
                "frame_rate_hz": (ns.csi_fps_samples >= 5).then_some(ns.csi_fps_ema),
                "frame_rate_samples": ns.csi_fps_samples,
                "latest_sequence": ns.latest_sequence,
                "inferred_lost_frames": ns.inferred_lost_frames,
                "sequence_observations": ns.sequence_observations,
                "packet_loss_percent": (sequence_total > 0).then_some(
                    (ns.inferred_lost_frames as f64 / sequence_total as f64) * 100.0,
                ),
                "sync": ns.sync_snapshot(),
                "motion_level": motion_level,
                "person_count": person_count,
                "source_binding_attested": source_binding_attested,
                "filter_enforced": source_binding_attested,
                "source_matched_filter": source_binding_attested,
                "identity_valid": source_binding_attested,
                "identity_matches_setup": identity_matches_setup,
                "binding_last_seen_ms": binding_last_seen_ms,
                "skipped_grid_frames": ns.skipped_grid_frames,
            })
        })
        .collect();
    nodes.sort_by_key(|node| {
        node.get("node_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(u64::MAX)
    });
    nodes
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

// ── UDP receiver task ────────────────────────────────────────────────────────

async fn udp_receiver_task(state: SharedState, udp_port: u16) {
    let addr = format!("0.0.0.0:{udp_port}");
    let socket = match UdpSocket::bind(&addr).await {
        Ok(s) => {
            info!("UDP listening on {addr} for ESP32 CSI frames");
            s
        }
        Err(e) => {
            error!("Failed to bind UDP {addr}: {e}");
            return;
        }
    };

    let mut buf = [0u8; 4096];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, src)) => {
                // ADR-039: Try edge vitals packet first (magic 0xC511_0002).
                if let Some(vitals) = parse_esp32_vitals(&buf[..len]) {
                    debug!(
                        "ESP32 vitals from {src}: node={} br={:.1} hr={:.1} pres={}",
                        vitals.node_id,
                        vitals.breathing_rate_bpm,
                        vitals.heartrate_bpm,
                        vitals.presence
                    );
                    let mut s = state.write().await;
                    if !edge_vitals_measurement_input_allowed(s.position_setup.is_some()) {
                        // A sealed experiment accepts only exact raw CSI whose
                        // TX-source trailer matches the setup. Do not touch
                        // global/per-RX liveness, RSSI, features, D4/D5/D6,
                        // position state, or the raw recorder for this packet.
                        debug!(
                            "ignoring edge-vitals packet from node {} while a sealed position setup is active",
                            vitals.node_id
                        );
                        continue;
                    }
                    // Issue #323: Also emit a sensing_update so the UI renders
                    // detections for ESP32 nodes running the edge DSP pipeline
                    // (Tier 2+).  Without this, vitals arrive but the UI shows
                    // "no detection" because it only renders sensing_update msgs.
                    s.source = "esp32".to_string();
                    s.last_esp32_frame = Some(std::time::Instant::now());

                    // ── Per-node state for edge vitals (issue #249) ──────
                    let node_id = vitals.node_id;
                    let ns = s.node_states.entry(node_id).or_insert_with(NodeState::new);
                    ns.last_frame_time = Some(std::time::Instant::now());
                    ns.edge_vitals = Some(vitals.clone());
                    ns.rssi_history.push_back(vitals.rssi as f64);
                    if ns.rssi_history.len() > 60 {
                        ns.rssi_history.pop_front();
                    }

                    // Store per-node person count from edge vitals.
                    let node_est = if vitals.presence {
                        (vitals.n_persons as usize).max(1)
                    } else {
                        0
                    };
                    ns.prev_person_count = node_est;

                    s.tick += 1;
                    let tick = s.tick;

                    let now = std::time::Instant::now();
                    let mut classification = edge_vitals_classification(&vitals);

                    // Edge-vitals is a useful fallback when a node does not
                    // provide raw CSI. Once D6 is calibrated, however, the
                    // static CSI fingerprints remain authoritative for room
                    // presence. Otherwise an interleaved vitals packet could
                    // overwrite a valid D6 position with the edge classifier.
                    if s.d5_presence.phase() == d5_presence::CalibrationPhase::Ready {
                        let sref: &mut AppStateInner = &mut s;
                        classification = aggregate_node_classification(
                            &sref.node_states,
                            now,
                            &mut sref.d5_presence,
                        );
                    } else {
                        let n_active = s
                            .node_states
                            .values()
                            .filter(|ns| {
                                ns.last_frame_time
                                    .is_some_and(|t| now.duration_since(t).as_secs() < 10)
                            })
                            .count();
                        if n_active > 1 {
                            classification.confidence = (classification.confidence
                                * (1.0 + 0.15 * (n_active as f64 - 1.0)))
                                .clamp(0.0, 1.0);
                        }
                    }
                    classification = apply_position_setup_classification_gate(
                        s.position_setup.is_some(),
                        s.d5_presence.phase(),
                        classification,
                    );
                    let public_vitals = public_edge_vitals_packet(&vitals, &classification);
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "type": "edge_vitals",
                        "node_id": public_vitals.node_id,
                        "presence": public_vitals.presence,
                        "fall_detected": public_vitals.fall_detected,
                        "motion": public_vitals.motion,
                        "breathing_rate_bpm": public_vitals.breathing_rate_bpm,
                        "heartrate_bpm": public_vitals.heartrate_bpm,
                        "n_persons": public_vitals.n_persons,
                        "motion_energy": public_vitals.motion_energy,
                        "presence_score": public_vitals.presence_score,
                        "rssi": public_vitals.rssi,
                    })) {
                        let _ = s.tx.send(json);
                    }

                    // Aggregate person count only after the authoritative
                    // presence decision, matching the raw-CSI path.
                    let _total_persons = if classification.presence {
                        let dedup = s.dedup_factor;
                        let (fused, fallback_count) = multistatic_bridge::fuse_or_fallback(
                            &s.multistatic_fuser,
                            &s.node_states,
                            dedup,
                        );
                        match fused {
                            Some(ref f) => {
                                let score =
                                    multistatic_bridge::compute_person_score_from_amplitudes(
                                        &f.fused_amplitude,
                                    );
                                s.smoothed_person_score =
                                    s.smoothed_person_score * 0.90 + score * 0.10;
                                // #803: don't let the saturating activity score
                                // discard count-aware per-node estimates.
                                let count =
                                    aggregate_person_count(s.person_count(), &s.node_states);
                                s.prev_person_count = count;
                                count.max(1) // presence=true => at least 1
                            }
                            None => {
                                aggregate_person_count(fallback_count.unwrap_or(0), &s.node_states)
                                    .max(1)
                            }
                        }
                    } else {
                        s.prev_person_count = 0;
                        0
                    };

                    // Governed trust cycle (ADR-135..146): run the same live
                    // frames through the privacy/provenance/witness control
                    // plane. Trust state is recorded on the bridge (exposed on
                    // /api/v1/status); engine errors are counted + rate-limit
                    // logged instead of being swallowed (review finding 1).
                    // Split-borrow the two distinct fields off the guard.
                    {
                        let sref: &mut AppStateInner = &mut s;
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        sref.engine_bridge.observe_cycle(&sref.node_states, now_ms);
                    }

                    // Feed field model calibration if active (use per-node history for ESP32).
                    if let Some(frame_history) = s
                        .node_states
                        .get(&node_id)
                        .map(|ns| ns.frame_history.clone())
                    {
                        if let Some(ref mut fm) = s.field_model {
                            field_bridge::maybe_feed_calibration(fm, &frame_history);
                        }
                    }

                    // Build nodes array with all active nodes.
                    let configured_positions = s.multistatic_fuser.node_positions().to_vec();
                    let mut active_nodes: Vec<NodeInfo> = s
                        .node_states
                        .iter()
                        .filter(|(_, n)| {
                            n.last_frame_time
                                .is_some_and(|t| now.duration_since(t).as_secs() < 10)
                        })
                        .map(|(&id, n)| NodeInfo {
                            node_id: id,
                            rssi_dbm: n.rssi_history.back().copied().unwrap_or(0.0),
                            position: configured_node_position(id, &configured_positions),
                            amplitude: vec![],
                            subcarrier_count: 0,
                            // Vitals-only path; still expose the sync snapshot
                            // if the node also speaks ESP-NOW.
                            sync: n.sync_snapshot(),
                        })
                        .collect();
                    active_nodes.sort_by_key(|node| node.node_id);

                    let features = FeatureInfo {
                        mean_rssi: vitals.rssi as f64,
                        variance: vitals.motion_energy as f64,
                        motion_band_power: vitals.motion_energy as f64,
                        breathing_band_power: if vitals.presence { 0.5 } else { 0.0 },
                        dominant_freq_hz: vitals.breathing_rate_bpm / 60.0,
                        change_points: 0,
                        spectral_power: vitals.motion_energy as f64,
                    };

                    // Store latest features on node for cross-node fusion.
                    if let Some(ns) = s.node_states.get_mut(&node_id) {
                        ns.latest_features = Some(features.clone());
                    }

                    // Cross-node fusion: combine features from all active nodes.
                    let fused_features = fuse_multi_node_features(&features, &s.node_states);

                    // Edge-vitals packets contain classifications and vital
                    // estimates but no per-subcarrier fingerprints of their
                    // own. Reuse only fresh D6 evidence already held by the
                    // raw-CSI path; the estimator fails closed otherwise.
                    let localization = estimate_live_localization(
                        &s.node_states,
                        now,
                        &classification,
                        s.tx_position,
                        s.room_dimensions,
                        &configured_positions,
                    );
                    let signal_field = signal_field_from_localization(&localization);
                    let candidate_position_estimate = match raw_csi_recording::now_unix_ns() {
                        Ok(now_unix_ns) => s.live_position_tracker.expire_if_raw_stale(now_unix_ns),
                        Err(error) => s.live_position_tracker.reject_input(format!(
                            "could not verify raw CSI freshness for edge vitals: {error}"
                        )),
                    };
                    let position_estimate = gate_mmwave_candidate_for_publication(
                        candidate_position_estimate,
                        s.mmwave.position_publication_allowed(),
                    );
                    let has_valid_position = classification.presence
                        && matches!(
                            &position_estimate,
                            position_live::LivePositionState::Position { .. }
                        );

                    let mut update = SensingUpdate {
                        msg_type: "sensing_update".to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        source: "esp32".to_string(),
                        tick,
                        tx_position: s.tx_position,
                        room_dimensions: s.room_dimensions,
                        nodes: active_nodes,
                        features: fused_features.clone(),
                        classification,
                        signal_field,
                        localization: Some(localization),
                        position_estimate: Some(position_estimate),
                        vital_signs: Some(VitalSigns {
                            breathing_rate_bpm: if vitals.breathing_rate_bpm > 0.0 {
                                Some(vitals.breathing_rate_bpm)
                            } else {
                                None
                            },
                            heart_rate_bpm: if vitals.heartrate_bpm > 0.0 {
                                Some(vitals.heartrate_bpm)
                            } else {
                                None
                            },
                            breathing_confidence: if vitals.presence { 0.7 } else { 0.0 },
                            heartbeat_confidence: if vitals.presence { 0.7 } else { 0.0 },
                            signal_quality: vitals.presence_score as f64,
                        }),
                        enhanced_motion: None,
                        enhanced_breathing: None,
                        posture: None,
                        signal_quality_score: None,
                        quality_verdict: None,
                        bssid_count: None,
                        pose_keypoints: None,
                        model_status: None,
                        persons: None,
                        estimated_persons: has_valid_position.then_some(1),
                        // ADR-084 Pass 3.6: surface per-node novelty_score
                        // (and the rest of the per-node feature snapshot)
                        // on the WebSocket envelope so cluster-Pi consumers
                        // can implement model-wake gating without round-
                        // tripping back to the server.
                        node_features: build_node_features(
                            &s.node_states,
                            now,
                            s.d5_presence.phase(),
                            s.position_setup.is_some(),
                        ),
                    };

                    let persons = derive_pose_from_sensing(&update);
                    s.pose_tracker = PoseTracker::new();
                    s.last_tracker_instant = None;
                    if !persons.is_empty() {
                        update.persons = Some(persons);
                    }
                    // ESP32 persons are exact discrete markers, never tracked
                    // synthetic skeletons or coarse signal-field peaks.
                    attach_field_positions(&mut update);

                    if let Ok(json) = serde_json::to_string(&update) {
                        let _ = s.tx.send(json);
                    }
                    s.latest_update = Some(update);
                    s.edge_vitals = Some(vitals);
                    continue;
                }

                // ADR-110 §A0.12: Try sync packet (magic 0xC511_A110).
                // A 32-byte UDP datagram carrying mesh-aligned epoch + sequence
                // high-water from the node's c6_sync_espnow EMA-smoothed offset.
                // Stored per-node so subsequent CSI frames with byte 19 bit 4
                // set can have an aligned timestamp recovered downstream.
                if len >= wifi_densepose_hardware::SYNC_PACKET_SIZE {
                    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    if magic == wifi_densepose_hardware::SYNC_PACKET_MAGIC {
                        match wifi_densepose_hardware::SyncPacket::from_bytes(&buf[..len]) {
                            Ok(sync) => {
                                debug!("ESP32 sync from {src}: node={} leader={} valid={} smoothed={} \
                                        seq={} offset_us={}",
                                       sync.node_id, sync.flags.is_leader, sync.flags.is_valid,
                                       sync.flags.smoothed_used, sync.sequence,
                                       sync.local_minus_epoch_us());
                                let mut s = state.write().await;
                                let node_id = sync.node_id;
                                let ns =
                                    s.node_states.entry(node_id).or_insert_with(NodeState::new);
                                ns.apply_sync_packet(sync, std::time::Instant::now());
                                continue;
                            }
                            Err(e) => {
                                debug!("Sync packet decode error from {src}: {e}");
                                // Fall through — magic matched but decode failed; not a CSI frame.
                                continue;
                            }
                        }
                    }
                }

                // ADR-063: Try edge fused vitals packet (magic 0xC511_0004).
                // Must come BEFORE the WASM parser — issue #928: these two
                // packet types shared a magic and the WASM parser was eating
                // fused-vitals frames on the C6+mmWave config. The reassign of
                // WASM_OUTPUT_MAGIC → 0xC511_0007 (firmware side) plus this
                // dedicated parser resolve the collision.
                if let Some(fused) = parse_edge_fused_vitals(&buf[..len]) {
                    debug!(
                        "Edge fused vitals from {src}: node={} br={:.1} hr={:.1} \
                         mmwave_targets={} fusion_conf={}",
                        fused.node_id,
                        fused.breathing_rate_bpm,
                        fused.heartrate_bpm,
                        fused.mmwave_targets,
                        fused.fusion_confidence,
                    );
                    let s = state.write().await;
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "type": "edge_fused_vitals",
                        "node_id": fused.node_id,
                        "breathing_rate_bpm": fused.breathing_rate_bpm,
                        "heartrate_bpm": fused.heartrate_bpm,
                        "n_persons": fused.n_persons,
                        "fusion_confidence": fused.fusion_confidence,
                        "mmwave": {
                            "hr_bpm": fused.mmwave_hr_bpm,
                            "br_bpm": fused.mmwave_br_bpm,
                            "distance_cm": fused.mmwave_distance_cm,
                            "targets": fused.mmwave_targets,
                            "confidence": fused.mmwave_confidence,
                            "type": fused.mmwave_type,
                        },
                        "motion_energy": fused.motion_energy,
                        "presence_score": fused.presence_score,
                        "timestamp_ms": fused.timestamp_ms,
                    })) {
                        let _ = s.tx.send(json);
                    }
                    continue;
                }

                // ADR-040: Try WASM output packet (magic 0xC511_0007 post-#928).
                if let Some(wasm_output) = parse_wasm_output(&buf[..len]) {
                    debug!(
                        "WASM output from {src}: node={} module={} events={}",
                        wasm_output.node_id,
                        wasm_output.module_id,
                        wasm_output.events.len()
                    );
                    let mut s = state.write().await;
                    // Broadcast WASM events via WebSocket.
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "type": "wasm_event",
                        "node_id": wasm_output.node_id,
                        "module_id": wasm_output.module_id,
                        "events": wasm_output.events,
                    })) {
                        let _ = s.tx.send(json);
                    }
                    s.latest_wasm_events = Some(wasm_output);
                    continue;
                }

                if let Some(frame) = parse_esp32_frame(&buf[..len]) {
                    debug!(
                        "ESP32 frame from {src}: node={}, subs={}, seq={}",
                        frame.node_id, frame.n_subcarriers, frame.sequence
                    );

                    let mut s = state.write().await;
                    let frame_now = std::time::Instant::now();

                    // Decode the exact raw frame for live positioning on every
                    // CSI packet before it can affect source liveness,
                    // classification, D4/D5/D6, positioning, or recording.
                    let mesh_timestamp_us = s
                        .node_states
                        .get(&frame.node_id)
                        .and_then(|node| node.mesh_aligned_us_for_csi_frame(frame.sequence));
                    let host_time = server_clock::now();
                    let live_position_timestamp_ns = host_time.host_unix_ns;
                    let context = raw_csi_recording::RawCsiFrameContext {
                        host_timestamp_unix_ns: live_position_timestamp_ns,
                        host_monotonic_ns: Some(host_time.host_monotonic_ns),
                        clock_epoch_id: Some(host_time.clock_epoch_id),
                        mesh_timestamp_us,
                        ..Default::default()
                    };
                    let position_setup = s.position_setup.clone();
                    let raw_frame =
                        match raw_csi_recording::RawCsiFrame::from_packet(&buf[..len], context) {
                            Ok(raw_frame) => raw_frame,
                            Err(error) => {
                                reject_raw_csi_ingress(
                                    &mut s,
                                    Some(frame.node_id),
                                    format!("exact raw CSI validation failed: {error}"),
                                );
                                warn!(
                                "node {}: parsed CSI datagram rejected by exact raw validation: {}",
                                frame.node_id, error
                            );
                                continue;
                            }
                        };
                    let matches_sealed_grid = if let Some(setup) = position_setup.as_deref() {
                        if let Err(error) = setup.validate_raw_csi_source_identity(&raw_frame) {
                            reject_raw_csi_ingress(
                                &mut s,
                                Some(frame.node_id),
                                format!("sealed position setup rejected source identity: {error}"),
                            );
                            warn!(
                                "node {}: sealed position setup rejected raw CSI source identity: {}",
                                frame.node_id, error
                            );
                            continue;
                        }
                        match setup.raw_csi_frame_matches_expected_grid(&raw_frame) {
                            Ok(matches) => matches,
                            Err(error) => {
                                reject_raw_csi_ingress(
                                    &mut s,
                                    Some(frame.node_id),
                                    format!("sealed position setup rejected receiver: {error}"),
                                );
                                continue;
                            }
                        }
                    } else {
                        true
                    };
                    let source_binding_observation = match validated_complete_source_binding(
                        raw_frame.source_binding.as_ref(),
                        frame_now,
                        position_setup.is_some(),
                    ) {
                        Ok(observation) => observation,
                        Err(error) => {
                            reject_raw_csi_ingress(
                                &mut s,
                                Some(frame.node_id),
                                format!("TX-source binding validation failed: {error}"),
                            );
                            continue;
                        }
                    };

                    // A valid controlled transmitter can emit multiple CSI
                    // symbol grids. Keep its binding fresh, but never mix an
                    // off-grid frame into D5/D6, recording, or live position.
                    let grid_accepted = s
                        .node_states
                        .entry(frame.node_id)
                        .or_insert_with(NodeState::new)
                        .observe_validated_grid(
                            source_binding_observation,
                            frame.grid(),
                            matches_sealed_grid,
                        );

                    if !grid_accepted {
                        debug!(
                            "node {}: filtering {}-subcarrier {:?} frame (active grid {:?}, sealed grid match {})",
                            frame.node_id,
                            frame.n_subcarriers,
                            frame.ppdu_type,
                            s.node_states
                                .get(&frame.node_id)
                                .and_then(|ns| ns.active_grid),
                            matches_sealed_grid,
                        );
                        continue;
                    }

                    s.mmwave.observe_csi(&raw_frame);

                    s.source = "esp32".to_string();
                    s.last_esp32_frame = Some(frame_now);

                    if s.recording_active
                        && s.raw_csi_tx
                            .send(RawCsiIngress::Frame(raw_frame.clone()))
                            .is_err()
                    {
                        warn!(
                            "node {}: active raw recorder has no receiver",
                            frame.node_id
                        );
                    }
                    let live_position_input_accepted = match route_raw_frame_to_live_position(
                        &mut s.live_position_tracker,
                        position_setup.as_deref(),
                        true,
                        raw_frame,
                    ) {
                        Ok(()) => {
                            s.last_raw_csi_frame = Some(frame_now);
                            true
                        }
                        Err(error) => {
                            let estimate = s.live_position_tracker.current().clone();
                            replace_latest_esp32_position_estimate(&mut s, estimate);
                            debug!(
                                "node {}: live position rejected raw frame: {}",
                                frame.node_id, error
                            );
                            false
                        }
                    };

                    // Also maintain global frame_history for backward compat
                    // (simulation path, REST endpoints, etc.).
                    s.frame_history.push_back(frame.amplitudes.clone());
                    if s.frame_history.len() > FRAME_HISTORY_CAPACITY {
                        s.frame_history.pop_front();
                    }

                    // ── ADR-099: real-time introspection tap ────────────────
                    // Per-frame update of the attractor / DTW pipeline running
                    // parallel to the window-aggregated event path. Placed
                    // BEFORE the per-node `&mut` borrow of `s.node_states` so
                    // `s.intro` / `s.intro_tx` stay reachable. Never window-
                    // blocked; `/ws/introspection` sees a fresh snapshot on
                    // every accepted frame.
                    {
                        let intro_feature = if frame.amplitudes.is_empty() {
                            0.0
                        } else {
                            frame.amplitudes.iter().copied().sum::<f64>()
                                / frame.amplitudes.len() as f64
                        };
                        let intro_ts_ns = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        let _ = s.intro.update(intro_ts_ns, intro_feature);
                        if let Ok(intro_json) = serde_json::to_string(s.intro.snapshot()) {
                            let _ = s.intro_tx.send(intro_json);
                        }
                    }

                    // ── Per-node processing (issue #249) ──────────────────
                    // Process entirely within per-node state so different
                    // ESP32 nodes never mix their smoothing/vitals buffers.
                    // We scope the mutable borrow of node_states so we can
                    // access other AppStateInner fields afterward.
                    let node_id = frame.node_id;
                    // Clone adaptive model before mutable borrow of node_states
                    // to avoid unsafe raw pointer (review finding #2).
                    let adaptive_model_clone = s.adaptive_model.clone();
                    let d5_phase = s.d5_presence.phase();

                    let ns = s.node_states.entry(node_id).or_insert_with(NodeState::new);
                    let (features, mut classification) =
                        observe_frame_for_presence(ns, &frame, frame_now, d5_phase);

                    // Adaptive override using cloned model (safe, no raw pointers).
                    if let Some(ref model) = adaptive_model_clone {
                        let amps = ns.frame_history.back().map(|v| v.as_slice()).unwrap_or(&[]);
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
                        classification.confidence =
                            (conf * 0.7 + classification.confidence * 0.3).clamp(0.0, 1.0);
                    }
                    ns.motion_confidence = classification.confidence;

                    ns.rssi_history.push_back(features.mean_rssi);
                    if ns.rssi_history.len() > 60 {
                        ns.rssi_history.pop_front();
                    }

                    let raw_vitals = ns
                        .vital_detector
                        .process_frame(&frame.amplitudes, &frame.phases);
                    let vitals = smooth_vitals_node(ns, &raw_vitals);
                    ns.latest_vitals = vitals.clone();

                    // DynamicMinCut person estimation from subcarrier correlation.
                    let corr_persons = estimate_persons_from_correlation(&ns.frame_history);
                    // #803: map the min-cut count onto a threshold-aligned score
                    // so it round-trips back to the same count. The old
                    // `corr_persons / 3.0` left 2 people at 0.667 — under the
                    // 0.70 up-threshold — so the count was pinned at 1.
                    let raw_score = corr_persons_to_score(corr_persons);
                    ns.smoothed_person_score = ns.smoothed_person_score * 0.92 + raw_score * 0.08;
                    if classification.presence {
                        let count =
                            score_to_person_count(ns.smoothed_person_score, ns.prev_person_count);
                        ns.prev_person_count = count;
                    } else {
                        ns.prev_person_count = 0;
                    }

                    // Store latest features on node for cross-node fusion.
                    ns.latest_features = Some(features.clone());

                    // Done with per-node mutable borrow; now read aggregated
                    // state from all nodes (the borrow of `ns` ends here).
                    // (We re-borrow node_states immutably via `s` below.)

                    s.rssi_history.push_back(features.mean_rssi);
                    if s.rssi_history.len() > 60 {
                        s.rssi_history.pop_front();
                    }
                    s.latest_vitals = vitals.clone();

                    // Cross-node fusion: combine features from all active nodes.
                    let fused_features = fuse_multi_node_features(&features, &s.node_states);
                    let now = frame_now;

                    // The room-level classification is a consensus across all
                    // live RX links, not the result of the last UDP packet.
                    let classification = {
                        let sref: &mut AppStateInner = &mut s;
                        aggregate_node_classification(&sref.node_states, now, &mut sref.d5_presence)
                    };
                    let classification = apply_position_setup_classification_gate(
                        s.position_setup.is_some(),
                        s.d5_presence.phase(),
                        classification,
                    );
                    let position_gate =
                        live_position_presence_gate(&s.node_states, now, &s.d5_presence);
                    let candidate_position_estimate = if live_position_input_accepted {
                        s.live_position_tracker
                            .tick(live_position_timestamp_ns, position_gate)
                    } else {
                        s.live_position_tracker.current().clone()
                    };
                    // A freshly generated mmWave-gated model remains usable
                    // inside the held-back evaluator but cannot become public
                    // before every blind-position gate passes.
                    let position_estimate = gate_mmwave_candidate_for_publication(
                        candidate_position_estimate,
                        s.mmwave.position_publication_allowed(),
                    );
                    let has_valid_position = classification.presence
                        && matches!(
                            &position_estimate,
                            position_live::LivePositionState::Position { .. }
                        );

                    s.tick += 1;
                    let tick = s.tick;

                    // Aggregate person count: gate on presence first (matching WiFi path).
                    let _total_persons = if classification.presence {
                        let dedup = s.dedup_factor;
                        let (fused, fallback_count) = multistatic_bridge::fuse_or_fallback(
                            &s.multistatic_fuser,
                            &s.node_states,
                            dedup,
                        );
                        match fused {
                            Some(ref f) => {
                                let score =
                                    multistatic_bridge::compute_person_score_from_amplitudes(
                                        &f.fused_amplitude,
                                    );
                                s.smoothed_person_score =
                                    s.smoothed_person_score * 0.90 + score * 0.10;
                                // #803: don't let the saturating activity score
                                // discard count-aware per-node estimates.
                                let count =
                                    aggregate_person_count(s.person_count(), &s.node_states);
                                s.prev_person_count = count;
                                count.max(1)
                            }
                            None => {
                                aggregate_person_count(fallback_count.unwrap_or(0), &s.node_states)
                                    .max(1)
                            }
                        }
                    } else {
                        s.prev_person_count = 0;
                        0
                    };

                    // Governed trust cycle (ADR-135..146): run the same live
                    // frames through the privacy/provenance/witness control
                    // plane. Trust state is recorded on the bridge (exposed on
                    // /api/v1/status); engine errors are counted + rate-limit
                    // logged instead of being swallowed (review finding 1).
                    // Split-borrow the two distinct fields off the guard.
                    {
                        let sref: &mut AppStateInner = &mut s;
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        sref.engine_bridge.observe_cycle(&sref.node_states, now_ms);
                    }

                    // Feed field model calibration if active (use per-node history for ESP32).
                    if let Some(frame_history) = s
                        .node_states
                        .get(&node_id)
                        .map(|ns| ns.frame_history.clone())
                    {
                        if let Some(ref mut fm) = s.field_model {
                            field_bridge::maybe_feed_calibration(fm, &frame_history);
                        }
                    }

                    // Build nodes array with all active nodes. ADR-141 output
                    // gating (review finding 1c): when the governed engine
                    // emitted this cycle at class Restricted (base mode, or a
                    // contradiction/mesh-risk demotion below the configured
                    // class), the per-node raw amplitude vectors are suppressed
                    // from the live publish — the same field mapping bfld's
                    // privacy gate applies at Restricted (drop amplitude/phase
                    // proxies).
                    let suppress_raw = s.engine_bridge.suppress_raw_outputs();
                    let configured_positions = s.multistatic_fuser.node_positions().to_vec();
                    let mut active_nodes: Vec<NodeInfo> = s
                        .node_states
                        .iter()
                        .filter(|(_, n)| {
                            n.last_frame_time
                                .is_some_and(|t| now.duration_since(t).as_secs() < 10)
                        })
                        .map(|(&id, n)| NodeInfo {
                            node_id: id,
                            rssi_dbm: n.rssi_history.back().copied().unwrap_or(0.0),
                            position: configured_node_position(id, &configured_positions),
                            amplitude: if suppress_raw {
                                vec![]
                            } else {
                                n.frame_history
                                    .back()
                                    .map(|a| a.iter().take(56).cloned().collect())
                                    .unwrap_or_default()
                            },
                            subcarrier_count: if suppress_raw {
                                0
                            } else {
                                n.frame_history.back().map_or(0, |a| a.len())
                            },
                            // ADR-110 iter 23 / iter 30 — single source of truth.
                            sync: n.sync_snapshot(),
                        })
                        .collect();
                    active_nodes.sort_by_key(|node| node.node_id);
                    let localization = estimate_live_localization(
                        &s.node_states,
                        now,
                        &classification,
                        s.tx_position,
                        s.room_dimensions,
                        &configured_positions,
                    );
                    let signal_field = signal_field_from_localization(&localization);

                    let mut update = SensingUpdate {
                        msg_type: "sensing_update".to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        source: "esp32".to_string(),
                        tick,
                        tx_position: s.tx_position,
                        room_dimensions: s.room_dimensions,
                        nodes: active_nodes,
                        features: fused_features.clone(),
                        classification,
                        signal_field,
                        localization: Some(localization),
                        position_estimate: Some(position_estimate),
                        vital_signs: Some(vitals),
                        enhanced_motion: None,
                        enhanced_breathing: None,
                        posture: None,
                        signal_quality_score: None,
                        quality_verdict: None,
                        bssid_count: None,
                        pose_keypoints: None,
                        model_status: None,
                        persons: None,
                        estimated_persons: has_valid_position.then_some(1),
                        // ADR-084 Pass 3.6: surface per-node novelty_score
                        // (and the rest of the per-node feature snapshot)
                        // on the WebSocket envelope so cluster-Pi consumers
                        // can implement model-wake gating without round-
                        // tripping back to the server.
                        node_features: build_node_features(
                            &s.node_states,
                            now,
                            s.d5_presence.phase(),
                            s.position_setup.is_some(),
                        ),
                    };

                    let persons = derive_pose_from_sensing(&update);
                    s.pose_tracker = PoseTracker::new();
                    s.last_tracker_instant = None;
                    if !persons.is_empty() {
                        update.persons = Some(persons);
                    }
                    // ESP32 persons are exact discrete markers, never tracked
                    // synthetic skeletons or coarse signal-field peaks.
                    attach_field_positions(&mut update);

                    if let Ok(json) = serde_json::to_string(&update) {
                        let _ = s.tx.send(json);
                    }

                    // ── ADR-262 P3: emit a signed RuField FieldEvent ────────
                    // Join this cycle's SensingUpdate (features / classification
                    // / signal_field) with the governed engine's trust state
                    // (effective_class / demoted, recorded by `observe_cycle`
                    // above) into a `SensingSnapshot`, and surface it on
                    // `/api/field` + `/ws/field` via the P1 bridge. Only cycles
                    // whose mapped privacy class clears the §10 network egress
                    // gate are surfaced (P1/P2); a `Derived → P4/P5` cycle is
                    // held edge-local. `presence == false` ⇒ no phantom event.
                    emit_rufield_event(&s, &update, node_id);

                    s.latest_update = Some(update);

                    // Evict stale nodes every 100 ticks to prevent memory leak.
                    if tick % 100 == 0 {
                        let stale = Duration::from_secs(60);
                        let before = s.node_states.len();
                        s.node_states.retain(|_id, ns| {
                            ns.last_frame_time
                                .is_some_and(|t| now.duration_since(t) < stale)
                        });
                        let evicted = before - s.node_states.len();
                        if evicted > 0 {
                            info!(
                                "Evicted {} stale node(s), {} active",
                                evicted,
                                s.node_states.len()
                            );
                        }
                    }
                } else if has_esp32_csi_magic(&buf[..len]) {
                    // The CSI magic is authoritative even when the derived
                    // parser cannot safely construct a frame (for example a
                    // truncated I/Q payload). Treat it as rejected CSI rather
                    // than an unrelated UDP packet so an active capture cannot
                    // silently remain "complete". Only byte 4 is used as RX ID
                    // when it is actually present.
                    let rx_id = esp32_csi_header_rx_id(&buf[..len]);
                    let mut s = state.write().await;
                    reject_raw_csi_ingress(
                        &mut s,
                        rx_id,
                        "CSI datagram has valid magic but an invalid or truncated frame payload",
                    );
                    warn!("malformed ESP32 CSI datagram from {src} rejected before sensing");
                }
            }
            Err(e) => {
                warn!("UDP recv error: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

// ── Simulated data task ──────────────────────────────────────────────────────

async fn simulated_data_task(state: SharedState, tick_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
    info!("Simulated data source active (tick={}ms)", tick_ms);

    loop {
        interval.tick().await;

        let mut s = state.write().await;

        // Issue #1004: in `auto` mode this task runs alongside `udp_receiver_task`.
        // Once a real frame promotes `source` → "esp32", stop emitting synthetic
        // frames so we never clobber live CSI with simulated poses. (For an
        // explicit `--source simulated` demo, `source` stays "simulated" and the
        // simulator keeps running — that path never binds UDP, so it is never
        // promoted.) The task stays alive so it can resume serving if the real
        // source later ages out to "esp32:offline".
        if s.effective_source() == "esp32" {
            continue;
        }

        s.tick += 1;
        let tick = s.tick;

        let frame = generate_simulated_frame(tick);

        // Append current amplitudes to history before feature extraction.
        s.frame_history.push_back(frame.amplitudes.clone());
        if s.frame_history.len() > FRAME_HISTORY_CAPACITY {
            s.frame_history.pop_front();
        }

        let sample_rate_hz = 1000.0 / tick_ms as f64;
        let (features, mut classification, breathing_rate_hz, sub_variances, raw_motion) =
            extract_features_from_frame(&frame, &s.frame_history, sample_rate_hz);
        smooth_and_classify(&mut s, &mut classification, raw_motion);
        adaptive_override(&s, &features, &mut classification);

        s.rssi_history.push_back(features.mean_rssi);
        if s.rssi_history.len() > 60 {
            s.rssi_history.pop_front();
        }

        let motion_score = motion_score_for_level(&classification.motion_level);

        let raw_vitals = s
            .vital_detector
            .process_frame(&frame.amplitudes, &frame.phases);
        let vitals = smooth_vitals(&mut s, &raw_vitals);
        s.latest_vitals = vitals.clone();

        let frame_amplitudes = frame.amplitudes.clone();
        let frame_n_sub = frame.n_subcarriers;

        // ADR-044 §5.2: feed raw features into rolling-P95 estimators before scoring.
        s.p95_variance.push(features.variance);
        s.p95_motion_band_power.push(features.motion_band_power);
        s.p95_spectral_power.push(features.spectral_power);

        // Multi-person estimation with temporal smoothing (EMA α=0.10).
        let raw_score = compute_person_score(&s, &features);
        s.smoothed_person_score = s.smoothed_person_score * 0.90 + raw_score * 0.10;
        let est_persons = if classification.presence {
            let count = s.person_count();
            s.prev_person_count = count;
            count
        } else {
            s.prev_person_count = 0;
            0
        };

        let mut update = SensingUpdate {
            msg_type: "sensing_update".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            source: "simulated".to_string(),
            tick,
            tx_position: s.tx_position,
            room_dimensions: s.room_dimensions,
            nodes: vec![NodeInfo {
                node_id: 1,
                rssi_dbm: features.mean_rssi,
                position: [2.0, 0.0, 1.5],
                amplitude: frame_amplitudes,
                subcarrier_count: frame_n_sub as usize,
                sync: None, // simulated frame path — no mesh peer
            }],
            features: features.clone(),
            classification,
            signal_field: generate_signal_field(
                features.mean_rssi,
                motion_score,
                breathing_rate_hz,
                features.variance.min(1.0),
                &sub_variances,
            ),
            localization: None,
            position_estimate: None,
            vital_signs: Some(vitals),
            enhanced_motion: None,
            enhanced_breathing: None,
            posture: None,
            signal_quality_score: None,
            quality_verdict: None,
            bssid_count: None,
            pose_keypoints: None,
            model_status: if s.model_loaded {
                Some(serde_json::json!({
                    "loaded": true,
                    "layers": s.progressive_loader.as_ref()
                        .map(|l| { let (a,b,c) = l.layer_status(); a as u8 + b as u8 + c as u8 })
                        .unwrap_or(0),
                    "sona_profile": s.active_sona_profile.as_deref().unwrap_or("default"),
                }))
            } else {
                None
            },
            persons: None,
            estimated_persons: if est_persons > 0 {
                Some(est_persons)
            } else {
                None
            },
            node_features: None,
        };

        // Populate persons from the sensing update (Kalman-smoothed via tracker).
        let raw_persons = derive_pose_from_sensing(&update);
        let mut last_tracker_instant = s.last_tracker_instant.take();
        let tracked = tracker_bridge::tracker_update(
            &mut s.pose_tracker,
            &mut last_tracker_instant,
            raw_persons,
        );
        s.last_tracker_instant = last_tracker_instant;
        if !tracked.is_empty() {
            update.persons = Some(tracked);
        }
        // #1050: attach real signal_field-peak positions to each person.
        attach_field_positions(&mut update);

        if update.classification.presence {
            s.total_detections += 1;
        }
        if let Ok(json) = serde_json::to_string(&update) {
            let _ = s.tx.send(json);
        }
        s.latest_update = Some(update);
    }
}

// ── Broadcast tick task (for ESP32 mode, sends buffered state) ───────────────

async fn broadcast_tick_task(state: SharedState, tick_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));

    loop {
        interval.tick().await;
        let mut s = state.write().await;
        if s.latest_update
            .as_ref()
            .is_some_and(|update| update.source == "esp32")
            && position_raw_input_is_stale(s.last_raw_csi_frame, std::time::Instant::now())
        {
            let estimate = match raw_csi_recording::now_unix_ns() {
                Ok(now_unix_ns) => s.live_position_tracker.expire_if_raw_stale(now_unix_ns),
                Err(error) => s.live_position_tracker.reject_input(format!(
                    "could not verify raw CSI freshness before rebroadcast: {error}"
                )),
            };
            apply_latest_esp32_position_estimate(&mut s, estimate);
        }
        if let Some(ref update) = s.latest_update {
            if s.tx.receiver_count() > 0 {
                // Re-broadcast the latest sensing_update so pose WS clients
                // always get data even when ESP32 pauses between frames.
                //
                // Tag every rebroadcast with `effective_source()`. When an
                // ESP32 stream is offline, `public_sensing_update` also clears
                // stale detections, fields, and vital estimates so a frozen
                // position is never presented as current evidence.
                let effective_source = s.effective_source();
                let tagged = public_sensing_update(update, &effective_source);
                if let Ok(json) = serde_json::to_string(&tagged) {
                    let _ = s.tx.send(json);
                }
            }
        }
    }
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
    // Issue #937 (prior fix): `auto` must never serve fake CSI *tagged as
    // production telemetry*. We keep that guarantee — in the gap before real
    // CSI arrives, `source` is the honest string "simulated" (downstream
    // `/api/v1/sensing/latest`, `/ws/sensing` see `source: "simulated"`, not a
    // production tag). What #937's hard-exit got wrong: at boot the firmware and
    // server race, so CSI usually is NOT flowing during the 2 s probe. Exiting
    // (or latching on simulate) meant the server could never pick up CSI that
    // started seconds later. The robust resolution (see `plan_source`): in
    // `auto` always bind the UDP :5005 receiver; serve simulated until the first
    // real frame; then `udp_receiver_task` promotes `source` → "esp32". Explicit
    // `--source simulated` stays a hard, UDP-free override for offline demos.
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
                "No real CSI source at boot — serving SIMULATED data (tagged as \
                 'simulated', not production) while the UDP :{} receiver stays bound. \
                 The server promotes to live the instant a real frame arrives (issue \
                 #1004). For an offline demo with no live promotion, pass \
                 --source simulated explicitly.",
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
    let models_dir = effective_models_dir();
    let _ = std::fs::create_dir_all(&models_dir);
    let _ = std::fs::create_dir_all("data/recordings");

    // Discover model and recording files on startup
    let initial_models = scan_model_files();
    let initial_recordings = scan_recording_files();
    info!(
        "Discovered {} model files, {} recording files",
        initial_models.len(),
        initial_recordings.len()
    );

    // ADR-044 §5.3: load persisted runtime config from the data directory.
    let data_dir = std::path::PathBuf::from("data");
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

    // ADR-262 P3: build the live RuField surface (dedicated ed25519 signer from
    // WDP_RUFIELD_SIGNING_SEED, else a logged dev default). The same Arc is
    // stored in AppStateInner (so the sensing loop can `emit()` per cycle) and
    // cloned into the additive `/api/field` + `/ws/field` router below.
    let field_surface: rufield_surface::FieldState =
        Arc::new(RwLock::new(rufield_surface::FieldSurface::from_env()));

    let mmwave_control = args.mmwave_node_url.as_ref().and_then(|base_url| {
        match std::env::var(&args.mmwave_token_env) {
            Ok(bearer_token) if !bearer_token.trim().is_empty() => {
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

    let state: SharedState = Arc::new(RwLock::new(AppStateInner {
        latest_update: None,
        rssi_history: VecDeque::new(),
        frame_history: VecDeque::new(),
        tick: 0,
        source: source.into(),
        tx_position: runtime_position_geometry.tx_position,
        room_dimensions: runtime_position_geometry.room_dimensions,
        position_setup: position_setup.clone(),
        mmwave: mmwave_calibration::MmwaveManager::new(
            args.mmwave_udp_port,
            runtime_position_geometry.room_dimensions,
            mmwave_control,
            position_setup
                .as_deref()
                .and_then(position_setup::SealedPositionSetup::mmwave)
                .map(|definition| {
                    let (origin_x_mm, origin_z_mm, yaw_mdeg, raw_x_inverted) =
                        definition.transform();
                    mmwave_calibration::ExpectedNode {
                        node_id: definition.node_id().to_string(),
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
                Some(mmwave_calibration::ExperimentContext {
                    setup_id: setup.setup_id().to_string(),
                    setup_sha256: setup.setup_sha256().to_string(),
                    server_version: env!("CARGO_PKG_VERSION").to_string(),
                    geometry: position_capture::PositionCaptureGeometry {
                        room_dimensions_m: setup.room_dimensions_m(),
                        tx_position_m: setup.transmitter_position_m(),
                        rx_positions_m: setup.receiver_positions_m().to_vec(),
                    },
                })
            }),
        ),
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
        // Training
        training_status: "idle".to_string(),
        training_config: None,
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
        d5_presence: d5_presence::PresenceFusionState::default(),
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
    // In `auto` mode with no boot source, `bind_udp` AND `run_simulator` are
    // both true: the UDP receiver is bound so real CSI can promote the source,
    // and the simulator serves poses in the meantime (it self-suspends once
    // promoted — see `simulated_data_task`). Explicit `--source simulated` has
    // `bind_udp = false`, so it serves simulated data only, with no live binding.
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
    tokio::spawn(mmwave_receiver_task(state.clone(), args.mmwave_udp_port));

    // ADR-166: Parse bind address once, use for all listeners
    let bind_ip: std::net::IpAddr = args
        .bind_addr
        .parse()
        .expect("Invalid --bind-addr (use 127.0.0.1 or 0.0.0.0)");

    // #443: optional bearer-token auth on `/api/v1/*`. `RUVIEW_API_TOKEN`
    // unset/empty ⇒ middleware is a no-op (LAN-mode default preserved); set ⇒
    // every `/api/v1/*` request must carry `Authorization: Bearer <token>`.
    let bearer_auth_state = wifi_densepose_sensing_server::bearer_auth::AuthState::from_env();
    if bearer_auth_state.is_enabled() {
        info!("API auth: bearer-token enforcement ON for /api/v1/* (RUVIEW_API_TOKEN set)");
        if bind_ip.is_unspecified() {
            warn!(
                "API auth ON but bind-addr is {} — consider --bind-addr 127.0.0.1 for LAN-only deployments",
                bind_ip
            );
        }
    } else {
        info!(
            "API auth: OFF — /api/v1/* is unauthenticated. Set RUVIEW_API_TOKEN=<token> to enforce bearer auth."
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

    // WebSocket server on dedicated port (8765)
    let ws_state = state.clone();
    let ws_app = Router::new()
        .route("/ws/sensing", get(ws_sensing_handler))
        .route("/health", get(health))
        .with_state(ws_state)
        // ADR-262 P3: additive `/ws/field` (+ `/api/field`) on the WS port too,
        // so a client on :8765 can stream signed RuField FieldEvents alongside
        // `/ws/sensing`. Merged with its own FieldState (different state type).
        .merge(rufield_surface::router(field_surface.clone()))
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
        // API info
        .route("/api/v1/info", get(api_info))
        .route("/api/v1/status", get(health_ready))
        .route("/api/v1/metrics", get(health_metrics))
        // Sensing endpoints
        .route("/api/v1/sensing/latest", get(latest))
        // Observatory Control Center — metadata-only experiment catalogue and
        // deterministic software replay. These routes never touch hardware.
        .route("/api/v1/experiments/status", get(experiments_status))
        .route(
            "/api/v1/experiments/runs",
            get(experiments_list).post(experiments_create),
        )
        .route(
            "/api/v1/experiments/runs/:id",
            get(experiment_get),
        )
        .route(
            "/api/v1/experiments/runs/:id/replay",
            post(experiment_replay),
        )
        .route(
            "/api/v1/experiments/setup-profiles",
            get(setup_profiles_list).post(setup_profile_create),
        )
        .route(
            "/api/v1/experiments/setup-profiles/:id",
            put(setup_profile_update),
        )
        .route(
            "/api/v1/experiments/workflows",
            post(workflow_create),
        )
        .route(
            "/api/v1/experiments/runs/:id/phase",
            post(workflow_advance),
        )
        .route(
            "/api/v1/experiments/runs/:id/artifacts",
            post(workflow_artifact_register),
        )
        .route(
            "/api/v1/experiments/runs/:id/report",
            get(experiment_report).post(workflow_report),
        )
        .route(
            "/api/v1/experiments/runs/:id/export",
            get(experiment_export),
        )
        .route("/api/v1/control-center/status", get(control_center_status))
        .route("/api/v1/benchmarks/catalog", get(benchmark_catalog))
        // Independent HLK-LD2450 calibration teacher and blind reference.
        .route("/api/v1/mmwave/status", get(mmwave_status_endpoint))
        .route("/api/v1/mmwave/mode", put(mmwave_mode_endpoint))
        .route("/api/v1/mmwave/transform", put(mmwave_transform_endpoint))
        .route(
            "/api/v1/mmwave/session/start",
            post(mmwave_session_start_endpoint),
        )
        .route("/api/v1/mmwave/session/status", get(mmwave_status_endpoint))
        .route(
            "/api/v1/mmwave/session/stop",
            post(mmwave_session_stop_endpoint),
        )
        // Per-node health endpoint
        .route("/api/v1/nodes", get(nodes_endpoint))
        // ADR-110 iter 29 — per-node mesh sync state for HTTP clients.
        .route("/api/v1/nodes/:id/sync", get(node_sync_endpoint))
        .route("/api/v1/mesh", get(mesh_endpoint))
        .route("/api/v1/mesh/metrics", get(mesh_metrics_endpoint))
        // Vital sign endpoints
        .route("/api/v1/vital-signs", get(vital_signs_endpoint))
        .route("/api/v1/edge-vitals", get(edge_vitals_endpoint))
        // ADR-102: Edge Module Registry — surfaces the canonical Cognitum cog
        // catalog (`https://storage.googleapis.com/cognitum-apps/app-registry.json`)
        // with in-process TTL cache + stale-on-error fallback. Disabled when
        // --no-edge-registry is set (returns 404).
        .route("/api/v1/edge/registry", get(edge_registry_endpoint))
        .route("/api/v1/wasm-events", get(wasm_events_endpoint))
        // RVF model container info
        .route("/api/v1/model/info", get(model_info))
        // Progressive loading & SONA endpoints (Phase 7-8)
        .route("/api/v1/model/layers", get(model_layers))
        .route("/api/v1/model/segments", get(model_segments))
        .route("/api/v1/model/sona/profiles", get(sona_profiles))
        .route("/api/v1/model/sona/activate", post(sona_activate))
        // Pose endpoints (WiFi-derived)
        .route("/api/v1/pose/current", get(pose_current))
        .route("/api/v1/pose/stats", get(pose_stats))
        .route("/api/v1/pose/zones/summary", get(pose_zones_summary))
        // Stream endpoints
        .route("/api/v1/stream/status", get(stream_status))
        .route("/api/v1/stream/pose", get(ws_pose_handler))
        // Sensing WebSocket on the HTTP port so the UI can reach it without a second port
        .route("/ws/sensing", get(ws_sensing_handler))
        // ADR-099: real-time introspection — per-frame attractor + DTW snapshot.
        .route("/ws/introspection", get(ws_introspection_handler))
        .route(
            "/api/v1/introspection/snapshot",
            get(api_introspection_snapshot),
        )
        // Model management endpoints (UI compatibility)
        .route("/api/v1/models", get(list_models))
        .route("/api/v1/models/active", get(get_active_model))
        .route("/api/v1/models/load", post(load_model))
        .route("/api/v1/models/unload", post(unload_model))
        .route("/api/v1/models/:id", delete(delete_model))
        .route("/api/v1/models/lora/profiles", get(list_lora_profiles))
        .route("/api/v1/models/lora/activate", post(activate_lora_profile))
        // Recording endpoints
        .route("/api/v1/recording/list", get(list_recordings))
        .route("/api/v1/recording/start", post(start_recording))
        .route("/api/v1/recording/stop", post(stop_recording))
        .route("/api/v1/recording/:id", delete(delete_recording))
        // Training endpoints
        .route("/api/v1/train/status", get(train_status))
        .route("/api/v1/train/start", post(train_start))
        .route("/api/v1/train/stop", post(train_stop))
        // Adaptive classifier endpoints
        .route("/api/v1/adaptive/train", post(adaptive_train))
        .route("/api/v1/adaptive/status", get(adaptive_status))
        .route("/api/v1/adaptive/unload", post(adaptive_unload))
        // Experimental D5 still-presence calibration (separate from FieldModel).
        .route(
            "/api/v1/classification/calibration/start",
            post(classification_calibration_start),
        )
        .route(
            "/api/v1/classification/calibration/stop",
            post(classification_calibration_stop),
        )
        .route(
            "/api/v1/classification/calibration/status",
            get(classification_calibration_status),
        )
        // Field model calibration (eigenvalue-based person counting)
        .route("/api/v1/calibration/start", post(calibration_start))
        .route("/api/v1/calibration/stop", post(calibration_stop))
        .route("/api/v1/calibration/status", get(calibration_status))
        // ADR-044 §5.3: runtime-configurable dedup factor
        .route(
            "/api/v1/config/dedup-factor",
            get(config_get_dedup_factor).post(config_set_dedup_factor),
        )
        .route("/api/v1/config/ground-truth", post(config_set_ground_truth))
        // Static UI files
        .nest_service("/ui", ServeDir::new(&ui_path))
        // ADR-102: make the edge registry handle (Option<Arc<EdgeRegistry>>)
        // available to the /api/v1/edge/registry handler. None when disabled.
        .layer(Extension(edge_registry.clone()))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        // Opt-in bearer-token auth on `/api/v1/*` (#443). When `RUVIEW_API_TOKEN`
        // is unset/empty the middleware is a no-op — the default stays
        // LAN-mode-friendly. `/health*`, `/ws/sensing`, and `/ui/*` are never
        // gated (orchestrator probes + local browsers).
        .layer(axum::middleware::from_fn_with_state(
            bearer_auth_state.clone(),
            wifi_densepose_sensing_server::bearer_auth::require_bearer,
        ))
        .with_state(state.clone())
        // ADR-262 P3: additive RuField surface (`/api/field` + `/ws/field`).
        // Merged AFTER `.with_state` (so http_app is already `Router<()>` and
        // can absorb the field router's own `FieldState`). These routes sit
        // OUTSIDE `/api/v1/*` so they are not bearer-gated, but the
        // host-validation layer below still applies (it is added last, so it
        // runs first, over the whole merged router). The surface's own §10
        // egress gate is what keeps above-policy classes off the wire.
        .merge(rufield_surface::router(field_surface.clone()))
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
    let server = axum::serve(http_listener, http_app).with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C handler");
        info!("Shutdown signal received");
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
        let sync = populated_sync(9);
        let expected = sync.mesh_aligned_us_for_sequence(22, 20.0);
        let now = std::time::Instant::now();
        ns.apply_sync_packet(sync, now);

        let frame_time = now + std::time::Duration::from_millis(50);
        ns.observe_accepted_csi_frame(22, frame_time);

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

// ── ADR-044 §5.3: dedup_factor runtime configuration endpoints ────────────────

/// `GET /api/v1/config/dedup-factor` — read the current dedup factor.
async fn config_get_dedup_factor(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "dedup_factor": s.dedup_factor,
        "description": "Divisor for multi-node person count deduplication (sum / factor). Range: 1.0–10.0."
    }))
}

/// `POST /api/v1/config/dedup-factor` — set the dedup factor (clamped 1.0–10.0).
///
/// Body: `{ "value": <f64> }`
async fn config_set_dedup_factor(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let value = body.get("value").and_then(|v| v.as_f64()).unwrap_or(3.0);
    let clamped = value.clamp(1.0, 10.0);
    let mut s = state.write().await;
    s.dedup_factor = clamped;
    let data_dir = s.data_dir.clone();
    drop(s);
    save_runtime_config(
        &data_dir,
        &RuntimeConfig {
            dedup_factor: clamped,
        },
    );
    Json(serde_json::json!({
        "status": "ok",
        "dedup_factor": clamped,
    }))
}

/// `POST /api/v1/config/ground-truth` — auto-tune dedup factor from a known person count.
///
/// Derives `dedup_factor = raw_node_sum / ground_truth_count` from the current
/// per-node person counts, clamped to [1.0, 10.0].  Persisted immediately.
///
/// Body: `{ "count": <u64> }`
async fn config_set_ground_truth(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let ground_truth = match body.get("count").and_then(|v| v.as_u64()) {
        Some(n) if n > 0 => n as usize,
        _ => return Json(serde_json::json!({"error": "count must be a positive integer"})),
    };
    let mut s = state.write().await;
    let raw_sum: usize = s
        .node_states
        .values()
        .filter(|ns| {
            ns.last_frame_time
                .map(|t| t.elapsed() < std::time::Duration::from_secs(10))
                .unwrap_or(false)
        })
        .map(|ns| ns.prev_person_count)
        .sum();
    let optimal = if raw_sum > 0 {
        (raw_sum as f64) / (ground_truth as f64)
    } else {
        3.0
    };
    let clamped = optimal.clamp(1.0, 10.0);
    s.dedup_factor = clamped;
    let data_dir = s.data_dir.clone();
    drop(s);
    save_runtime_config(
        &data_dir,
        &RuntimeConfig {
            dedup_factor: clamped,
        },
    );
    Json(serde_json::json!({
        "status": "ok",
        "ground_truth": ground_truth,
        "raw_sum": raw_sum,
        "computed_dedup_factor": clamped,
    }))
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
