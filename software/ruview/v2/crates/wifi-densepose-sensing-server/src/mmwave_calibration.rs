//! HLK-LD2450 teacher/reference ingestion for the guided D6 calibration flow.
//!
//! This module never contributes radar coordinates to WiFi prediction. During
//! calibration it selects accessible discrete zones and gates stable CSI
//! blocks. During blind evaluation it retains radar observations as separate
//! truth. The separation is explicit in the session mode and API snapshots.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::calibration_dataset::{
    self, CalibrationDatasetRecord, RadarObservation, ALIGNMENT_LIMIT_NS,
};
use super::mmwave_position_index::{MmwavePositionIndexArtifact, TrainingBlockProvenance};
use super::mmwave_position_index::{DEFAULT_ZONE_COUNT, MAX_ZONE_COUNT, MIN_ZONE_COUNT};
use super::position_artifact::{
    deterministic_pretty_json, sha256_bytes, sha256_file, signal_sha256,
    write_pretty_json_no_clobber,
};
use super::position_capture::{
    build_position_empty_reference, extract_position_feature_window, PositionCapture,
    PositionCaptureGeometry, PositionEmptyReference, PositionGridIdentity,
};
use super::position_fingerprint::{
    FingerprintPosition, PositionFingerprintConfig, PositionFingerprintModel,
    PositionFingerprintSample,
};
use super::position_live::LivePositionState;
use super::raw_csi_recording::RawCsiFrame;
use super::server_clock::HostTimestamp;

pub(crate) const MMWAVE_SCHEMA: &str = "ruview.mmwave.ld2450.v1";
pub(crate) const DEFAULT_UDP_PORT: u16 = 5010;
const STALE_AFTER_NS: u64 = 1_000_000_000;
const COVERAGE_CELL_MM: i32 = 250;
const MIN_ZONE_SEPARATION_MM: i32 = 750;
const ZONE_RADIUS_MM: i32 = 375;
const STABILITY_RADIUS_MM: i32 = 250;
const MAX_STABLE_SPEED_CM_S: i16 = 10;
const STABLE_BLOCK_NS: u64 = 5_000_000_000;
const BLOCKS_PER_ZONE: u8 = 6;
const BLIND_VISITS_PER_ZONE: u8 = 2;
const MAX_RECENT_OBSERVATIONS: usize = 256;
const RECENT_RADAR_SEQUENCE_WINDOW: usize = 16;
const REJECTION_WINDOW_NS: u64 = 25_000_000_000;
const MIN_CELL_OBSERVATIONS: u32 = 3;
const MIN_EMPTY_CALIBRATION_SECONDS: u64 = 60;
const DEFAULT_EMPTY_CALIBRATION_SECONDS: u64 = 65;
const MAX_EMPTY_CALIBRATION_SECONDS: u64 = 3_600;
const FEATURE_OFFSET_NS: u64 = 1_000_000_000;
const PREFLIGHT_WINDOW_NS: u64 = 25_000_000_000;
const PREFLIGHT_FRESH_NS: u64 = 1_000_000_000;
// Nominally 5 Hz yields 125 frames in 25 seconds. Allow a few short WiFi
// gaps while retaining the independent span, freshness, grid, source, and
// clock gates below so a real interruption still fails closed.
const PREFLIGHT_MIN_FRAMES_PER_RX: usize = 120;
const NODE_STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const TRANSPORT_METRIC_WINDOW_SAMPLES: usize = 256;
const YAW_CALIBRATION_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MeasurementMode {
    Calibration,
    Reference,
}

impl MeasurementMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Calibration => "calibration",
            Self::Reference => "reference",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CoordinateFrame {
    pub(crate) local: String,
    pub(crate) room: String,
    pub(crate) origin_x_mm: i32,
    pub(crate) origin_z_mm: i32,
    pub(crate) yaw_mdeg: i32,
    pub(crate) raw_x_inverted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RadarTarget {
    slot: u8,
    present: bool,
    x_mm: i16,
    y_mm: i16,
    pub(crate) room_x_mm: i32,
    pub(crate) room_z_mm: i32,
    pub(crate) speed_cm_s: i16,
    resolution_mm: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RadarPacket {
    schema: String,
    node_id: String,
    pub(crate) mode: MeasurementMode,
    boot_id: u32,
    sequence: u32,
    sensor_time_us: i64,
    unix_time_ms: i64,
    pub(crate) coordinate_frame: CoordinateFrame,
    targets: Vec<RadarTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LinkState {
    Disconnected,
    Stale,
    NoTarget,
    MultiTarget,
    Invalid,
    Valid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Zone {
    pub(crate) id: String,
    pub(crate) center_mm: [i32; 2],
    pub(crate) training_blocks: u8,
    pub(crate) blind_visits: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionKind {
    Calibration,
    Blind,
    MmwaveOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationPolicy {
    #[serde(default = "default_zone_count")]
    pub(crate) zone_count: usize,
    #[serde(default = "default_empty_calibration_seconds")]
    pub(crate) empty_calibration_seconds: u64,
}

impl Default for CalibrationPolicy {
    fn default() -> Self {
        Self {
            zone_count: DEFAULT_ZONE_COUNT,
            empty_calibration_seconds: DEFAULT_EMPTY_CALIBRATION_SECONDS,
        }
    }
}

fn default_zone_count() -> usize {
    DEFAULT_ZONE_COUNT
}

fn default_empty_calibration_seconds() -> u64 {
    DEFAULT_EMPTY_CALIBRATION_SECONDS
}

impl CalibrationPolicy {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if !(MIN_ZONE_COUNT..=MAX_ZONE_COUNT).contains(&self.zone_count) {
            return Err(format!("zone_count must be in {MIN_ZONE_COUNT}..={MAX_ZONE_COUNT}"));
        }
        if !(MIN_EMPTY_CALIBRATION_SECONDS..=MAX_EMPTY_CALIBRATION_SECONDS)
            .contains(&self.empty_calibration_seconds)
        {
            return Err(format!(
                "empty_calibration_seconds must be in {MIN_EMPTY_CALIBRATION_SECONDS}..={MAX_EMPTY_CALIBRATION_SECONDS}"
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionPhase {
    EmptyCalibration,
    Coverage,
    Training,
    Blind,
    MmwaveOnly,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionLifecycle {
    Active,
    Complete,
    Stopped,
    Error,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyCalibrationValidity {
    pub(crate) verdict: String,
    pub(crate) reasons: Vec<String>,
    pub(crate) outside_room_targets: u64,
    pub(crate) in_room_targets: u64,
    pub(crate) multi_target_packets: u64,
    pub(crate) invalid_packets: u64,
    pub(crate) sequence_gaps: u64,
    pub(crate) reboots: u64,
    pub(crate) radar_packets: u64,
    pub(crate) max_radar_gap_ms: u64,
    pub(crate) csi_frames: u64,
    pub(crate) duration_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MmwaveOnlyValidity {
    pub(crate) verdict: String,
    pub(crate) reasons: Vec<String>,
    pub(crate) radar_packets: u64,
    pub(crate) valid_target_packets: u64,
    pub(crate) no_target_packets: u64,
    pub(crate) outside_room_targets: u64,
    pub(crate) multi_target_packets: u64,
    pub(crate) invalid_packets: u64,
    pub(crate) sequence_gaps: u64,
    pub(crate) reboots: u64,
    pub(crate) max_radar_gap_ms: u64,
    pub(crate) duration_seconds: u64,
}

#[derive(Debug, Clone)]
struct StableCandidate {
    zone_index: usize,
    started_at_unix_ns: u64,
    started_at_monotonic_ns: u64,
    anchor_mm: [i32; 2],
}

#[derive(Debug)]
struct Session {
    id: String,
    kind: SessionKind,
    phase: SessionPhase,
    started_at_ns: u64,
    stable_candidate: Option<StableCandidate>,
    must_exit_zone: Option<usize>,
    recording_path: PathBuf,
    manifest_path: PathBuf,
    writer: BufWriter<File>,
    csi_recording_path: PathBuf,
    csi_writer: BufWriter<File>,
    aligned_samples: u64,
    rejected_samples: u64,
    empty_started_at_ns: Option<u64>,
    empty_duration_ns: u64,
    empty_frames: Vec<RawCsiFrame>,
    empty_outside_room_targets: u64,
    empty_in_room_targets: u64,
    empty_multi_target_packets: u64,
    empty_invalid_packets: u64,
    empty_sequence_gaps: u64,
    empty_reboots: u64,
    empty_radar_packets: u64,
    empty_last_radar_packet_ns: Option<u64>,
    empty_max_radar_gap_ns: u64,
    empty_validity: Option<EmptyCalibrationValidity>,
    mmwave_only_validity: Option<MmwaveOnlyValidity>,
    candidate_frames: Vec<RawCsiFrame>,
    candidate_positions_mm: Vec<[i32; 2]>,
    candidate_radar: Vec<RadarObservation>,
    empty_reference: Option<PositionEmptyReference>,
    training_blocks: Vec<TrainingBlockProvenance>,
    receiver_grids: Option<Vec<PositionGridIdentity>>,
    blind_predictions: Vec<GuidedBlindPrediction>,
    blind_truth: Vec<GuidedBlindTruth>,
    trajectory: Vec<[i32; 2]>,
    dataset_records: Vec<CalibrationDatasetRecord>,
    clock_epoch_id: String,
    io_error: Option<String>,
    mmwave_only_radar_packets: u64,
    mmwave_only_valid_target_packets: u64,
    mmwave_only_no_target_packets: u64,
    mmwave_only_outside_room_targets: u64,
    mmwave_only_multi_target_packets: u64,
    mmwave_only_invalid_packets: u64,
    mmwave_only_sequence_gaps: u64,
    mmwave_only_reboots: u64,
    mmwave_only_last_radar_packet_ns: Option<u64>,
    mmwave_only_max_radar_gap_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionManifest {
    schema_version: u16,
    id: String,
    kind: SessionKind,
    lifecycle: SessionLifecycle,
    phase: SessionPhase,
    started_at_unix_ns: u64,
    updated_at_unix_ns: u64,
    recording_path: String,
    csi_recording_path: String,
    aligned_samples: u64,
    rejected_samples: u64,
    empty_duration_seconds: Option<u64>,
    #[serde(default)]
    empty_validity: Option<EmptyCalibrationValidity>,
    #[serde(default)]
    mmwave_only_validity: Option<MmwaveOnlyValidity>,
    error: Option<String>,
}

const SESSION_MANIFEST_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum GuidedPredictionDecision {
    Position {
        point_id: String,
        coordinates_m: [f64; 3],
    },
    Unknown,
    Ambiguous,
    Insufficient,
    Uncalibrated,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidedBlindPrediction {
    visit_id: String,
    observed_at_unix_ns: u64,
    decision: GuidedPredictionDecision,
    receiver_ablation_predictions: Vec<GuidedReceiverPrediction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidedReceiverPrediction {
    rx_id: u8,
    point_id: String,
    coordinates_m: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidedBlindTruth {
    visit_id: String,
    expected_point_id: String,
    radar_coordinates_m: [f64; 3],
    prediction_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
struct GuidedPredictionsArtifact {
    schema_version: u16,
    kind: String,
    setup_sha256: String,
    index_sha256: String,
    predictions: Vec<GuidedBlindPrediction>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
struct GuidedTruthArtifact {
    schema_version: u16,
    kind: String,
    setup_sha256: String,
    predictions_sha256: String,
    items: Vec<GuidedBlindTruth>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
struct GuidedBlindReport {
    schema_version: u16,
    kind: String,
    setup_sha256: String,
    index_sha256: String,
    predictions_sha256: String,
    truth_sha256: String,
    total: usize,
    decided: usize,
    correct: usize,
    abstentions: usize,
    accuracy_decided: Option<f64>,
    median_floor_error_m: Option<f64>,
    maximum_floor_error_m: Option<f64>,
    trajectory_coverage_cells: usize,
    trajectory_median_zone_error_m: Option<f64>,
    trajectory_p95_zone_error_m: Option<f64>,
    receiver_ablation_metrics: Vec<GuidedReceiverMetrics>,
    gates: GuidedBlindGates,
    verdict: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
struct GuidedReceiverMetrics {
    rx_id: u8,
    total: usize,
    correct: usize,
    nearest_accuracy: Option<f64>,
    median_floor_error_m: Option<f64>,
    maximum_floor_error_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
struct GuidedBlindGates {
    expected_visit_count_met: bool,
    minimum_decided_count_met: bool,
    minimum_correct_count_met: bool,
    decided_accuracy_at_least_ninety_percent: bool,
    abstention_limit_met: bool,
    median_error_at_most_0_75_m: bool,
    maximum_error_at_most_1_30_m: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MmwaveStatus {
    pub(crate) cad_profile: Option<CadProfileGeometry>,
    pub(crate) cad_profile_error: Option<String>,
    pub(crate) udp_port: u16,
    pub(crate) state: LinkState,
    pub(crate) reason: String,
    pub(crate) configured: bool,
    pub(crate) setup_sealed: bool,
    pub(crate) room_dimensions_m: Option<[f64; 3]>,
    pub(crate) mounting_position_m: Option<[f64; 3]>,
    pub(crate) receiver_positions_m: Option<Vec<[f64; 3]>>,
    pub(crate) node_id: Option<String>,
    pub(crate) mode: Option<MeasurementMode>,
    pub(crate) expected_mode: Option<MeasurementMode>,
    pub(crate) boot_id: Option<u32>,
    pub(crate) sequence: Option<u32>,
    pub(crate) packet_age_ms: Option<u64>,
    pub(crate) target_count: usize,
    pub(crate) target_raw_position_mm: Option<[i16; 2]>,
    pub(crate) target_position_mm: Option<[i32; 2]>,
    pub(crate) targets: Vec<RadarTargetStatus>,
    pub(crate) packets_received: u64,
    pub(crate) packets_rejected: u64,
    pub(crate) packets_lost: u64,
    pub(crate) raw_udp_packets: u64,
    pub(crate) reject_reasons: BTreeMap<String, u64>,
    pub(crate) packets_rejected_window: u64,
    pub(crate) reject_reasons_window: BTreeMap<String, u64>,
    pub(crate) last_rejection: Option<PacketRejectionStatus>,
    pub(crate) last_sequence_gap: Option<SequenceGapStatus>,
    pub(crate) transport: MmwaveTransportStatus,
    pub(crate) reboot_count: u64,
    pub(crate) uart_bytes_received: Option<u64>,
    pub(crate) radar_frames_valid: Option<u64>,
    pub(crate) udp_packets_sent: Option<u64>,
    pub(crate) udp_send_failures: Option<u64>,
    pub(crate) udp_send_failures_window: Option<u64>,
    pub(crate) node_status_error: Option<String>,
    pub(crate) node_control: NodeControlStatus,
    pub(crate) transform: Option<CoordinateFrame>,
    pub(crate) source_transform: Option<CoordinateFrame>,
    pub(crate) yaw_calibration: Option<YawCalibrationStatus>,
    pub(crate) coverage_cells: usize,
    pub(crate) zones: Vec<Zone>,
    pub(crate) zone_count: usize,
    pub(crate) recommended_zone_id: Option<String>,
    pub(crate) session: Option<SessionStatus>,
    pub(crate) fixed_point_calibration: Option<FixedPointCalibrationStatus>,
    pub(crate) position_index_sha256: Option<String>,
    pub(crate) position_live_approved: bool,
    pub(crate) blind_report_sha256: Option<String>,
    pub(crate) blind_verdict: Option<String>,
    pub(crate) preflight: PreflightStatus,
    pub(crate) radar_preflight: PreflightStatus,
    pub(crate) fixed_point_preflight: PreflightStatus,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PacketRejectionStatus {
    pub(crate) category: String,
    pub(crate) reason: String,
    pub(crate) age_ms: u64,
    pub(crate) raw_position_mm: Option<[i16; 2]>,
    pub(crate) position_mm: Option<[i32; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KnownPointCheck {
    pub(crate) expected_position_mm: [i32; 2],
    pub(crate) observed_raw_position_mm: [i32; 2],
    pub(crate) observed_position_mm: [i32; 2],
    pub(crate) sample_count: usize,
    pub(crate) tolerance_mm: u32,
    pub(crate) median_error_mm: u32,
    pub(crate) maximum_error_mm: u32,
    pub(crate) pass: bool,
    pub(crate) transform: Option<CoordinateFrame>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RadarTargetStatus {
    pub(crate) slot: u8,
    pub(crate) raw_position_mm: [i16; 2],
    pub(crate) position_mm: [i32; 2],
    pub(crate) inside_room: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FixedPointAnchorStatus {
    pub(crate) id: String,
    pub(crate) expected_position_mm: [i32; 2],
    pub(crate) state: &'static str,
    pub(crate) check: Option<KnownPointCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FixedPointCalibrationStatus {
    pub(crate) state: &'static str,
    pub(crate) current_anchor_id: Option<String>,
    pub(crate) anchors: Vec<FixedPointAnchorStatus>,
    pub(crate) yaw_calibration: Option<YawCalibrationStatus>,
}

#[derive(Debug, Clone)]
struct FixedPointAnchor {
    id: String,
    expected_position_mm: [i32; 2],
    check: Option<KnownPointCheck>,
}

#[derive(Debug, Clone)]
struct FixedPointCalibration {
    current_index: usize,
    anchors: Vec<FixedPointAnchor>,
    yaw_calibration: Option<YawCalibrationStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct YawCalibrationAnchorStatus {
    pub(crate) id: String,
    pub(crate) before_error_mm: u32,
    pub(crate) after_error_mm: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct YawCalibrationStatus {
    pub(crate) schema_version: u16,
    pub(crate) source_kind: String,
    pub(crate) source_id: String,
    pub(crate) source_sha256: String,
    pub(crate) created_at_unix_ns: u64,
    pub(crate) base_transform: CoordinateFrame,
    pub(crate) base_yaw_mdeg: i32,
    pub(crate) previous_effective_yaw_mdeg: i32,
    pub(crate) optimized_yaw_mdeg: i32,
    pub(crate) correction_mdeg: i32,
    pub(crate) point_count: usize,
    pub(crate) before_rms_error_mm: u32,
    pub(crate) after_rms_error_mm: u32,
    pub(crate) before_median_error_mm: u32,
    pub(crate) after_median_error_mm: u32,
    pub(crate) before_maximum_error_mm: u32,
    pub(crate) after_maximum_error_mm: u32,
    pub(crate) improvement_percent: f64,
    pub(crate) applied: bool,
    pub(crate) persisted: bool,
    pub(crate) anchors: Vec<YawCalibrationAnchorStatus>,
}

#[derive(Debug, Clone)]
struct RecentRadarObservation {
    observed_at_ns: u64,
    raw_position_mm: [i32; 2],
    room_position_mm: [i32; 2],
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SequenceGapStatus {
    pub(crate) expected_sequence: u32,
    pub(crate) received_sequence: u32,
    pub(crate) missing_packets: u64,
    pub(crate) age_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MmwaveTransportStatus {
    pub(crate) raw_udp_packets: u64,
    pub(crate) queue_length: usize,
    pub(crate) queue_peak: usize,
    pub(crate) last_receive_to_process_delay_ms: Option<u64>,
    pub(crate) max_receive_to_process_delay_ms: Option<u64>,
    pub(crate) last_processing_duration_ms: Option<u64>,
    pub(crate) max_processing_duration_ms: Option<u64>,
    pub(crate) duplicate_packets: u64,
    pub(crate) sequence_discards: u64,
    pub(crate) window_samples: usize,
    pub(crate) valid_samples: usize,
    pub(crate) valid_rate_hz: Option<f64>,
    pub(crate) inter_arrival_median_ms: Option<u64>,
    pub(crate) inter_arrival_p95_ms: Option<u64>,
    pub(crate) receive_to_process_median_ms: Option<u64>,
    pub(crate) receive_to_process_p95_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct NodeControlStatus {
    pub(crate) url_configured: bool,
    pub(crate) token_configured: bool,
    pub(crate) reachable: Option<bool>,
    pub(crate) last_success_age_ms: Option<u64>,
    pub(crate) last_error_kind: Option<String>,
    pub(crate) last_error: Option<String>,
}

/// Saved CAD geometry for unsealed radar preview, never a calibration seal.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CadProfileGeometry {
    pub(crate) revision_id: String,
    pub(crate) profile_sha256: String,
    pub(crate) room_dimensions_m: [f64; 3],
    pub(crate) mounting_position_m: [f64; 3],
    pub(crate) tx_position_m: Option<[f64; 3]>,
    pub(crate) receiver_positions_m: Vec<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) yaw_mdeg: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) raw_x_inverted: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PreflightGate {
    id: &'static str,
    pass: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PreflightStatus {
    ready: bool,
    observation_window_ms: u64,
    gates: Vec<PreflightGate>,
}

#[derive(Debug, Clone)]
struct CsiPreflightObservation {
    monotonic_ns: u64,
    grid: PositionGridIdentity,
    source_bound: bool,
    clock_epoch_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RadarSequenceDisposition {
    Fresh,
    Duplicate,
}

#[derive(Debug, Clone, Deserialize)]
struct NodeStatusResponse {
    diagnostics: NodeDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketRejectCategory {
    InvalidJson,
    PacketShape,
    UnexpectedNode,
    NodeChanged,
    SequenceOutOfOrder,
    ModeMismatch,
    TransformMismatch,
    RoomBounds,
}

impl PacketRejectCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidJson => "invalid_json",
            Self::PacketShape => "packet_shape",
            Self::UnexpectedNode => "unexpected_node",
            Self::NodeChanged => "node_changed",
            Self::SequenceOutOfOrder => "sequence_out_of_order",
            Self::ModeMismatch => "mode_mismatch",
            Self::TransformMismatch => "transform_mismatch",
            Self::RoomBounds => "room_bounds",
        }
    }
}

#[derive(Debug, Clone)]
struct PacketValidationError {
    category: PacketRejectCategory,
    reason: String,
}

impl PacketValidationError {
    fn new(category: PacketRejectCategory, reason: impl Into<String>) -> Self {
        Self {
            category,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct LastPacketRejection {
    category: String,
    reason: String,
    raw_position_mm: Option<[i16; 2]>,
    position_mm: Option<[i32; 2]>,
    at_monotonic_ns: u64,
}

#[derive(Debug, Clone)]
struct LastSequenceGap {
    expected_sequence: u32,
    received_sequence: u32,
    missing_packets: u64,
    at_monotonic_ns: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeDiagnostics {
    pub(crate) uart_bytes_received: u64,
    pub(crate) radar_frames_valid: u64,
    pub(crate) udp_packets_sent: u64,
    pub(crate) udp_send_failures: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct NodeDiagnosticsWindow {
    pub(crate) diagnostics: NodeDiagnostics,
    pub(crate) uart_bytes_delta: u64,
    pub(crate) radar_frames_delta: u64,
    pub(crate) udp_packets_sent_delta: u64,
    pub(crate) udp_send_failures_delta: u64,
}

#[derive(Debug, Default)]
pub(crate) struct MmwaveTransportMetrics {
    raw_udp_packets: AtomicU64,
    queue_length: AtomicUsize,
    queue_peak: AtomicUsize,
    last_receive_to_process_delay_ms: AtomicU64,
    max_receive_to_process_delay_ms: AtomicU64,
    last_processing_duration_ms: AtomicU64,
    max_processing_duration_ms: AtomicU64,
    duplicate_packets: AtomicU64,
    sequence_discards: AtomicU64,
    window: Mutex<MmwaveTransportWindow>,
}

#[derive(Debug, Default)]
struct MmwaveTransportWindow {
    samples: VecDeque<MmwaveTransportSample>,
}

#[derive(Debug)]
struct MmwaveTransportSample {
    received_at_monotonic_ns: u64,
    receive_to_process_delay_ms: u64,
    accepted: bool,
}

#[derive(Debug, Default)]
struct MmwaveTransportWindowSnapshot {
    samples: usize,
    valid_samples: usize,
    valid_rate_hz: Option<f64>,
    inter_arrival_median_ms: Option<u64>,
    inter_arrival_p95_ms: Option<u64>,
    receive_to_process_median_ms: Option<u64>,
    receive_to_process_p95_ms: Option<u64>,
}

impl MmwaveTransportWindow {
    fn record(&mut self, sample: MmwaveTransportSample) {
        self.samples.push_back(sample);
        if self.samples.len() > TRANSPORT_METRIC_WINDOW_SAMPLES {
            self.samples.pop_front();
        }
    }

    fn snapshot(&self) -> MmwaveTransportWindowSnapshot {
        let mut inter_arrivals_ms: Vec<u64> = self
            .samples
            .iter()
            .zip(self.samples.iter().skip(1))
            .map(|(previous, current)| {
                current
                    .received_at_monotonic_ns
                    .saturating_sub(previous.received_at_monotonic_ns)
                    / 1_000_000
            })
            .collect();
        let mut receive_to_process_ms: Vec<u64> = self
            .samples
            .iter()
            .map(|sample| sample.receive_to_process_delay_ms)
            .collect();
        let valid_times: Vec<u64> = self
            .samples
            .iter()
            .filter(|sample| sample.accepted)
            .map(|sample| sample.received_at_monotonic_ns)
            .collect();
        let valid_rate_hz = valid_times.first().zip(valid_times.last()).and_then(
            |(first, last)| {
                let span_ns = last.saturating_sub(*first);
                (valid_times.len() >= 2 && span_ns > 0).then(|| {
                    (valid_times.len() - 1) as f64 * 1_000_000_000.0 / span_ns as f64
                })
            },
        );
        MmwaveTransportWindowSnapshot {
            samples: self.samples.len(),
            valid_samples: valid_times.len(),
            valid_rate_hz,
            inter_arrival_median_ms: percentile_ms(&mut inter_arrivals_ms, 50),
            inter_arrival_p95_ms: percentile_ms(&mut inter_arrivals_ms, 95),
            receive_to_process_median_ms: percentile_ms(&mut receive_to_process_ms, 50),
            receive_to_process_p95_ms: percentile_ms(&mut receive_to_process_ms, 95),
        }
    }
}

fn percentile_ms(values: &mut [u64], percentile: usize) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let index = values
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(values.len() - 1);
    Some(values[index])
}

impl MmwaveTransportMetrics {
    pub(crate) fn note_received(&self) {
        self.raw_udp_packets.fetch_add(1, Ordering::Relaxed);
        let queue_length = self.queue_length.fetch_add(1, Ordering::Relaxed) + 1;
        let mut peak = self.queue_peak.load(Ordering::Relaxed);
        while queue_length > peak {
            match self.queue_peak.compare_exchange_weak(
                peak,
                queue_length,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => peak = observed,
            }
        }
    }

    pub(crate) fn note_dequeued(&self) {
        let _ = self.queue_length.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |length| Some(length.saturating_sub(1)),
        );
    }

    pub(crate) fn set_queue_length(&self, queue_length: usize) {
        self.queue_length.store(queue_length, Ordering::Relaxed);
        let mut peak = self.queue_peak.load(Ordering::Relaxed);
        while queue_length > peak {
            match self.queue_peak.compare_exchange_weak(
                peak,
                queue_length,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => peak = observed,
            }
        }
    }

    pub(crate) fn note_duplicate(&self) {
        self.duplicate_packets.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_sequence_discard(&self) {
        self.sequence_discards.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_processed(
        &self,
        received_at_monotonic_ns: u64,
        processed_at_monotonic_ns: u64,
        processing_duration_ms: u64,
        accepted: bool,
    ) {
        let queue_delay_ms = processed_at_monotonic_ns
            .saturating_sub(received_at_monotonic_ns)
            / 1_000_000;
        self.last_receive_to_process_delay_ms
            .store(queue_delay_ms, Ordering::Relaxed);
        update_atomic_max(&self.max_receive_to_process_delay_ms, queue_delay_ms);
        self.last_processing_duration_ms
            .store(processing_duration_ms, Ordering::Relaxed);
        update_atomic_max(&self.max_processing_duration_ms, processing_duration_ms);
        self.window
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record(MmwaveTransportSample {
                received_at_monotonic_ns,
                receive_to_process_delay_ms: queue_delay_ms,
                accepted,
            });
    }

    fn snapshot(&self) -> MmwaveTransportStatus {
        let window = self
            .window
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot();
        MmwaveTransportStatus {
            raw_udp_packets: self.raw_udp_packets.load(Ordering::Relaxed),
            queue_length: self.queue_length.load(Ordering::Relaxed),
            queue_peak: self.queue_peak.load(Ordering::Relaxed),
            last_receive_to_process_delay_ms: optional_counter(
                self.last_receive_to_process_delay_ms.load(Ordering::Relaxed),
            ),
            max_receive_to_process_delay_ms: optional_counter(
                self.max_receive_to_process_delay_ms.load(Ordering::Relaxed),
            ),
            last_processing_duration_ms: optional_counter(
                self.last_processing_duration_ms.load(Ordering::Relaxed),
            ),
            max_processing_duration_ms: optional_counter(
                self.max_processing_duration_ms.load(Ordering::Relaxed),
            ),
            duplicate_packets: self.duplicate_packets.load(Ordering::Relaxed),
            sequence_discards: self.sequence_discards.load(Ordering::Relaxed),
            window_samples: window.samples,
            valid_samples: window.valid_samples,
            valid_rate_hz: window.valid_rate_hz,
            inter_arrival_median_ms: window.inter_arrival_median_ms,
            inter_arrival_p95_ms: window.inter_arrival_p95_ms,
            receive_to_process_median_ms: window.receive_to_process_median_ms,
            receive_to_process_p95_ms: window.receive_to_process_p95_ms,
        }
    }
}

fn optional_counter(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn update_atomic_max(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange_weak(
            current,
            value,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

impl MmwaveStatus {
    pub(crate) fn preflight_ready(&self) -> bool {
        self.preflight.ready
    }

    pub(crate) fn radar_preflight_ready(&self) -> bool {
        self.radar_preflight.ready
    }

    pub(crate) fn attach_node_diagnostics(&mut self, diagnostics: Result<NodeDiagnostics, String>) {
        let diagnostics = diagnostics.map(|diagnostics| NodeDiagnosticsWindow {
            uart_bytes_delta: diagnostics.uart_bytes_received,
            radar_frames_delta: diagnostics.radar_frames_valid,
            udp_packets_sent_delta: diagnostics.udp_packets_sent,
            udp_send_failures_delta: diagnostics.udp_send_failures,
            diagnostics,
        });
        self.attach_node_diagnostics_window(diagnostics);
    }

    pub(crate) fn attach_node_diagnostics_window(
        &mut self,
        diagnostics: Result<NodeDiagnosticsWindow, String>,
    ) {
        match diagnostics {
            Ok(window) => {
                self.uart_bytes_received = Some(window.diagnostics.uart_bytes_received);
                self.radar_frames_valid = Some(window.diagnostics.radar_frames_valid);
                self.udp_packets_sent = Some(window.diagnostics.udp_packets_sent);
                self.udp_send_failures = Some(window.diagnostics.udp_send_failures);
                self.udp_send_failures_window = Some(window.udp_send_failures_delta);
                self.node_status_error = None;
                self.node_control.reachable = Some(true);
                self.node_control.last_success_age_ms = Some(0);
                self.node_control.last_error_kind = None;
                self.node_control.last_error = None;
                self.preflight.gates.push(PreflightGate {
                    id: "node_diagnostics_streaming",
                    pass: window.uart_bytes_delta > 0
                        && window.radar_frames_delta > 0
                        && window.udp_packets_sent_delta > 0
                        && window.udp_send_failures_delta == 0,
                    detail: format!(
                        "uart={} valid={} udp_sent={} udp_failures={} (window deltas: uart={} valid={} udp_sent={} udp_failures={})",
                        window.diagnostics.uart_bytes_received,
                        window.diagnostics.radar_frames_valid,
                        window.diagnostics.udp_packets_sent,
                        window.diagnostics.udp_send_failures,
                        window.uart_bytes_delta,
                        window.radar_frames_delta,
                        window.udp_packets_sent_delta,
                        window.udp_send_failures_delta
                    ),
                });
            }
            Err(error) => {
                self.node_status_error = Some(error.clone());
                self.node_control.reachable = Some(false);
                self.node_control.last_error_kind = Some(classify_node_status_error(&error).to_string());
                self.node_control.last_error = Some(error.clone());
                self.preflight.gates.push(PreflightGate {
                    id: "node_diagnostics_streaming",
                    pass: false,
                    detail: error,
                });
            }
        }
        self.preflight.ready = self.preflight.gates.iter().all(|gate| gate.pass);
    }
}

pub(crate) fn classify_node_status_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("invalid mmwave node status")
        || lower.contains("invalid json")
        || lower.contains("json")
    {
        "invalid_json"
    } else if lower.contains("http ") || lower.contains("returned http") {
        "http_error"
    } else {
        "unreachable"
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionStatus {
    id: String,
    kind: SessionKind,
    phase: SessionPhase,
    lifecycle: SessionLifecycle,
    started_at_unix_ns: u64,
    recording_path: String,
    csi_recording_path: String,
    aligned_samples: u64,
    rejected_samples: u64,
    empty_duration_seconds: Option<u64>,
    empty_remaining_seconds: Option<u64>,
    empty_validity: Option<EmptyCalibrationValidity>,
    mmwave_only_validity: Option<MmwaveOnlyValidity>,
    error: Option<String>,
    next_instruction: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NodeControl {
    pub(crate) base_url: String,
    pub(crate) bearer_token: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExperimentContext {
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) server_version: String,
    pub(crate) geometry: PositionCaptureGeometry,
}

#[derive(Debug, Clone)]
pub(crate) struct ExpectedNode {
    pub(crate) node_id: String,
    pub(crate) transform: CoordinateFrame,
    pub(crate) mounting_position_m: Option<[f64; 3]>,
}

#[derive(Debug)]
pub(crate) struct MmwaveManager {
    cad_profile: Option<CadProfileGeometry>,
    cad_profile_error: Option<String>,
    udp_port: u16,
    room_dimensions_mm: Option<[i32; 2]>,
    control: Option<NodeControl>,
    expected_node: Option<ExpectedNode>,
    experiment: Option<ExperimentContext>,
    state: LinkState,
    reason: String,
    expected_mode: Option<MeasurementMode>,
    node_id: Option<String>,
    mode: Option<MeasurementMode>,
    boot_id: Option<u32>,
    sequence: Option<u32>,
    recent_sequences: VecDeque<u32>,
    last_transport_packet_ns: Option<u64>,
    last_packet_ns: Option<u64>,
    last_transform: Option<CoordinateFrame>,
    target_count: usize,
    target_raw_position_mm: Option<[i16; 2]>,
    target_position_mm: Option<[i32; 2]>,
    targets: Vec<RadarTargetStatus>,
    packets_received: u64,
    packets_rejected: u64,
    packets_lost: u64,
    reject_reasons: BTreeMap<String, u64>,
    recent_rejections: VecDeque<(u64, String)>,
    last_rejection: Option<LastPacketRejection>,
    last_sequence_gap: Option<LastSequenceGap>,
    reboot_count: u64,
    last_radar_sequence_fault_ns: Option<u64>,
    last_radar_reboot_ns: Option<u64>,
    coverage: BTreeMap<(i32, i32), u32>,
    zones: Vec<Zone>,
    last_csi_ns: [Option<u64>; 4],
    session: Option<Session>,
    fixed_point_calibration: Option<FixedPointCalibration>,
    recent_observations: VecDeque<RecentRadarObservation>,
    yaw_calibration: Option<YawCalibrationStatus>,
    pending_index: Option<(PathBuf, String)>,
    blind_index: Option<MmwavePositionIndexArtifact>,
    position_index_sha256: Option<String>,
    current_wifi_prediction: GuidedPredictionDecision,
    blind_report_sha256: Option<String>,
    blind_verdict: Option<String>,
    candidate_requires_validation: bool,
    zone_count: usize,
    rx_preflight: [VecDeque<CsiPreflightObservation>; 4],
    csi_clock_rejections: u64,
    clock_epoch_id: Option<String>,
    transport_metrics: Arc<MmwaveTransportMetrics>,
    node_url_configured: bool,
    node_token_configured: bool,
    restored_session: Option<SessionStatus>,
}

impl MmwaveManager {
    pub(crate) fn new(
        udp_port: u16,
        room_dimensions_m: Option<[f64; 3]>,
        control: Option<NodeControl>,
        expected_node: Option<ExpectedNode>,
        experiment: Option<ExperimentContext>,
    ) -> Self {
        let room_dimensions_mm = room_dimensions_m.and_then(|dimensions| {
            let length = metres_to_mm(dimensions[0])?;
            let width = metres_to_mm(dimensions[2])?;
            (length > 0 && width > 0).then_some([length, width])
        });
        let node_control_configured = control.is_some();
        Self {
            cad_profile: None,
            cad_profile_error: None,
            udp_port,
            room_dimensions_mm,
            control,
            expected_node,
            experiment,
            state: LinkState::Disconnected,
            reason: "No mmWave packet has been received.".to_string(),
            expected_mode: None,
            node_id: None,
            mode: None,
            boot_id: None,
            sequence: None,
            recent_sequences: VecDeque::new(),
            last_transport_packet_ns: None,
            last_packet_ns: None,
            last_transform: None,
            target_count: 0,
            target_raw_position_mm: None,
            target_position_mm: None,
            targets: Vec::new(),
            packets_received: 0,
            packets_rejected: 0,
            packets_lost: 0,
            reject_reasons: BTreeMap::new(),
            recent_rejections: VecDeque::new(),
            last_rejection: None,
            last_sequence_gap: None,
            reboot_count: 0,
            last_radar_sequence_fault_ns: None,
            last_radar_reboot_ns: None,
            coverage: BTreeMap::new(),
            zones: Vec::new(),
            last_csi_ns: [None; 4],
            session: None,
            fixed_point_calibration: None,
            recent_observations: VecDeque::new(),
            yaw_calibration: None,
            pending_index: None,
            blind_index: None,
            position_index_sha256: None,
            current_wifi_prediction: GuidedPredictionDecision::Uncalibrated,
            blind_report_sha256: None,
            blind_verdict: None,
            candidate_requires_validation: false,
            zone_count: DEFAULT_ZONE_COUNT,
            rx_preflight: std::array::from_fn(|_| VecDeque::new()),
            csi_clock_rejections: 0,
            clock_epoch_id: None,
            transport_metrics: Arc::new(MmwaveTransportMetrics::default()),
            node_url_configured: node_control_configured,
            node_token_configured: node_control_configured,
            restored_session: None,
        }
    }

    pub(crate) fn control(&self) -> Option<NodeControl> {
        self.control.clone()
    }

    pub(crate) fn discovered_control(&mut self, url: Option<String>, token: Option<String>) {
        self.node_url_configured = url.is_some();
        self.node_token_configured = token.is_some();
        self.control = url.zip(token).map(|(base_url, bearer_token)| NodeControl { base_url, bearer_token });
    }

    pub(crate) fn apply_cad_profile(&mut self, profile: &super::experiment::SetupProfile) {
        // A running or sealed experiment keeps its exact acquisition geometry.
        // Saving remains possible, but cannot silently rewrite that experiment.
        if self.expected_node.is_some() || self.experiment.is_some() || self.session.is_some() {
            self.cad_profile_error = Some("Profil gespeichert. Aktiver Versuch behält seine versiegelte Geometrie; für die neue Profilversion ist ein neues Setup nötig.".to_string());
            return;
        }
        let room = serde_json::from_value::<[f64; 3]>(profile.document["room_dimensions_m"].clone());
        let mounting = serde_json::from_value::<[f64; 3]>(profile.document["mmwave"]["mounting_position_m"].clone());
        let (Ok(room), Ok(mounting)) = (room, mounting) else {
            self.cad_profile_error = Some("CAD-Profil enthält keine vollständige Raum- und mmWave-Geometrie.".to_string());
            return;
        };
        let tx_position_m = serde_json::from_value::<[f64; 3]>(
            profile.document["transmitter"]["position_m"].clone(),
        )
        .ok();
        let receiver_positions_m = profile
            .document
            .get("receivers")
            .and_then(serde_json::Value::as_array)
            .map(|receivers| {
                receivers
                    .iter()
                    .filter_map(|receiver| {
                        serde_json::from_value::<[f64; 3]>(receiver["position_m"].clone()).ok()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !room.iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 100.0)
            || !mounting.iter().all(|v| v.is_finite() && v.abs() <= 100.0) {
            self.cad_profile_error = Some("CAD-Geometrie liegt außerhalb des unterstützten Bereichs.".to_string());
            return;
        }
        let orientation = (|| {
            let mmwave = profile
                .document
                .get("mmwave")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| "CAD-Profil enthält keine mmWave-Ausrichtung.".to_string())?;
            let yaw_mdeg = match mmwave.get("yaw_mdeg") {
                None => None,
                Some(value) => {
                    let value = value
                        .as_i64()
                        .and_then(|value| i32::try_from(value).ok())
                        .filter(|value| (-360000..=360000).contains(value))
                        .ok_or_else(|| "mmwave.yaw_mdeg muss eine ganze Zahl zwischen -360000 und 360000 sein.".to_string())?;
                    Some(value)
                }
            };
            let raw_x_inverted = mmwave
                .get("raw_x_inverted")
                .map(|value| {
                    value
                        .as_bool()
                        .ok_or_else(|| "mmwave.raw_x_inverted muss ein boolescher Wert sein.".to_string())
                })
                .transpose()?;
            Ok::<_, String>((yaw_mdeg, raw_x_inverted))
        })();
        let (yaw_mdeg, raw_x_inverted) = match orientation {
            Ok(orientation) => orientation,
            Err(error) => {
                self.cad_profile_error = Some(error);
                return;
            }
        };
        if self.cad_profile.as_ref().is_some_and(|cad| cad.profile_sha256 == profile.profile_sha256) {
            return;
        }
        self.room_dimensions_mm = Some([(room[0] * 1000.0).round() as i32, (room[2] * 1000.0).round() as i32]);
        self.cad_profile = Some(CadProfileGeometry {
            revision_id: profile.revision_id.clone(), profile_sha256: profile.profile_sha256.clone(),
            room_dimensions_m: room, mounting_position_m: mounting,
            tx_position_m, receiver_positions_m,
            yaw_mdeg, raw_x_inverted,
        });
        self.cad_profile_error = None;
        self.last_transform = None;
        self.target_position_mm = None;
        self.target_raw_position_mm = None;
        self.targets.clear();
        self.last_rejection = None;
        self.fixed_point_calibration = None;
        self.yaw_calibration = None;
        self.coverage.clear();
        self.reset_stability();
        self.state = LinkState::Disconnected;
        self.reason = "CAD-Profil übernommen; warte auf ein neues Radar-Paket.".to_string();
    }

    pub(crate) fn transport_metrics(&self) -> Arc<MmwaveTransportMetrics> {
        self.transport_metrics.clone()
    }

    pub(crate) fn check_known_point(
        &self,
        expected_position_mm: [i32; 2],
        tolerance_mm: u32,
        window_ms: u64,
        now_ns: u64,
    ) -> Result<KnownPointCheck, String> {
        let window_ns = window_ms.saturating_mul(1_000_000);
        let positions: Vec<[i32; 2]> = self
            .recent_observations
            .iter()
            .filter(|observation| {
                now_ns.saturating_sub(observation.observed_at_ns) <= window_ns
            })
            .map(|observation| observation.room_position_mm)
            .collect();
        let raw_positions: Vec<[i32; 2]> = self
            .recent_observations
            .iter()
            .filter(|observation| {
                now_ns.saturating_sub(observation.observed_at_ns) <= window_ns
            })
            .map(|observation| observation.raw_position_mm)
            .collect();
        let observed_position_mm = median_position_mm(&positions)
            .ok_or_else(|| "no fresh single-target radar observations in the check window".to_string())?;
        let observed_raw_position_mm = median_position_mm(&raw_positions)
            .ok_or_else(|| "no fresh single-target radar observations in the check window".to_string())?;
        let error_for = |position: [i32; 2]| -> u32 {
            let dx = i64::from(position[0]) - i64::from(expected_position_mm[0]);
            let dz = i64::from(position[1]) - i64::from(expected_position_mm[1]);
            (((dx * dx + dz * dz) as f64).sqrt().round() as u64).min(u64::from(u32::MAX)) as u32
        };
        let errors: Vec<u32> = positions.iter().copied().map(error_for).collect();
        let mut sorted_errors = errors.clone();
        sorted_errors.sort_unstable();
        let median_error_mm = sorted_errors[sorted_errors.len() / 2];
        let maximum_error_mm = sorted_errors.iter().copied().max().unwrap_or(0);
        Ok(KnownPointCheck {
            expected_position_mm,
            observed_raw_position_mm,
            observed_position_mm,
            sample_count: positions.len(),
            tolerance_mm,
            median_error_mm,
            maximum_error_mm,
            pass: median_error_mm <= tolerance_mm,
            transform: self.last_transform.clone(),
        })
    }

    fn fixed_point_status(&self) -> Option<FixedPointCalibrationStatus> {
        let calibration = self.fixed_point_calibration.as_ref()?;
        let complete = calibration.current_index >= calibration.anchors.len();
        Some(FixedPointCalibrationStatus {
            state: if complete { "complete" } else { "active" },
            current_anchor_id: (!complete)
                .then(|| calibration.anchors[calibration.current_index].id.clone()),
            anchors: calibration
                .anchors
                .iter()
                .enumerate()
                .map(|(index, anchor)| FixedPointAnchorStatus {
                    id: anchor.id.clone(),
                    expected_position_mm: anchor.expected_position_mm,
                    state: if anchor.check.is_some() {
                        "complete"
                    } else if index == calibration.current_index {
                        "current"
                    } else {
                        "pending"
                    },
                    check: anchor.check.clone(),
                })
                .collect(),
            yaw_calibration: calibration.yaw_calibration.clone(),
        })
    }

    pub(crate) fn validate_fixed_point_start(&self, now_ns: u64) -> Result<(), String> {
        if self.session.is_some() {
            return Err("an mmWave session is already active".to_string());
        }
        let (_, receivers) = self.fixed_point_geometry().ok_or_else(|| {
            "an active CAD profile or sealed setup with TX and RX1 through RX4 is required"
                .to_string()
        })?;
        if receivers.len() != 4 {
            return Err("fixed-point calibration requires RX1 through RX4".to_string());
        }
        let preflight = self.fixed_point_preflight(now_ns);
        if !preflight.ready {
            let blockers = preflight
                .gates
                .iter()
                .filter(|gate| !gate.pass)
                .map(|gate| gate.id)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("mmWave radar preflight is not ready: {blockers}"));
        }
        Ok(())
    }

    fn fixed_point_geometry(&self) -> Option<([f64; 3], Vec<[f64; 3]>)> {
        if let Some(experiment) = &self.experiment {
            return Some((
                experiment.geometry.tx_position_m,
                experiment.geometry.rx_positions_m.clone(),
            ));
        }
        let cad = self.cad_profile.as_ref()?;
        Some((cad.tx_position_m?, cad.receiver_positions_m.clone()))
    }

    pub(crate) fn start_fixed_point_calibration(
        &mut self,
        now_ns: u64,
    ) -> Result<FixedPointCalibrationStatus, String> {
        self.validate_fixed_point_start(now_ns)?;
        let (tx, receivers) = self
            .fixed_point_geometry()
            .expect("validated fixed-point geometry");
        let mut anchors = receivers
            .iter()
            .enumerate()
            .map(|(index, position)| FixedPointAnchor {
                id: format!("RX{}", index + 1),
                expected_position_mm: [
                    (position[0] * 1_000.0).round() as i32,
                    (position[2] * 1_000.0).round() as i32,
                ],
                check: None,
            })
            .collect::<Vec<_>>();
        anchors.push(FixedPointAnchor {
            id: "TX".to_string(),
            expected_position_mm: [
                (tx[0] * 1_000.0).round() as i32,
                (tx[2] * 1_000.0).round() as i32,
            ],
            check: None,
        });
        self.fixed_point_calibration = Some(FixedPointCalibration {
            current_index: 0,
            anchors,
            yaw_calibration: None,
        });
        self.expected_mode = Some(MeasurementMode::Calibration);
        self.fixed_point_status()
            .ok_or_else(|| "could not start fixed-point calibration".to_string())
    }

    pub(crate) fn check_current_fixed_point(
        &mut self,
        tolerance_mm: u32,
        window_ms: u64,
        now_ns: u64,
    ) -> Result<FixedPointCalibrationStatus, String> {
        let (current_index, expected_position_mm) = {
            let calibration = self
                .fixed_point_calibration
                .as_ref()
                .ok_or_else(|| "fixed-point calibration is not active".to_string())?;
            if calibration.current_index >= calibration.anchors.len() {
                return Err("fixed-point calibration is already complete".to_string());
            }
            (
                calibration.current_index,
                calibration.anchors[calibration.current_index].expected_position_mm,
            )
        };
        let check = self.check_known_point(
            expected_position_mm,
            tolerance_mm,
            window_ms,
            now_ns,
        )?;
        let complete = {
            let calibration = self
                .fixed_point_calibration
                .as_mut()
                .expect("fixed-point calibration remained active");
            calibration.anchors[current_index].check = Some(check);
            calibration.current_index += 1;
            calibration.current_index >= calibration.anchors.len()
        };
        if complete {
            let yaw_calibration = self.optimize_fixed_point_yaw(now_unix_ns())?;
            self.yaw_calibration = Some(yaw_calibration.clone());
            self.fixed_point_calibration
                .as_mut()
                .expect("fixed-point calibration remained active")
                .yaw_calibration = Some(yaw_calibration);
        }
        self.fixed_point_status()
            .ok_or_else(|| "fixed-point calibration disappeared".to_string())
    }

    fn calibration_source(&self) -> Option<(&'static str, String, String)> {
        if let Some(experiment) = &self.experiment {
            return Some((
                "setup",
                experiment.setup_id.clone(),
                experiment.setup_sha256.clone(),
            ));
        }
        self.cad_profile.as_ref().map(|profile| {
            (
                "profile",
                profile.revision_id.clone(),
                profile.profile_sha256.clone(),
            )
        })
    }

    fn optimize_fixed_point_yaw(
        &self,
        created_at_unix_ns: u64,
    ) -> Result<YawCalibrationStatus, String> {
        let calibration = self
            .fixed_point_calibration
            .as_ref()
            .ok_or_else(|| "fixed-point calibration is not active".to_string())?;
        let points = calibration
            .anchors
            .iter()
            .map(|anchor| {
                anchor
                    .check
                    .as_ref()
                    .map(|check| (anchor.id.clone(), anchor.expected_position_mm, check))
                    .ok_or_else(|| format!("fixed point {} has no observation", anchor.id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if points.len() < 2 {
            return Err("yaw calibration requires at least two measured reference points".to_string());
        }
        let base_transform = points
            .first()
            .and_then(|(_, _, check)| check.transform.clone())
            .or_else(|| self.last_transform.clone())
            .ok_or_else(|| "yaw calibration requires a known source transform".to_string())?;
        let (source_kind, source_id, source_sha256) = self
            .calibration_source()
            .ok_or_else(|| "yaw calibration requires a saved profile or sealed setup".to_string())?;
        let previous_effective_yaw_mdeg = self
            .yaw_calibration
            .as_ref()
            .filter(|calibration| {
                calibration.source_kind == source_kind
                    && calibration.source_id == source_id
                    && calibration.source_sha256 == source_sha256
                    && calibration.base_transform == base_transform
            })
            .map(|calibration| calibration.optimized_yaw_mdeg)
            .unwrap_or(base_transform.yaw_mdeg);

        let mut dot = 0.0_f64;
        let mut cross = 0.0_f64;
        for (_, expected, check) in &points {
            let right = f64::from(check.observed_raw_position_mm[0])
                * if base_transform.raw_x_inverted { -1.0 } else { 1.0 };
            let forward = f64::from(check.observed_raw_position_mm[1]);
            let expected_x = f64::from(expected[0] - base_transform.origin_x_mm);
            let expected_z = f64::from(expected[1] - base_transform.origin_z_mm);
            dot += forward * expected_x + right * expected_z;
            cross += forward * expected_z - right * expected_x;
        }
        if dot.abs() < f64::EPSILON && cross.abs() < f64::EPSILON {
            return Err("reference points cannot determine a yaw angle".to_string());
        }
        let candidate_yaw_mdeg = normalize_yaw_mdeg(
            (cross.atan2(dot).to_degrees() * 1_000.0).round() as i32,
        );
        let before_errors = yaw_errors_mm(
            &points,
            &base_transform,
            previous_effective_yaw_mdeg,
        );
        let candidate_errors = yaw_errors_mm(&points, &base_transform, candidate_yaw_mdeg);
        let before_rms = rms_error_mm(&before_errors);
        let candidate_rms = rms_error_mm(&candidate_errors);
        let applied = candidate_rms + f64::EPSILON < before_rms;
        let optimized_yaw_mdeg = if applied {
            candidate_yaw_mdeg
        } else {
            previous_effective_yaw_mdeg
        };
        let after_errors = if applied {
            candidate_errors
        } else {
            before_errors.clone()
        };
        let after_rms = rms_error_mm(&after_errors);
        let before_summary = error_summary_mm(&before_errors);
        let after_summary = error_summary_mm(&after_errors);
        let anchors = points
            .iter()
            .enumerate()
            .map(|(index, (id, _, _))| YawCalibrationAnchorStatus {
                id: id.clone(),
                before_error_mm: before_errors[index].round().max(0.0) as u32,
                after_error_mm: after_errors[index].round().max(0.0) as u32,
            })
            .collect();

        Ok(YawCalibrationStatus {
            schema_version: YAW_CALIBRATION_SCHEMA_VERSION,
            source_kind: source_kind.to_string(),
            source_id,
            source_sha256,
            created_at_unix_ns,
            base_transform: base_transform.clone(),
            base_yaw_mdeg: base_transform.yaw_mdeg,
            previous_effective_yaw_mdeg,
            optimized_yaw_mdeg,
            correction_mdeg: normalize_signed_yaw_mdeg(
                optimized_yaw_mdeg - base_transform.yaw_mdeg,
            ),
            point_count: points.len(),
            before_rms_error_mm: before_rms.round().max(0.0) as u32,
            after_rms_error_mm: after_rms.round().max(0.0) as u32,
            before_median_error_mm: before_summary.0,
            after_median_error_mm: after_summary.0,
            before_maximum_error_mm: before_summary.1,
            after_maximum_error_mm: after_summary.1,
            improvement_percent: if before_rms > 0.0 {
                ((before_rms - after_rms) / before_rms * 100.0).max(0.0)
            } else {
                0.0
            },
            applied,
            persisted: false,
            anchors,
        })
    }

    fn effective_transform(&self) -> Option<CoordinateFrame> {
        let mut transform = self.last_transform.clone()?;
        if let Some(calibration) = &self.yaw_calibration {
            if calibration.base_transform == transform {
                transform.yaw_mdeg = calibration.optimized_yaw_mdeg;
            }
        }
        Some(transform)
    }

    fn apply_yaw_calibration(&self, packet: &mut RadarPacket) {
        let Some(calibration) = self.yaw_calibration.as_ref() else {
            return;
        };
        if calibration.base_transform != packet.coordinate_frame {
            return;
        }
        let yaw = (f64::from(calibration.optimized_yaw_mdeg) / 1_000.0).to_radians();
        for target in &mut packet.targets {
            let right = f64::from(target.x_mm)
                * if packet.coordinate_frame.raw_x_inverted { -1.0 } else { 1.0 };
            let forward = f64::from(target.y_mm);
            target.room_x_mm = packet.coordinate_frame.origin_x_mm
                + (forward * yaw.cos() - right * yaw.sin()).round() as i32;
            target.room_z_mm = packet.coordinate_frame.origin_z_mm
                + (forward * yaw.sin() + right * yaw.cos()).round() as i32;
        }
    }

    pub(crate) fn persist_yaw_calibration(&mut self, data_dir: &Path) -> Result<(), String> {
        let calibration = self
            .yaw_calibration
            .as_mut()
            .ok_or_else(|| "no completed yaw calibration is available".to_string())?;
        if calibration.persisted {
            return Ok(());
        }
        let directory = data_dir.join("mmwave").join("yaw-calibrations");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
        let path = directory.join(format!(
            "yaw-{}-{}.json",
            calibration.created_at_unix_ns,
            calibration.source_sha256.chars().take(16).collect::<String>()
        ));
        let mut artifact = calibration.clone();
        artifact.persisted = true;
        write_pretty_json_no_clobber(&path, &artifact).map_err(|error| error.to_string())?;
        *calibration = artifact.clone();
        if let Some(fixed) = self.fixed_point_calibration.as_mut() {
            fixed.yaw_calibration = Some(artifact);
        }
        Ok(())
    }

    pub(crate) fn restore_yaw_calibration(&mut self, data_dir: &Path) -> Result<(), String> {
        let Some((source_kind, source_id, source_sha256)) = self.calibration_source() else {
            return Ok(());
        };
        let directory = data_dir.join("mmwave").join("yaw-calibrations");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!("could not read {}: {error}", directory.display()));
            }
        };
        let mut latest = None;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(entry.path()).map_err(|error| error.to_string())?;
            let calibration: YawCalibrationStatus = match serde_json::from_slice(&bytes) {
                Ok(calibration) => calibration,
                Err(_) => continue,
            };
            if calibration.schema_version != YAW_CALIBRATION_SCHEMA_VERSION
                || calibration.source_kind != source_kind
                || calibration.source_id != source_id
                || calibration.source_sha256 != source_sha256
                || self
                    .expected_node
                    .as_ref()
                    .is_some_and(|node| node.transform != calibration.base_transform)
            {
                continue;
            }
            if latest.as_ref().is_none_or(|current: &YawCalibrationStatus| {
                calibration.created_at_unix_ns > current.created_at_unix_ns
            }) {
                latest = Some(calibration);
            }
        }
        self.yaw_calibration = latest;
        Ok(())
    }

    pub(crate) fn cancel_fixed_point_calibration(&mut self) {
        self.fixed_point_calibration = None;
        if self.session.is_none() {
            self.expected_mode = None;
        }
    }

    pub(crate) fn validate_calibration_phase_two_start(&self) -> Result<(), String> {
        let experiment = self
            .experiment
            .as_ref()
            .ok_or_else(|| "a sealed schema-v2 setup is required".to_string())?;
        let calibration = self.fixed_point_calibration.as_ref().ok_or_else(|| {
            "phase 1 must calibrate RX1 through RX4 and TX before CSI calibration"
                .to_string()
        })?;
        if calibration.current_index < calibration.anchors.len() {
            return Err("phase 1 must be completed before CSI calibration".to_string());
        }
        let mut expected_positions = experiment
            .geometry
            .rx_positions_m
            .iter()
            .map(|position| [
                (position[0] * 1_000.0).round() as i32,
                (position[2] * 1_000.0).round() as i32,
            ])
            .collect::<Vec<_>>();
        let tx = experiment.geometry.tx_position_m;
        expected_positions.push([
            (tx[0] * 1_000.0).round() as i32,
            (tx[2] * 1_000.0).round() as i32,
        ]);
        if calibration
            .anchors
            .iter()
            .map(|anchor| anchor.expected_position_mm)
            .ne(expected_positions)
        {
            return Err("phase 1 positions do not match the active sealed setup".to_string());
        }
        Ok(())
    }

    pub(crate) fn set_node_control_configuration(
        &mut self,
        url_configured: bool,
        token_configured: bool,
    ) {
        self.node_url_configured = url_configured;
        self.node_token_configured = token_configured;
    }

    pub(crate) fn restore_session_manifests(&mut self, data_dir: &Path) -> Result<(), String> {
        let directory = data_dir.join("mmwave");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "could not inspect session manifests in {}: {error}",
                    directory.display()
                ));
            }
        };
        let mut manifests = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".manifest.json"))
            {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let Ok(mut manifest) = serde_json::from_slice::<SessionManifest>(&bytes) else {
                continue;
            };
            if manifest.schema_version != SESSION_MANIFEST_SCHEMA_VERSION {
                continue;
            }
            if manifest.lifecycle == SessionLifecycle::Active {
                manifest.lifecycle = SessionLifecycle::Interrupted;
                manifest.updated_at_unix_ns = now_unix_ns();
                write_session_manifest(&path, &manifest)?;
            }
            manifests.push(manifest);
        }
        manifests.sort_by_key(|manifest| manifest.updated_at_unix_ns);
        self.restored_session = manifests
            .into_iter()
            .rev()
            .find(|manifest| {
                matches!(
                    manifest.lifecycle,
                    SessionLifecycle::Interrupted | SessionLifecycle::Error
                )
            })
            .map(|manifest| session_status_from_manifest(&manifest));
        Ok(())
    }

    pub(crate) fn prepare_session_start(&mut self, kind: SessionKind) -> Result<(), String> {
        self.validate_session_start(kind)?;
        self.expected_mode = Some(match kind {
            SessionKind::Calibration => MeasurementMode::Calibration,
            SessionKind::Blind => MeasurementMode::Reference,
            SessionKind::MmwaveOnly => MeasurementMode::Calibration,
        });
        Ok(())
    }

    pub(crate) fn cancel_prepared_session_start(&mut self) {
        if self.session.is_none() {
            self.expected_mode = None;
        }
    }

    fn persist_current_session_manifest(
        &self,
        lifecycle: SessionLifecycle,
        error: Option<String>,
        updated_at_unix_ns: u64,
    ) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| "no mmWave session is active".to_string())?;
        let manifest = session_manifest_from_session(
            session,
            lifecycle,
            updated_at_unix_ns,
            error,
        );
        write_session_manifest(&session.manifest_path, &manifest)
    }

    pub(crate) fn transform_reconfiguration_allowed(&self) -> bool {
        self.expected_node.is_none() && self.session.is_none()
    }

    pub(crate) fn clear_observed_transform(&mut self) {
        self.last_transform = None;
        self.state = LinkState::Disconnected;
        self.reason = "Waiting for a packet with the new transform.".to_string();
    }

    pub(crate) fn observe_csi(&mut self, frame: &RawCsiFrame) {
        let rx_id = frame.rx_id;
        let (Some(host_monotonic_ns), Some(clock_epoch_id)) =
            (frame.host_monotonic_ns, frame.clock_epoch_id.as_deref())
        else {
            self.csi_clock_rejections += 1;
            if let Some(session) = &mut self.session {
                session.rejected_samples += 1;
            }
            return;
        };
        if let Some(slot) = self
            .last_csi_ns
            .get_mut(usize::from(rx_id.saturating_sub(1)))
        {
            *slot = Some(host_monotonic_ns);
        }
        if let Some(observations) = self
            .rx_preflight
            .get_mut(usize::from(rx_id.saturating_sub(1)))
        {
            observations.push_back(CsiPreflightObservation {
                monotonic_ns: host_monotonic_ns,
                grid: PositionGridIdentity::from_frame(frame),
                source_bound: frame
                    .source_binding
                    .as_ref()
                    .is_some_and(|binding| binding.has_required_flags()),
                clock_epoch_id: clock_epoch_id.to_string(),
            });
            while observations.front().is_some_and(|observation| {
                host_monotonic_ns.saturating_sub(observation.monotonic_ns) > PREFLIGHT_WINDOW_NS
            }) {
                observations.pop_front();
            }
        }
        if let Some(session) = &mut self.session {
            if session.clock_epoch_id != clock_epoch_id {
                session.io_error = Some("CSI clock epoch differs from the active calibration session".to_string());
                return;
            }
            if session.io_error.is_none() {
                let write_result = serde_json::to_writer(&mut session.csi_writer, frame)
                    .and_then(|()| session.csi_writer.write_all(b"\n").map_err(serde_json::Error::io));
                if let Err(error) = write_result {
                    session.io_error = Some(format!("could not persist CSI packet: {error}"));
                }
            }
            match session.phase {
                SessionPhase::EmptyCalibration if session.empty_started_at_ns.is_some() => {
                    session.empty_frames.push(frame.clone());
                }
                SessionPhase::Training | SessionPhase::Blind
                    if session.stable_candidate.is_some() =>
                {
                    session.candidate_frames.push(frame.clone());
                }
                _ => {}
            }
        }
    }

    pub(crate) fn take_pending_index(&mut self) -> Option<(PathBuf, String)> {
        self.pending_index.take()
    }

    /// A generated candidate remains private to the blind evaluator until all
    /// predeclared position gates pass. The WiFi tracker may still evaluate it
    /// internally so predictions can be frozen before radar truth is attached.
    pub(crate) fn position_publication_allowed(&self) -> bool {
        !self.candidate_requires_validation
    }

    /// Freeze the latest WiFi-only decision before the next radar packet is
    /// allowed to contribute blind truth.
    pub(crate) fn observe_wifi_prediction(&mut self, state: &LivePositionState) {
        self.current_wifi_prediction = match state {
            LivePositionState::Position {
                point_id,
                coordinates_m,
            } => GuidedPredictionDecision::Position {
                point_id: point_id.clone(),
                coordinates_m: *coordinates_m,
            },
            LivePositionState::Unknown => GuidedPredictionDecision::Unknown,
            LivePositionState::Ambiguous => GuidedPredictionDecision::Ambiguous,
            LivePositionState::Insufficient => GuidedPredictionDecision::Insufficient,
            LivePositionState::Uncalibrated => GuidedPredictionDecision::Uncalibrated,
            LivePositionState::Stale => GuidedPredictionDecision::Stale,
        };
    }

    pub(crate) fn ingest_json(
        &mut self,
        bytes: &[u8],
        host_time: impl Into<HostTimestamp>,
    ) -> Result<(), String> {
        let host_time = host_time.into();
        let now_ns = host_time.host_monotonic_ns;
        let packet: RadarPacket = serde_json::from_slice(bytes)
            .map_err(|error| {
                self.reject_at(
                    PacketRejectCategory::InvalidJson,
                    format!("invalid JSON packet: {error}"),
                    now_ns,
                )
            })?;
        self.ingest(packet, bytes, host_time)
    }

    fn ingest(
        &mut self,
        mut packet: RadarPacket,
        original_bytes: &[u8],
        host_time: HostTimestamp,
    ) -> Result<(), String> {
        validate_packet_shape(&packet)
            .map_err(|error| {
                self.reject_at(
                    PacketRejectCategory::PacketShape,
                    error,
                    host_time.host_monotonic_ns,
                )
            })?;
        if let Some(cad) = &self.cad_profile {
            // Preview uses the saved mounting point and any explicit profile
            // orientation. Raw packet bytes are never rewritten.
            let origin_x = (cad.mounting_position_m[0] * 1000.0).round() as i32;
            let origin_z = (cad.mounting_position_m[2] * 1000.0).round() as i32;
            let yaw_mdeg = cad.yaw_mdeg.unwrap_or(packet.coordinate_frame.yaw_mdeg);
            let raw_x_inverted = cad
                .raw_x_inverted
                .unwrap_or(packet.coordinate_frame.raw_x_inverted);
            let yaw = (yaw_mdeg as f64 / 1000.0).to_radians();
            for target in &mut packet.targets {
                let right = f64::from(target.x_mm) * if raw_x_inverted { -1.0 } else { 1.0 };
                let forward = f64::from(target.y_mm);
                target.room_x_mm = origin_x + (forward * yaw.cos() - right * yaw.sin()).round() as i32;
                target.room_z_mm = origin_z + (forward * yaw.sin() + right * yaw.cos()).round() as i32;
            }
            packet.coordinate_frame.origin_x_mm = origin_x;
            packet.coordinate_frame.origin_z_mm = origin_z;
            packet.coordinate_frame.yaw_mdeg = yaw_mdeg;
            packet.coordinate_frame.raw_x_inverted = raw_x_inverted;
        }
        let sequence_disposition = self
            .validate_identity_and_sequence(&packet, host_time.host_monotonic_ns)
            .map_err(|error| {
                self.reject_at(error.category, error.reason, host_time.host_monotonic_ns)
            })?;
        if sequence_disposition == RadarSequenceDisposition::Duplicate {
            return Ok(());
        }
        // The firmware and sealed setup retain the immutable source transform.
        // A setup-bound calibration may refine only the derived room positions.
        self.apply_yaw_calibration(&mut packet);
        // Advance the transport high-water mark before validating content such
        // as room bounds. A well-formed packet that is rejected for content was
        // still received; otherwise every such reject is miscounted as a gap
        // from the last accepted packet.
        self.boot_id = Some(packet.boot_id);
        self.sequence = Some(packet.sequence);
        self.remember_sequence(packet.sequence);
        if let Some(expected) = self.expected_mode {
            if packet.mode != expected {
                return Err(self.reject_at(
                    PacketRejectCategory::ModeMismatch,
                    format!(
                        "expected {} mode, received {}",
                        expected.as_str(),
                        packet.mode.as_str()
                    ),
                    host_time.host_monotonic_ns,
                ));
            }
        }
        if let Some(transform) = &self.last_transform {
            if transform != &packet.coordinate_frame {
                return Err(self.reject_at(
                    PacketRejectCategory::TransformMismatch,
                    "coordinate transform changed inside the active setup".to_string(),
                    host_time.host_monotonic_ns,
                ));
            }
        }

        // A packet with a valid wire identity and expected mode proves that
        // the transport is alive even when its target payload is rejected
        // below. Target validity remains a separate content/label gate.
        self.last_transport_packet_ns = Some(host_time.host_monotonic_ns);

        let present: Vec<&RadarTarget> = packet
            .targets
            .iter()
            .filter(|target| target.present)
            .collect();
        self.node_id = Some(packet.node_id.clone());
        self.mode = Some(packet.mode);
        self.last_packet_ns = Some(host_time.host_monotonic_ns);
        self.clock_epoch_id = Some(host_time.clock_epoch_id.clone());
        self.last_transform = Some(packet.coordinate_frame.clone());
        self.target_count = present.len();
        self.targets = present
            .iter()
            .map(|target| RadarTargetStatus {
                slot: target.slot,
                raw_position_mm: [target.x_mm, target.y_mm],
                position_mm: [target.room_x_mm, target.room_z_mm],
                inside_room: self.validate_room_bounds(target).is_ok(),
            })
            .collect();
        if let [target] = present.as_slice() {
            self.remember_recent_observation(target, &host_time);
        }
        let mut outside_room_reason = None;
        if let Some((target, error)) = present.iter().find_map(|target| {
            self.validate_room_bounds(target)
                .err()
                .map(|error| (*target, error))
        }) {
                let reason = self.reject_at_with_position(
                    PacketRejectCategory::RoomBounds,
                    error,
                    host_time.host_monotonic_ns,
                    Some([target.x_mm, target.y_mm]),
                    Some([target.room_x_mm, target.room_z_mm]),
                );
                if self.session.as_ref().is_some_and(|session| {
                    matches!(session.phase, SessionPhase::EmptyCalibration | SessionPhase::MmwaveOnly)
                })
                {
                    outside_room_reason = Some(reason);
                } else {
                    return Err(reason);
                }
        }

        self.packets_received += 1;
        if let Some(session) = &mut self.session {
            if matches!(session.phase, SessionPhase::EmptyCalibration | SessionPhase::MmwaveOnly) {
                if let Some(previous) = session.empty_last_radar_packet_ns {
                    session.empty_max_radar_gap_ns = session
                        .empty_max_radar_gap_ns
                        .max(host_time.host_monotonic_ns.saturating_sub(previous));
                }
                session.empty_last_radar_packet_ns = Some(host_time.host_monotonic_ns);
                session.empty_radar_packets = session.empty_radar_packets.saturating_add(1);
                if session.phase == SessionPhase::MmwaveOnly {
                    if let Some(previous) = session.mmwave_only_last_radar_packet_ns {
                        session.mmwave_only_max_radar_gap_ns = session
                            .mmwave_only_max_radar_gap_ns
                            .max(host_time.host_monotonic_ns.saturating_sub(previous));
                    }
                    session.mmwave_only_last_radar_packet_ns =
                        Some(host_time.host_monotonic_ns);
                    session.mmwave_only_radar_packets =
                        session.mmwave_only_radar_packets.saturating_add(1);
                }
            }
        }

        match (outside_room_reason, present.as_slice()) {
            (Some(reason), _) => {
                self.target_raw_position_mm = None;
                self.target_position_mm = None;
                self.state = LinkState::Invalid;
                self.reason = if self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.phase == SessionPhase::MmwaveOnly)
                {
                    format!("Outside-room radar target ignored; radar-only check continues. {reason}")
                } else {
                    format!("Outside-room radar target ignored; empty-room calibration continues. {reason}")
                };
                self.advance_empty_calibration(&host_time)?;
                self.advance_mmwave_only_calibration(&host_time)?;
            }
            (None, []) => {
                self.target_raw_position_mm = None;
                self.target_position_mm = None;
                self.state = LinkState::NoTarget;
                self.reason = "Radar is connected but currently sees no target.".to_string();
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.phase == SessionPhase::MmwaveOnly)
                {
                    if let Some(session) = &mut self.session {
                        session.mmwave_only_no_target_packets = session
                            .mmwave_only_no_target_packets
                            .saturating_add(1);
                    }
                }
                self.reset_stability();
                self.advance_empty_calibration(&host_time)?;
                self.advance_mmwave_only_calibration(&host_time)?;
            }
            (None, [target]) => {
                self.target_raw_position_mm = Some([target.x_mm, target.y_mm]);
                self.target_position_mm = Some([target.room_x_mm, target.room_z_mm]);
                self.state = LinkState::Valid;
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| matches!(session.phase, SessionPhase::EmptyCalibration | SessionPhase::MmwaveOnly))
                {
                    if let Some(session) = &mut self.session {
                        if session.phase == SessionPhase::MmwaveOnly {
                            session.mmwave_only_valid_target_packets = session
                                .mmwave_only_valid_target_packets
                                .saturating_add(1);
                        } else {
                            session.empty_in_room_targets = session.empty_in_room_targets.saturating_add(1);
                        }
                    }
                    self.reason = "In-room radar target recorded; empty-room calibration continues and will be evaluated at the end.".to_string();
                    self.advance_empty_calibration(&host_time)?;
                    self.advance_mmwave_only_calibration(&host_time)?;
                } else {
                    self.reason = "One valid radar target is available.".to_string();
                    self.observe_target(target, &host_time)?;
                }
            }
            (None, _) => {
                self.target_raw_position_mm = None;
                self.target_position_mm = None;
                self.state = LinkState::MultiTarget;
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| matches!(session.phase, SessionPhase::EmptyCalibration | SessionPhase::MmwaveOnly))
                {
                    if let Some(session) = &mut self.session {
                        if session.phase == SessionPhase::MmwaveOnly {
                            session.mmwave_only_multi_target_packets = session
                                .mmwave_only_multi_target_packets
                                .saturating_add(1);
                        } else {
                            session.empty_multi_target_packets = session
                                .empty_multi_target_packets
                                .saturating_add(1);
                        }
                    }
                    self.reason = "Multiple radar targets recorded; empty-room calibration continues and will be evaluated at the end.".to_string();
                    self.advance_empty_calibration(&host_time)?;
                    self.advance_mmwave_only_calibration(&host_time)?;
                } else {
                    self.reason =
                        "Multiple radar targets are visible; no label is emitted.".to_string();
                    self.reset_stability();
                }
            }
        }
        self.write_session_packet(original_bytes, &host_time)?;
        let completed_kind = self
            .session
            .as_ref()
            .and_then(|session| (session.phase == SessionPhase::Complete).then_some(session.kind));
        match completed_kind {
            Some(SessionKind::Calibration)
                if self.position_index_sha256.is_none() && self.empty_calibration_is_valid() =>
            {
                self.finalize_training_index()?;
            }
            Some(SessionKind::Blind) if self.blind_report_sha256.is_none() => {
                self.finalize_blind_evaluation()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn empty_calibration_is_valid(&self) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session
                .empty_validity
                .as_ref()
                .is_some_and(|validity| validity.verdict == "valid")
        })
    }

    fn validate_identity_and_sequence(
        &mut self,
        packet: &RadarPacket,
        host_monotonic_ns: u64,
    ) -> Result<RadarSequenceDisposition, PacketValidationError> {
        if let Some(expected) = &self.expected_node {
            if packet.node_id != expected.node_id {
                return Err(PacketValidationError::new(
                    PacketRejectCategory::UnexpectedNode,
                    format!(
                        "sealed setup requires mmWave node {:?}, received {:?}",
                        expected.node_id, packet.node_id
                    ),
                ));
            }
            if packet.coordinate_frame != expected.transform {
                return Err(PacketValidationError::new(
                    PacketRejectCategory::TransformMismatch,
                    "packet transform does not match the sealed setup",
                ));
            }
        }
        if let Some(node_id) = &self.node_id {
            if node_id != &packet.node_id {
                return Err(PacketValidationError::new(
                    PacketRejectCategory::NodeChanged,
                    format!("node changed from {node_id:?} to {:?}", packet.node_id),
                ));
            }
        }
        match (self.boot_id, self.sequence) {
            (Some(boot_id), Some(previous)) if boot_id == packet.boot_id => {
                if self.recent_sequences.contains(&packet.sequence) {
                    return Ok(RadarSequenceDisposition::Duplicate);
                }
                let expected = previous.wrapping_add(1);
                if packet.sequence != expected {
                    let gap = packet.sequence.wrapping_sub(expected);
                    if gap > u32::MAX / 2 {
                        self.last_radar_sequence_fault_ns = Some(host_monotonic_ns);
                    self.last_sequence_gap = Some(LastSequenceGap {
                        expected_sequence: expected,
                        received_sequence: packet.sequence,
                        missing_packets: 0,
                        at_monotonic_ns: host_monotonic_ns,
                    });
                    if let Some(session) = &mut self.session {
                        if session.phase == SessionPhase::EmptyCalibration {
                            session.empty_sequence_gaps = session.empty_sequence_gaps.saturating_add(1);
                        } else if session.phase == SessionPhase::MmwaveOnly {
                            session.mmwave_only_sequence_gaps =
                                session.mmwave_only_sequence_gaps.saturating_add(1);
                        }
                    }
                    return Err(PacketValidationError::new(
                            PacketRejectCategory::SequenceOutOfOrder,
                            "out-of-order packet sequence",
                        ));
                    }
                    self.last_radar_sequence_fault_ns = Some(host_monotonic_ns);
                    self.packets_lost += u64::from(gap);
                    self.last_sequence_gap = Some(LastSequenceGap {
                        expected_sequence: expected,
                        received_sequence: packet.sequence,
                        missing_packets: u64::from(gap),
                        at_monotonic_ns: host_monotonic_ns,
                    });
                    if let Some(session) = &mut self.session {
                        if session.phase == SessionPhase::EmptyCalibration {
                            session.empty_sequence_gaps = session.empty_sequence_gaps.saturating_add(1);
                        } else if session.phase == SessionPhase::MmwaveOnly {
                            session.mmwave_only_sequence_gaps =
                                session.mmwave_only_sequence_gaps.saturating_add(1);
                        }
                    }
                }
            }
            (Some(_), _) => {
                self.recent_sequences.clear();
                self.last_radar_sequence_fault_ns = Some(host_monotonic_ns);
                self.last_radar_reboot_ns = Some(host_monotonic_ns);
                self.reboot_count += 1;
                if let Some(session) = &mut self.session {
                    if session.phase == SessionPhase::EmptyCalibration {
                        session.empty_reboots = session.empty_reboots.saturating_add(1);
                    } else if session.phase == SessionPhase::MmwaveOnly {
                        session.mmwave_only_reboots = session.mmwave_only_reboots.saturating_add(1);
                    }
                }
                self.reset_stability();
            }
            _ => {}
        }
        Ok(RadarSequenceDisposition::Fresh)
    }

    fn remember_sequence(&mut self, sequence: u32) {
        self.recent_sequences.push_back(sequence);
        while self.recent_sequences.len() > RECENT_RADAR_SEQUENCE_WINDOW {
            self.recent_sequences.pop_front();
        }
    }

    fn validate_room_bounds(&self, target: &RadarTarget) -> Result<(), String> {
        let dimensions = self
            .room_dimensions_mm
            .ok_or_else(|| "room dimensions are not configured".to_string())?;
        if target.room_x_mm < 0
            || target.room_z_mm < 0
            || target.room_x_mm > dimensions[0]
            || target.room_z_mm > dimensions[1]
        {
            return Err(format!(
                "target [{}, {}] mm is outside room [0..{}, 0..{}] mm",
                target.room_x_mm, target.room_z_mm, dimensions[0], dimensions[1]
            ));
        }
        Ok(())
    }

    fn reject_at(
        &mut self,
        category: PacketRejectCategory,
        reason: String,
        now_ns: u64,
    ) -> String {
        self.reject_at_with_position(category, reason, now_ns, None, None)
    }

    fn reject_at_with_position(
        &mut self,
        category: PacketRejectCategory,
        reason: String,
        now_ns: u64,
        raw_position_mm: Option<[i16; 2]>,
        position_mm: Option<[i32; 2]>,
    ) -> String {
        self.packets_rejected += 1;
        *self
            .reject_reasons
            .entry(category.as_str().to_string())
            .or_default() += 1;
        if let Some(session) = &mut self.session {
            if session.phase == SessionPhase::EmptyCalibration {
                if category == PacketRejectCategory::RoomBounds {
                    session.empty_outside_room_targets = session
                        .empty_outside_room_targets
                        .saturating_add(1);
                } else {
                    session.empty_invalid_packets = session.empty_invalid_packets.saturating_add(1);
                }
            } else if session.phase == SessionPhase::MmwaveOnly {
                if category == PacketRejectCategory::RoomBounds {
                    session.mmwave_only_outside_room_targets = session
                        .mmwave_only_outside_room_targets
                        .saturating_add(1);
                } else {
                    session.mmwave_only_invalid_packets =
                        session.mmwave_only_invalid_packets.saturating_add(1);
                }
            }
        }
        self.recent_rejections
            .push_back((now_ns, category.as_str().to_string()));
        while self
            .recent_rejections
            .front()
            .is_some_and(|(at_ns, _)| now_ns.saturating_sub(*at_ns) > REJECTION_WINDOW_NS)
        {
            self.recent_rejections.pop_front();
        }
        self.last_rejection = Some(LastPacketRejection {
            category: category.as_str().to_string(),
            reason: reason.clone(),
            raw_position_mm,
            position_mm,
            at_monotonic_ns: now_ns,
        });
        self.reset_stability();
        self.state = LinkState::Invalid;
        self.reason = reason.clone();
        reason
    }

    fn observe_target(&mut self, target: &RadarTarget, host_time: &HostTimestamp) -> Result<(), String> {
        let position = [target.room_x_mm, target.room_z_mm];

        let aligned = self.csi_is_aligned(host_time.host_monotonic_ns);
        if let Some(session) = &mut self.session {
            if aligned {
                session.aligned_samples += 1;
            } else {
                session.rejected_samples += 1;
            }
        }

        let phase = self.session.as_ref().map(|session| session.phase);
        if phase == Some(SessionPhase::Blind) {
            if let Some(session) = &mut self.session {
                session.trajectory.push(position);
            }
        }
        match phase {
            Some(SessionPhase::Coverage) => {
                if aligned {
                    let cell = (
                        target.room_x_mm.div_euclid(COVERAGE_CELL_MM),
                        target.room_z_mm.div_euclid(COVERAGE_CELL_MM),
                    );
                    *self.coverage.entry(cell).or_default() += 1;
                    if let Ok(zones) = select_zones(&self.coverage, self.zone_count) {
                        self.zones = zones;
                        if let Some(session) = &mut self.session {
                            session.phase = SessionPhase::Training;
                        }
                    }
                }
            }
            Some(SessionPhase::Training) => {
                self.observe_stable_zone(position, target.speed_cm_s, host_time, aligned, false)?;
            }
            Some(SessionPhase::Blind) => {
                self.observe_stable_zone(position, target.speed_cm_s, host_time, aligned, true)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn remember_recent_observation(&mut self, target: &RadarTarget, host_time: &HostTimestamp) {
        self.recent_observations.push_back(RecentRadarObservation {
            observed_at_ns: host_time.host_monotonic_ns,
            raw_position_mm: [i32::from(target.x_mm), i32::from(target.y_mm)],
            room_position_mm: [target.room_x_mm, target.room_z_mm],
        });
        while self.recent_observations.len() > MAX_RECENT_OBSERVATIONS {
            self.recent_observations.pop_front();
        }
    }

    pub(crate) fn tick(&mut self, now: impl Into<HostTimestamp>) -> Result<(), String> {
        let now = now.into();
        self.advance_empty_calibration(&now)?;
        self.advance_mmwave_only_calibration(&now)
    }

    fn advance_empty_calibration(&mut self, host_time: &HostTimestamp) -> Result<(), String> {
        let should_finish = if let Some(session) = &mut self.session {
            if session.phase != SessionPhase::EmptyCalibration {
                return Ok(());
            }
            let Some(started) = session.empty_started_at_ns else {
                return Ok(());
            };
            host_time.host_monotonic_ns.saturating_sub(started) >= session.empty_duration_ns
        } else {
            false
        };
        if should_finish {
            self.finish_empty_calibration(
                host_time.host_unix_ns,
                host_time.host_monotonic_ns,
            )?;
        }
        Ok(())
    }

    fn advance_mmwave_only_calibration(
        &mut self,
        host_time: &HostTimestamp,
    ) -> Result<(), String> {
        let should_finish = if let Some(session) = &mut self.session {
            if session.phase != SessionPhase::MmwaveOnly {
                return Ok(());
            }
            let Some(started) = session.empty_started_at_ns else {
                return Ok(());
            };
            host_time.host_monotonic_ns.saturating_sub(started) >= session.empty_duration_ns
        } else {
            false
        };
        if should_finish {
            self.finish_mmwave_only_calibration(
                host_time.host_unix_ns,
                host_time.host_monotonic_ns,
            )?;
        }
        Ok(())
    }

    fn finish_mmwave_only_calibration(
        &mut self,
        ended_at_ns: u64,
        ended_at_monotonic_ns: u64,
    ) -> Result<(), String> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "missing mmWave-only calibration session".to_string())?;
        if let Err(error) = session.writer.flush() {
            let message = format!("could not flush mmWave-only radar recording: {error}");
            session.io_error = Some(message.clone());
            return Err(message);
        }
        let duration_ns = session.empty_duration_ns;
        let max_gap_ns = session.mmwave_only_max_radar_gap_ns.max(
            ended_at_monotonic_ns.saturating_sub(
                session
                    .mmwave_only_last_radar_packet_ns
                    .unwrap_or(session.empty_started_at_ns.unwrap_or(ended_at_monotonic_ns)),
            ),
        );
        let mut reasons = Vec::new();
        if session.mmwave_only_radar_packets == 0 {
            reasons.push("no valid radar transport packet was observed".to_string());
        }
        if session.mmwave_only_valid_target_packets > 0 {
            reasons.push(format!(
                "{} in-room radar target packet(s) were observed",
                session.mmwave_only_valid_target_packets
            ));
        }
        if session.mmwave_only_outside_room_targets > 0 {
            reasons.push(format!(
                "{} outside-room radar target packet(s) were observed",
                session.mmwave_only_outside_room_targets
            ));
        }
        if session.mmwave_only_multi_target_packets > 0 {
            reasons.push(format!(
                "{} multi-target radar packet(s) were observed",
                session.mmwave_only_multi_target_packets
            ));
        }
        if session.mmwave_only_invalid_packets > 0 {
            reasons.push(format!(
                "{} invalid radar packet(s) were observed",
                session.mmwave_only_invalid_packets
            ));
        }
        if session.mmwave_only_sequence_gaps > 0 {
            reasons.push(format!(
                "{} radar sequence gap/out-of-order event(s) were observed",
                session.mmwave_only_sequence_gaps
            ));
        }
        if session.mmwave_only_reboots > 0 {
            reasons.push(format!(
                "{} radar reboot event(s) were observed",
                session.mmwave_only_reboots
            ));
        }
        if max_gap_ns > STALE_AFTER_NS {
            reasons.push(format!(
                "radar transport gap reached {} ms",
                max_gap_ns / 1_000_000
            ));
        }
        let validity = MmwaveOnlyValidity {
            verdict: if reasons.is_empty() { "valid" } else { "invalid" }.to_string(),
            reasons,
            radar_packets: session.mmwave_only_radar_packets,
            valid_target_packets: session.mmwave_only_valid_target_packets,
            no_target_packets: session.mmwave_only_no_target_packets,
            outside_room_targets: session.mmwave_only_outside_room_targets,
            multi_target_packets: session.mmwave_only_multi_target_packets,
            invalid_packets: session.mmwave_only_invalid_packets,
            sequence_gaps: session.mmwave_only_sequence_gaps,
            reboots: session.mmwave_only_reboots,
            max_radar_gap_ms: max_gap_ns / 1_000_000,
            duration_seconds: duration_ns / 1_000_000_000,
        };
        session.mmwave_only_validity = Some(validity);
        session.phase = SessionPhase::Complete;
        session.empty_started_at_ns = None;
        let _ = session;
        self.persist_current_session_manifest(
            SessionLifecycle::Complete,
            None,
            ended_at_ns,
        )
    }

    fn finish_empty_calibration(
        &mut self,
        ended_at_ns: u64,
        ended_at_monotonic_ns: u64,
    ) -> Result<(), String> {
        let context = self
            .experiment
            .as_ref()
            .ok_or_else(|| "missing sealed experiment context".to_string())?
            .clone();
        let (
            session_id,
            duration_ns,
            _started_at_monotonic_ns,
            frames,
            outside_room_targets,
            in_room_targets,
            multi_target_packets,
            invalid_packets,
            sequence_gaps,
            reboots,
            radar_packets,
            max_radar_gap_ns,
        ) = {
            let session = self
                .session
                .as_mut()
                .ok_or_else(|| "missing calibration session".to_string())?;
            let started_at_monotonic_ns = session
                .empty_started_at_ns
                .ok_or_else(|| "empty calibration has no start time".to_string())?;
            (
                session.id.clone(),
                session.empty_duration_ns,
                started_at_monotonic_ns,
                std::mem::take(&mut session.empty_frames),
                session.empty_outside_room_targets,
                session.empty_in_room_targets,
                session.empty_multi_target_packets,
                session.empty_invalid_packets,
                session.empty_sequence_gaps,
                session.empty_reboots,
                session.empty_radar_packets,
                session.empty_max_radar_gap_ns.max(
                    ended_at_monotonic_ns.saturating_sub(
                        session
                            .empty_last_radar_packet_ns
                            .unwrap_or(started_at_monotonic_ns),
                    ),
                ),
            )
        };
        let capture = PositionCapture {
            recording_id: format!("{}-empty", session_id),
            setup_id: context.setup_id,
            setup_sha256: context.setup_sha256.clone(),
            server_version: context.server_version,
            geometry: context.geometry,
            started_at_unix_ns: ended_at_ns.saturating_sub(duration_ns),
            ended_at_unix_ns: ended_at_ns,
            frames,
        };
        let csi_frames = capture.frames.len() as u64;
        let mut reasons = Vec::new();
        if in_room_targets > 0 {
            reasons.push(format!(
                "{in_room_targets} in-room radar target packet(s) were observed"
            ));
        }
        if multi_target_packets > 0 {
            reasons.push(format!(
                "{multi_target_packets} multi-target radar packet(s) were observed"
            ));
        }
        if invalid_packets > 0 {
            reasons.push(format!(
                "{invalid_packets} invalid radar packet(s) were observed"
            ));
        }
        if sequence_gaps > 0 {
            reasons.push(format!(
                "{sequence_gaps} radar sequence gap/out-of-order event(s) were observed"
            ));
        }
        if reboots > 0 {
            reasons.push(format!("{reboots} radar reboot event(s) were observed"));
        }
        if radar_packets == 0 {
            reasons.push("no valid radar transport packet was observed".to_string());
        }
        if max_radar_gap_ns > STALE_AFTER_NS {
            reasons.push(format!(
                "radar transport gap reached {} ms",
                max_radar_gap_ns / 1_000_000
            ));
        }

        let reference = match build_position_empty_reference(&capture, &context.setup_sha256) {
            Ok(reference) => Some(reference),
            Err(error) => {
                reasons.push(format!("empty-room CSI reference was rejected: {error}"));
                None
            }
        };
        if reference.is_none() && reasons.is_empty() {
            reasons.push("no usable empty-room CSI reference was produced".to_string());
        }
        let verdict = if reasons.is_empty() { "valid" } else { "invalid" };
        let validity = EmptyCalibrationValidity {
            verdict: verdict.to_string(),
            reasons,
            outside_room_targets,
            in_room_targets,
            multi_target_packets,
            invalid_packets,
            sequence_gaps,
            reboots,
            radar_packets,
            max_radar_gap_ms: max_radar_gap_ns / 1_000_000,
            csi_frames,
            duration_seconds: duration_ns / 1_000_000_000,
        };
        if let Some(session) = &mut self.session {
            session.empty_reference = reference;
            session.empty_validity = Some(validity);
            session.phase = SessionPhase::Coverage;
            session.empty_started_at_ns = None;
        }
        if let Err(error) = self.persist_current_session_manifest(
            SessionLifecycle::Active,
            None,
            ended_at_ns,
        ) {
            if let Some(session) = &mut self.session {
                session.io_error = Some(error.clone());
            }
            return Err(error);
        }
        Ok(())
    }

    fn csi_is_aligned(&self, radar_ns: u64) -> bool {
        self.last_csi_ns.iter().all(|timestamp| {
            timestamp.is_some_and(|timestamp| radar_ns.abs_diff(timestamp) <= ALIGNMENT_LIMIT_NS)
        })
    }

    fn observe_stable_zone(
        &mut self,
        position: [i32; 2],
        speed_cm_s: i16,
        host_time: &HostTimestamp,
        aligned: bool,
        blind: bool,
    ) -> Result<(), String> {
        let Some(zone_index) = nearest_zone(&self.zones, position) else {
            self.reset_stability();
            return Ok(());
        };
        if self
            .session
            .as_ref()
            .and_then(|session| session.must_exit_zone)
            .is_some_and(|blocked| blocked == zone_index)
        {
            return Ok(());
        }
        if let Some(session) = &mut self.session {
            if session.must_exit_zone.is_some() {
                session.must_exit_zone = None;
            }
        }
        if !aligned || speed_cm_s.abs() > MAX_STABLE_SPEED_CM_S {
            self.reset_stability();
            return Ok(());
        }

        let candidate = self
            .session
            .as_ref()
            .and_then(|session| session.stable_candidate.clone());
        let (candidate, candidate_changed) = match candidate {
            Some(candidate)
                if candidate.zone_index == zone_index
                    && squared_distance(candidate.anchor_mm, position)
                        <= i64::from(STABILITY_RADIUS_MM).pow(2) =>
            {
                (candidate, false)
            }
            _ => (
                StableCandidate {
                    zone_index,
                    started_at_unix_ns: host_time.host_unix_ns,
                    started_at_monotonic_ns: host_time.host_monotonic_ns,
                    anchor_mm: position,
                },
                true,
            ),
        };
        let complete = host_time
            .host_monotonic_ns
            .saturating_sub(candidate.started_at_monotonic_ns)
            >= STABLE_BLOCK_NS;
        let transform_sha256 = self
            .last_transform
            .as_ref()
            .map(deterministic_pretty_json)
            .transpose()
            .map_err(|error| error.to_string())?
            .map(|bytes| sha256_bytes(&bytes))
            .ok_or_else(|| "stable radar observation has no transform".to_string())?;
        let radar_observation = RadarObservation {
            host_unix_ns: host_time.host_unix_ns,
            host_monotonic_ns: host_time.host_monotonic_ns,
            clock_epoch_id: host_time.clock_epoch_id.clone(),
            boot_id: self.boot_id.ok_or_else(|| "radar boot identity is missing".to_string())?,
            sequence: self.sequence.ok_or_else(|| "radar sequence is missing".to_string())?,
            transform_sha256,
            position_mm: position,
        };
        if let Some(session) = &mut self.session {
            if candidate_changed {
                session.candidate_frames.clear();
                session.candidate_positions_mm.clear();
                session.candidate_radar.clear();
            }
            session.stable_candidate = Some(candidate);
            session.candidate_positions_mm.push(position);
            session.candidate_radar.push(radar_observation);
        }
        if !complete {
            return Ok(());
        }

        let recorded = if blind {
            let radar_position_mm = median_position_mm(
                self.session
                    .as_ref()
                    .map(|session| session.candidate_positions_mm.as_slice())
                    .unwrap_or_default(),
            )
            .ok_or_else(|| "stable blind visit has no radar positions".to_string())?;
            self.record_blind_visit(zone_index, host_time.host_unix_ns, radar_position_mm)?;
            true
        } else {
            self.record_training_block(zone_index, host_time)?
        };
        if !recorded {
            self.reset_stability();
            return Ok(());
        }
        let zone = &mut self.zones[zone_index];
        if blind {
            zone.blind_visits = zone
                .blind_visits
                .saturating_add(1)
                .min(BLIND_VISITS_PER_ZONE);
        } else {
            zone.training_blocks = zone.training_blocks.saturating_add(1).min(BLOCKS_PER_ZONE);
        }
        if let Some(session) = &mut self.session {
            session.stable_candidate = None;
            session.candidate_frames.clear();
            session.candidate_positions_mm.clear();
            session.candidate_radar.clear();
            session.must_exit_zone = Some(zone_index);
        }
        let done = if blind {
            self.zones
                .iter()
                .all(|zone| zone.blind_visits >= BLIND_VISITS_PER_ZONE)
        } else {
            self.zones
                .iter()
                .all(|zone| zone.training_blocks >= BLOCKS_PER_ZONE)
        };
        if done {
            if let Some(session) = &mut self.session {
                session.phase = SessionPhase::Complete;
            }
            if let Err(error) = self.persist_current_session_manifest(
                SessionLifecycle::Complete,
                None,
                host_time.host_unix_ns,
            ) {
                if let Some(session) = &mut self.session {
                    session.io_error = Some(error.clone());
                }
                return Err(error);
            }
        }
        Ok(())
    }

    fn reset_stability(&mut self) {
        if let Some(session) = &mut self.session {
            session.stable_candidate = None;
            session.candidate_frames.clear();
            session.candidate_positions_mm.clear();
            session.candidate_radar.clear();
        }
    }

    fn record_training_block(
        &mut self,
        zone_index: usize,
        ended_at: &HostTimestamp,
    ) -> Result<bool, String> {
        let context = self
            .experiment
            .as_ref()
            .ok_or_else(|| "missing sealed experiment context".to_string())?
            .clone();
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "missing calibration session".to_string())?;
        let candidate = session
            .stable_candidate
            .as_ref()
            .ok_or_else(|| "stable block has no radar candidate".to_string())?
            .clone();
        let empty_reference = session
            .empty_reference
            .as_ref()
            .ok_or_else(|| "stable block has no empty-room reference".to_string())?;
        let frames = std::mem::take(&mut session.candidate_frames);
        let csi_signal_sha256 = signal_sha256(&frames)
            .map_err(|error| format!("could not seal stable CSI block: {error}"))?;
        let capture = PositionCapture {
            recording_id: format!(
                "{}-{}-{:02}",
                session.id,
                self.zones[zone_index].id,
                session.training_blocks.len() + 1
            ),
            setup_id: context.setup_id.clone(),
            setup_sha256: context.setup_sha256.clone(),
            server_version: context.server_version.clone(),
            geometry: context.geometry.clone(),
            started_at_unix_ns: candidate.started_at_unix_ns,
            ended_at_unix_ns: ended_at.host_unix_ns,
            frames,
        };
        let window_start = candidate
            .started_at_unix_ns
            .checked_add(FEATURE_OFFSET_NS)
            .ok_or_else(|| "stable feature timestamp overflow".to_string())?;
        let block = extract_position_feature_window(&capture, empty_reference, window_start)
            .map_err(|error| format!("stable CSI block was rejected: {error}"))?;
        let midpoint_monotonic_ns = candidate
            .started_at_monotonic_ns
            .checked_add(FEATURE_OFFSET_NS + super::position_capture::WINDOW_NS / 2)
            .ok_or_else(|| "stable midpoint timestamp overflow".to_string())?;
        let receiver_midpoints: Vec<(u8, u64)> = (1..=4)
            .filter_map(|rx_id| {
                capture
                    .frames
                    .iter()
                    .filter(|frame| frame.rx_id == rx_id)
                    .filter_map(|frame| frame.host_monotonic_ns.map(|timestamp| (timestamp, frame)))
                    .min_by_key(|(timestamp, _)| midpoint_monotonic_ns.abs_diff(*timestamp))
                    .map(|(timestamp, _)| (rx_id, timestamp))
            })
            .collect();
        let zone_id = self.zones[zone_index].id.clone();
        let dataset_record = calibration_dataset::align_record(
            calibration_dataset::deterministic_sample_id(
                &context.setup_sha256,
                &session.id,
                &zone_id,
                midpoint_monotonic_ns,
            ),
            zone_id.clone(),
            &block,
            midpoint_monotonic_ns,
            &receiver_midpoints,
            &session.candidate_radar,
        );
        if matches!(dataset_record, CalibrationDatasetRecord::Rejected { .. }) {
            session.dataset_records.push(dataset_record);
            session.rejected_samples += 1;
            return Ok(false);
        }
        let grids: Vec<PositionGridIdentity> = block
            .receivers
            .iter()
            .map(|receiver| receiver.grid)
            .collect();
        match &session.receiver_grids {
            Some(expected) if expected != &grids => {
                return Err("stable CSI block grid differs from earlier blocks".to_string());
            }
            None => session.receiver_grids = Some(grids),
            _ => {}
        }
        let zone = &self.zones[zone_index];
        session.training_blocks.push(TrainingBlockProvenance {
            zone_id: zone.id.clone(),
            started_at_unix_ns: candidate.started_at_unix_ns,
            ended_at_unix_ns: ended_at.host_unix_ns,
            csi_signal_sha256,
        });
        session.dataset_records.push(dataset_record);
        let _ = session;
        if let Err(error) = self.persist_current_session_manifest(
            SessionLifecycle::Active,
            None,
            ended_at.host_unix_ns,
        ) {
            if let Some(session) = &mut self.session {
                session.io_error = Some(error.clone());
            }
            return Err(error);
        }
        Ok(true)
    }

    fn finalize_training_index(&mut self) -> Result<(), String> {
        let context = self
            .experiment
            .as_ref()
            .ok_or_else(|| "missing sealed experiment context".to_string())?
            .clone();
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "missing calibration session".to_string())?;
        session
            .writer
            .flush()
            .map_err(|error| format!("could not flush radar training record: {error}"))?;
        session
            .csi_writer
            .flush()
            .map_err(|error| format!("could not flush CSI training record: {error}"))?;
        if let Some(error) = session.io_error.clone() {
            return Err(error);
        }
        let radar_recording_sha256 =
            sha256_file(&session.recording_path).map_err(|error| error.to_string())?;
        let raw_csi_sha256 =
            sha256_file(&session.csi_recording_path).map_err(|error| error.to_string())?;
        let dataset = calibration_dataset::write_dataset(
            session
                .recording_path
                .parent()
                .ok_or_else(|| "calibration recording has no parent directory".to_string())?,
            &session.id,
            &session.id,
            &context.setup_id,
            &context.setup_sha256,
            &session.clock_epoch_id,
            raw_csi_sha256,
            radar_recording_sha256.clone(),
            self.zone_count,
            &session.dataset_records,
        )?;
        let training_samples: Vec<PositionFingerprintSample> = session
            .dataset_records
            .iter()
            .filter_map(|record| match record {
                CalibrationDatasetRecord::Accepted {
                    zone_id,
                    receivers,
                    ..
                } => {
                    let zone = self.zones.iter().find(|zone| &zone.id == zone_id)?;
                    Some(PositionFingerprintSample {
                        position: FingerprintPosition {
                            id: zone.id.clone(),
                            coordinates_m: [
                                f64::from(zone.center_mm[0]) / 1000.0,
                                0.0,
                                f64::from(zone.center_mm[1]) / 1000.0,
                            ],
                        },
                        rx_features: receivers
                            .iter()
                            .map(|receiver| receiver.features.clone())
                            .collect(),
                    })
                }
                CalibrationDatasetRecord::Rejected { .. } => None,
            })
            .collect();
        let model = PositionFingerprintModel::train(
            &training_samples,
            PositionFingerprintConfig {
                minimum_samples_per_position: usize::from(BLOCKS_PER_ZONE),
            },
        )
        .map_err(|error| format!("could not train mmWave-gated position model: {error}"))?;
        let points: Vec<FingerprintPosition> = self
            .zones
            .iter()
            .map(|zone| FingerprintPosition {
                id: zone.id.clone(),
                coordinates_m: [
                    f64::from(zone.center_mm[0]) / 1000.0,
                    0.0,
                    f64::from(zone.center_mm[1]) / 1000.0,
                ],
            })
            .collect();
        let artifact = MmwavePositionIndexArtifact::new(
            context.setup_id,
            context.setup_sha256,
            context.server_version,
            context.geometry,
            radar_recording_sha256,
            dataset.manifest_sha256,
            self.zones.len(),
            calibration_dataset::ALIGNMENT_LIMIT_MS,
            session
                .receiver_grids
                .clone()
                .ok_or_else(|| "training produced no CSI grid contract".to_string())?,
            points,
            session.training_blocks.clone(),
            session
                .empty_reference
                .clone()
                .ok_or_else(|| "training produced no empty reference".to_string())?,
            model,
        )?;
        let index_path = session
            .recording_path
            .with_file_name(format!("{}.position-index.json", session.id));
        let index_sha256 = artifact.write(&index_path)?;
        self.blind_index = Some(artifact);
        self.position_index_sha256 = Some(index_sha256.clone());
        self.pending_index = Some((index_path, index_sha256));
        self.candidate_requires_validation = true;
        Ok(())
    }

    fn record_blind_visit(
        &mut self,
        zone_index: usize,
        observed_at_ns: u64,
        radar_position_mm: [i32; 2],
    ) -> Result<(), String> {
        let context = self
            .experiment
            .as_ref()
            .ok_or_else(|| "missing sealed experiment context".to_string())?
            .clone();
        let index = self
            .blind_index
            .as_ref()
            .ok_or_else(|| "blind evaluation has no frozen WiFi index".to_string())?;
        let zone = self.zones[zone_index].clone();
        let visit_id = format!("{}-visit-{}", zone.id, zone.blind_visits + 1);
        let (candidate, candidate_frames) = {
            let session = self
                .session
                .as_ref()
                .ok_or_else(|| "missing blind session".to_string())?;
            (
                session
                    .stable_candidate
                    .as_ref()
                    .ok_or_else(|| "blind visit has no stable radar candidate".to_string())?
                    .clone(),
                session.candidate_frames.clone(),
            )
        };
        let capture = PositionCapture {
            recording_id: format!("{}-{}", visit_id, observed_at_ns),
            setup_id: context.setup_id,
            setup_sha256: context.setup_sha256,
            server_version: context.server_version,
            geometry: context.geometry,
            started_at_unix_ns: candidate.started_at_unix_ns,
            ended_at_unix_ns: observed_at_ns,
            frames: candidate_frames,
        };
        let window_start = candidate
            .started_at_unix_ns
            .checked_add(FEATURE_OFFSET_NS)
            .ok_or_else(|| "blind feature timestamp overflow".to_string())?;
        let block =
            extract_position_feature_window(&capture, index.empty_reference(), window_start)
                .map_err(|error| format!("blind CSI block was rejected: {error}"))?;
        let receiver_ablation_predictions = (1..=4)
            .map(|rx_id| {
                index
                    .predict_receiver_feature_block(&block, rx_id)
                    .map(|position| GuidedReceiverPrediction {
                        rx_id,
                        point_id: position.id,
                        coordinates_m: position.coordinates_m,
                    })
                    .map_err(|error| format!("RX{rx_id} blind ablation failed: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let prediction = GuidedBlindPrediction {
            visit_id: visit_id.clone(),
            observed_at_unix_ns: observed_at_ns,
            decision: self.current_wifi_prediction.clone(),
            receiver_ablation_predictions,
        };
        let prediction_bytes = deterministic_pretty_json(&prediction)
            .map_err(|error| format!("could not freeze blind prediction: {error}"))?;
        let truth = guided_blind_truth(&zone, visit_id, radar_position_mm, &prediction_bytes);
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "missing blind session".to_string())?;
        session.blind_predictions.push(prediction);
        session.blind_truth.push(truth);
        Ok(())
    }

    fn finalize_blind_evaluation(&mut self) -> Result<(), String> {
        let context = self
            .experiment
            .as_ref()
            .ok_or_else(|| "missing sealed experiment context".to_string())?
            .clone();
        let index_sha256 = self
            .position_index_sha256
            .clone()
            .ok_or_else(|| "blind evaluation has no frozen position index".to_string())?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "missing blind session".to_string())?;

        let predictions = GuidedPredictionsArtifact {
            schema_version: 2,
            kind: "ruview.mmwave-guided-position-predictions".to_string(),
            setup_sha256: context.setup_sha256.clone(),
            index_sha256: index_sha256.clone(),
            predictions: session.blind_predictions.clone(),
        };
        let predictions_path = session
            .recording_path
            .with_file_name(format!("{}.predictions.json", session.id));
        write_pretty_json_no_clobber(&predictions_path, &predictions)
            .map_err(|error| error.to_string())?;
        let predictions_sha256 =
            sha256_file(&predictions_path).map_err(|error| error.to_string())?;

        let truth = GuidedTruthArtifact {
            schema_version: 1,
            kind: "ruview.mmwave-guided-position-truth".to_string(),
            setup_sha256: context.setup_sha256.clone(),
            predictions_sha256: predictions_sha256.clone(),
            items: session.blind_truth.clone(),
        };
        let truth_path = session
            .recording_path
            .with_file_name(format!("{}.truth.json", session.id));
        write_pretty_json_no_clobber(&truth_path, &truth).map_err(|error| error.to_string())?;
        let truth_sha256 = sha256_file(&truth_path).map_err(|error| error.to_string())?;

        let report = build_blind_report(
            &context.setup_sha256,
            &index_sha256,
            &predictions_sha256,
            &truth_sha256,
            &session.blind_predictions,
            &session.blind_truth,
            &session.trajectory,
            &self.zones,
        )?;
        let report_path = session
            .recording_path
            .with_file_name(format!("{}.evaluation.json", session.id));
        write_pretty_json_no_clobber(&report_path, &report).map_err(|error| error.to_string())?;
        let report_sha256 = sha256_file(&report_path).map_err(|error| error.to_string())?;
        self.candidate_requires_validation = report.verdict != "PASS";
        self.blind_verdict = Some(report.verdict.clone());
        self.blind_report_sha256 = Some(report_sha256);
        Ok(())
    }

    fn write_session_packet(
        &mut self,
        original_bytes: &[u8],
        host_time: &HostTimestamp,
    ) -> Result<(), String> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };
        let result = (|| {
            let packet: serde_json::Value = serde_json::from_slice(original_bytes)
                .map_err(|error| format!("could not persist radar packet: {error}"))?;
            let record = serde_json::json!({
                "schema_version": 2,
                "host_unix_ns": host_time.host_unix_ns,
                "host_monotonic_ns": host_time.host_monotonic_ns,
                "clock_epoch_id": host_time.clock_epoch_id,
                "packet": packet,
            });
            serde_json::to_writer(&mut session.writer, &record)
                .map_err(|error| format!("could not persist radar packet: {error}"))?;
            session
                .writer
                .write_all(b"\n")
                .map_err(|error| format!("could not persist radar packet: {error}"))?;
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            session.io_error = Some(error.clone());
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn start_session(
        &mut self,
        kind: SessionKind,
        data_dir: &Path,
        now: impl Into<HostTimestamp>,
        policy: CalibrationPolicy,
    ) -> Result<(), String> {
        let now = now.into();
        let policy = policy.validate()?;
        self.validate_session_start(kind)?;
        let previous_expected_mode = self.expected_mode;
        self.expected_mode = Some(match kind {
            SessionKind::Calibration => MeasurementMode::Calibration,
            SessionKind::Blind => MeasurementMode::Reference,
            SessionKind::MmwaveOnly => MeasurementMode::Calibration,
        });
        if let Err(error) = self.validate_live_session_start(kind, now.host_monotonic_ns) {
            self.expected_mode = previous_expected_mode;
            return Err(error);
        }
        let id = format!(
            "mmwave-{}-{}",
            match kind {
                SessionKind::Calibration => "calibration",
                SessionKind::Blind => "blind",
                SessionKind::MmwaveOnly => "only",
            },
            now.host_unix_ns
        );
        let directory = data_dir.join("mmwave");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
        let recording_path = directory.join(format!("{id}.mmwave.jsonl"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&recording_path)
            .map_err(|error| format!("could not create {}: {error}", recording_path.display()))?;
        let csi_recording_path = directory.join(format!("{id}.raw-csi.v2.jsonl"));
        let csi_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&csi_recording_path)
            .map_err(|error| {
                let _ = std::fs::remove_file(&recording_path);
                format!("could not create {}: {error}", csi_recording_path.display())
            })?;

        let manifest_path = directory.join(format!("{id}.manifest.json"));
        let phase = match kind {
            SessionKind::Calibration => SessionPhase::EmptyCalibration,
            SessionKind::Blind => SessionPhase::Blind,
            SessionKind::MmwaveOnly => SessionPhase::MmwaveOnly,
        };
        let session_manifest = SessionManifest {
            schema_version: SESSION_MANIFEST_SCHEMA_VERSION,
            id: id.clone(),
            kind,
            lifecycle: SessionLifecycle::Active,
            phase,
            started_at_unix_ns: now.host_unix_ns,
            updated_at_unix_ns: now.host_unix_ns,
            recording_path: recording_path.display().to_string(),
            csi_recording_path: csi_recording_path.display().to_string(),
            aligned_samples: 0,
            rejected_samples: 0,
            empty_duration_seconds: matches!(kind, SessionKind::Calibration | SessionKind::MmwaveOnly)
                .then_some(policy.empty_calibration_seconds),
            empty_validity: None,
            mmwave_only_validity: None,
            error: None,
        };
        if let Err(error) = write_session_manifest(&manifest_path, &session_manifest) {
            let _ = std::fs::remove_file(&recording_path);
            let _ = std::fs::remove_file(&csi_recording_path);
            self.expected_mode = previous_expected_mode;
            return Err(error);
        }
        if kind == SessionKind::Calibration {
            self.zone_count = policy.zone_count;
            self.coverage.clear();
            self.zones.clear();
            self.blind_index = None;
            self.position_index_sha256 = None;
            // Starting a new calibration invalidates any earlier public result
            // immediately. Aborting or failing later must not silently restore
            // the previous deployment model.
            self.candidate_requires_validation = true;
        } else if kind == SessionKind::Blind {
            self.blind_report_sha256 = None;
            self.blind_verdict = None;
            for zone in &mut self.zones {
                zone.blind_visits = 0;
            }
        }
        self.session = Some(Session {
            id,
            kind,
            phase: match kind {
                SessionKind::Calibration => SessionPhase::EmptyCalibration,
                SessionKind::Blind => SessionPhase::Blind,
                SessionKind::MmwaveOnly => SessionPhase::MmwaveOnly,
            },
            started_at_ns: now.host_unix_ns,
            stable_candidate: None,
            must_exit_zone: None,
            recording_path,
            manifest_path,
            writer: BufWriter::new(file),
            csi_recording_path,
            csi_writer: BufWriter::new(csi_file),
            aligned_samples: 0,
            rejected_samples: 0,
            empty_started_at_ns: matches!(kind, SessionKind::Calibration | SessionKind::MmwaveOnly)
                .then_some(now.host_monotonic_ns),
            empty_duration_ns: policy.empty_calibration_seconds * 1_000_000_000,
            empty_frames: Vec::new(),
            empty_outside_room_targets: 0,
            empty_in_room_targets: 0,
            empty_multi_target_packets: 0,
            empty_invalid_packets: 0,
            empty_sequence_gaps: 0,
            empty_reboots: 0,
            empty_radar_packets: 0,
            empty_last_radar_packet_ns: None,
            empty_max_radar_gap_ns: 0,
            empty_validity: None,
            mmwave_only_validity: None,
            candidate_frames: Vec::new(),
            candidate_positions_mm: Vec::new(),
            candidate_radar: Vec::new(),
            empty_reference: None,
            training_blocks: Vec::new(),
            receiver_grids: None,
            blind_predictions: Vec::new(),
            blind_truth: Vec::new(),
            trajectory: Vec::new(),
            dataset_records: Vec::new(),
            clock_epoch_id: now.clock_epoch_id,
            io_error: None,
            mmwave_only_radar_packets: 0,
            mmwave_only_valid_target_packets: 0,
            mmwave_only_no_target_packets: 0,
            mmwave_only_outside_room_targets: 0,
            mmwave_only_multi_target_packets: 0,
            mmwave_only_invalid_packets: 0,
            mmwave_only_sequence_gaps: 0,
            mmwave_only_reboots: 0,
            mmwave_only_last_radar_packet_ns: None,
            mmwave_only_max_radar_gap_ns: 0,
        });
        Ok(())
    }

    pub(crate) fn validate_session_start(&self, kind: SessionKind) -> Result<(), String> {
        if self.session.is_some() {
            return Err("an mmWave session is already active".to_string());
        }
        if self.room_dimensions_mm.is_none() {
            return Err("a sealed room geometry is required".to_string());
        }
        if kind != SessionKind::MmwaveOnly && self.experiment.is_none() {
            return Err("a sealed schema-v2 setup with mmWave identity is required".to_string());
        }
        if kind == SessionKind::MmwaveOnly
            && self.cad_profile.is_none()
            && self.expected_node.is_none()
        {
            return Err("a saved CAD profile or sealed mmWave geometry is required".to_string());
        }
        if kind == SessionKind::Blind
            && (self.zones.len() != self.zone_count
                || self
                    .zones
                    .iter()
                    .any(|zone| zone.training_blocks < BLOCKS_PER_ZONE))
        {
            return Err(format!(
                "blind evaluation requires {} fully trained zones",
                self.zone_count
            ));
        }
        if kind == SessionKind::Blind
            && (self.blind_index.is_none() || self.position_index_sha256.is_none())
        {
            return Err(
                "blind evaluation requires the frozen WiFi position index from this calibration"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub(crate) fn validate_live_session_start(
        &self,
        kind: SessionKind,
        now_monotonic_ns: u64,
    ) -> Result<(), String> {
        self.validate_session_start(kind)?;
        let preflight = match kind {
            SessionKind::MmwaveOnly => self.radar_preflight(now_monotonic_ns),
            SessionKind::Calibration | SessionKind::Blind => self.preflight(now_monotonic_ns),
        };
        if !preflight.ready {
            let blockers = preflight
                .gates
                .iter()
                .filter(|gate| !gate.pass)
                .map(|gate| gate.id)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("mmWave preflight is not ready: {blockers}"));
        }
        Ok(())
    }

    pub(crate) fn stop_session(&mut self) -> Result<(), String> {
        let Some(session) = self.session.as_mut() else {
            return Err("no mmWave session is active".to_string());
        };
        if let Err(error) = session.writer.flush() {
            let message = format!("could not flush radar recording: {error}");
            session.io_error = Some(message.clone());
            let _ = self.persist_current_session_manifest(
                SessionLifecycle::Error,
                Some(message.clone()),
                now_unix_ns(),
            );
            return Err(message);
        }
        if let Err(error) = session.csi_writer.flush() {
            let message = format!("could not flush CSI recording: {error}");
            session.io_error = Some(message.clone());
            let _ = self.persist_current_session_manifest(
                SessionLifecycle::Error,
                Some(message.clone()),
                now_unix_ns(),
            );
            return Err(message);
        }
        if let Some(error) = session.io_error.clone() {
            let _ = self.persist_current_session_manifest(
                SessionLifecycle::Error,
                Some(error.clone()),
                now_unix_ns(),
            );
            return Err(error);
        }
        if let Err(error) = self.persist_current_session_manifest(
            SessionLifecycle::Stopped,
            None,
            now_unix_ns(),
        ) {
            if let Some(session) = &mut self.session {
                session.io_error = Some(error.clone());
            }
            return Err(error);
        }
        self.session.take();
        self.expected_mode = None;
        Ok(())
    }

    pub(crate) fn status(&self, now_ns: u64) -> MmwaveStatus {
        let age_ns = self
            .last_transport_packet_ns
            .map(|seen| now_ns.saturating_sub(seen));
        let stale = age_ns.is_some_and(|age| age > STALE_AFTER_NS);
        let transport = self.transport_metrics.snapshot();
        let mut reject_reasons_window = BTreeMap::new();
        for (at_ns, category) in &self.recent_rejections {
            if now_ns.saturating_sub(*at_ns) <= REJECTION_WINDOW_NS {
                *reject_reasons_window.entry(category.clone()).or_default() += 1;
            }
        }
        MmwaveStatus {
            cad_profile: self.cad_profile.clone(),
            cad_profile_error: self.cad_profile_error.clone(),
            udp_port: self.udp_port,
            state: if stale { LinkState::Stale } else { self.state },
            reason: if stale {
                "The last mmWave packet is stale.".to_string()
            } else {
                self.reason.clone()
            },
            configured: self.control.is_some(),
            setup_sealed: self.expected_node.is_some() && self.experiment.is_some(),
            room_dimensions_m: self
                .experiment
                .as_ref()
                .map(|experiment| experiment.geometry.room_dimensions_m)
                .or_else(|| self.cad_profile.as_ref().map(|cad| cad.room_dimensions_m)),
            mounting_position_m: self
                .expected_node
                .as_ref()
                .and_then(|node| node.mounting_position_m)
                .or_else(|| self.cad_profile.as_ref().map(|cad| cad.mounting_position_m)),
            receiver_positions_m: self
                .experiment
                .as_ref()
                .map(|experiment| experiment.geometry.rx_positions_m.clone())
                .or_else(|| {
                    self.cad_profile.as_ref().and_then(|cad| {
                        (!cad.receiver_positions_m.is_empty())
                            .then(|| cad.receiver_positions_m.clone())
                    })
                }),
            node_id: self.node_id.clone(),
            mode: self.mode,
            expected_mode: self.expected_mode,
            boot_id: self.boot_id,
            sequence: self.sequence,
            packet_age_ms: age_ns.map(|age| age / 1_000_000),
            target_count: self.target_count,
            target_raw_position_mm: self.target_raw_position_mm,
            target_position_mm: self.target_position_mm,
            targets: self.targets.clone(),
            packets_received: self.packets_received,
            packets_rejected: self.packets_rejected,
            packets_lost: self.packets_lost,
            raw_udp_packets: transport.raw_udp_packets,
            reject_reasons: self.reject_reasons.clone(),
            packets_rejected_window: reject_reasons_window.values().sum(),
            reject_reasons_window,
            last_rejection: self.last_rejection.as_ref().map(|rejection| {
                PacketRejectionStatus {
                    category: rejection.category.clone(),
                    reason: rejection.reason.clone(),
                    age_ms: now_ns
                        .saturating_sub(rejection.at_monotonic_ns)
                        / 1_000_000,
                    raw_position_mm: rejection.raw_position_mm,
                    position_mm: rejection.position_mm,
                }
            }),
            last_sequence_gap: self.last_sequence_gap.as_ref().map(|gap| SequenceGapStatus {
                expected_sequence: gap.expected_sequence,
                received_sequence: gap.received_sequence,
                missing_packets: gap.missing_packets,
                age_ms: now_ns.saturating_sub(gap.at_monotonic_ns) / 1_000_000,
            }),
            transport,
            reboot_count: self.reboot_count,
            uart_bytes_received: None,
            radar_frames_valid: None,
            udp_packets_sent: None,
            udp_send_failures: None,
            udp_send_failures_window: None,
            node_status_error: None,
            node_control: NodeControlStatus {
                url_configured: self.node_url_configured,
                token_configured: self.node_token_configured,
                ..NodeControlStatus::default()
            },
            transform: self.effective_transform(),
            source_transform: self.last_transform.clone(),
            yaw_calibration: self.yaw_calibration.clone(),
            coverage_cells: self.coverage.len(),
            zones: self.zones.clone(),
            zone_count: self.zone_count,
            recommended_zone_id: self.recommended_zone_id(),
            session: self
                .session
                .as_ref()
                .map(|session| session_status(session, now_ns))
                .or_else(|| self.restored_session.clone()),
            fixed_point_calibration: self.fixed_point_status(),
            position_index_sha256: self.position_index_sha256.clone(),
            position_live_approved: self.position_index_sha256.is_some()
                && self.position_publication_allowed(),
            blind_report_sha256: self.blind_report_sha256.clone(),
            blind_verdict: self.blind_verdict.clone(),
            preflight: self.preflight(now_ns),
            radar_preflight: self.radar_preflight(now_ns),
            fixed_point_preflight: self.fixed_point_preflight(now_ns),
        }
    }

    fn fixed_point_preflight(&self, now_ns: u64) -> PreflightStatus {
        let mut preflight = self.radar_preflight(now_ns);
        preflight
            .gates
            .retain(|gate| gate.id != "node_control_configured");
        preflight.ready = preflight.gates.iter().all(|gate| gate.pass);
        preflight
    }

    fn radar_preflight(&self, now_ns: u64) -> PreflightStatus {
        let sequence_fault_in_window = self.last_radar_sequence_fault_ns.is_some_and(|seen| {
            now_ns.saturating_sub(seen) <= PREFLIGHT_WINDOW_NS
        });
        let reboot_in_window = self.last_radar_reboot_ns.is_some_and(|seen| {
            now_ns.saturating_sub(seen) <= PREFLIGHT_WINDOW_NS
        });
        let mode_matches = self
            .expected_mode
            .is_none_or(|expected| self.mode == Some(expected));
        let transform_ready = self.cad_profile.is_some() || self.expected_node.is_some();
        let gates = vec![
            PreflightGate {
                id: "node_control_configured",
                pass: self.control.is_some(),
                detail: "mmWave node control URL and bearer token are configured".to_string(),
            },
            PreflightGate {
                id: "cad_profile_active",
                pass: transform_ready,
                detail: "a saved CAD profile or sealed transform is active".to_string(),
            },
            PreflightGate {
                id: "radar_stream_fresh",
                pass: self.last_transport_packet_ns.is_some_and(|seen| {
                    now_ns.saturating_sub(seen) <= PREFLIGHT_FRESH_NS && mode_matches
                }),
                detail: format!(
                    "a valid radar packet must be fresh (age_ms={}, mode_matches={mode_matches})",
                    self.last_transport_packet_ns
                        .map(|seen| now_ns.saturating_sub(seen) / 1_000_000)
                        .unwrap_or(u64::MAX),
                ),
            },
            PreflightGate {
                id: "radar_sequence_loss_free",
                pass: !sequence_fault_in_window && !reboot_in_window,
                detail: format!(
                    "radar packet gaps={} reboots={} (25-s window: sequence_fault={} reboot={})",
                    self.packets_lost,
                    self.reboot_count,
                    sequence_fault_in_window,
                    reboot_in_window
                ),
            },
        ];
        PreflightStatus {
            ready: gates.iter().all(|gate| gate.pass),
            observation_window_ms: PREFLIGHT_WINDOW_NS / 1_000_000,
            gates,
        }
    }

    fn preflight(&self, now_ns: u64) -> PreflightStatus {
        // Keep lifetime counters for diagnosis; only faults in the current
        // observation window may block a new calibration run.
        let sequence_fault_in_window = self.last_radar_sequence_fault_ns.is_some_and(|seen| {
            now_ns.saturating_sub(seen) <= PREFLIGHT_WINDOW_NS
        });
        let reboot_in_window = self.last_radar_reboot_ns.is_some_and(|seen| {
            now_ns.saturating_sub(seen) <= PREFLIGHT_WINDOW_NS
        });
        let mut gates = vec![
            PreflightGate {
                id: "node_control_configured",
                pass: self.control.is_some(),
                detail: "mmWave node control URL and bearer token are configured".to_string(),
            },
            PreflightGate {
                id: "setup_and_transform_sealed",
                pass: self.expected_node.is_some()
                    && self.experiment.is_some()
                    && self.last_transform.as_ref()
                        == self.expected_node.as_ref().map(|node| &node.transform),
                detail: "schema-v2 setup, node identity and transform must match".to_string(),
            },
            PreflightGate {
                id: "radar_stream_fresh",
                pass: self.last_transport_packet_ns.is_some_and(|seen| {
                    now_ns.saturating_sub(seen) <= PREFLIGHT_FRESH_NS
                        && self
                            .expected_mode
                            .is_none_or(|expected| self.mode == Some(expected))
                }),
                detail: format!(
                    "a valid transport packet must be fresh (age_ms={}, mode_matches={})",
                    self.last_transport_packet_ns
                        .map(|seen| now_ns.saturating_sub(seen) / 1_000_000)
                        .unwrap_or(u64::MAX),
                    self.expected_mode
                        .is_none_or(|expected| self.mode == Some(expected))
                ),
            },
            PreflightGate {
                id: "radar_sequence_loss_free",
                pass: !sequence_fault_in_window && !reboot_in_window,
                detail: format!(
                    "radar packet gaps={} reboots={} (25-s window: sequence_fault={} reboot={})",
                    self.packets_lost,
                    self.reboot_count,
                    sequence_fault_in_window,
                    reboot_in_window
                ),
            },
            PreflightGate {
                id: "csi_v2_clock",
                pass: self.csi_clock_rejections == 0,
                detail: format!(
                    "CSI frames without monotonic v2 timestamps={}",
                    self.csi_clock_rejections
                ),
            },
        ];
        for (index, observations) in self.rx_preflight.iter().enumerate() {
            let recent: Vec<_> = observations
                .iter()
                .filter(|item| now_ns.saturating_sub(item.monotonic_ns) <= PREFLIGHT_WINDOW_NS)
                .collect();
            let span = recent
                .first()
                .zip(recent.last())
                .map(|(first, last)| last.monotonic_ns.saturating_sub(first.monotonic_ns))
                .unwrap_or(0);
            let coverage_age = recent
                .first()
                .map(|first| now_ns.saturating_sub(first.monotonic_ns))
                .unwrap_or(0);
            let grids = recent
                .iter()
                .map(|item| item.grid)
                .collect::<BTreeSet<_>>();
            let stable_grid = grids.len() == 1;
            let grid_detail = grids
                .iter()
                .map(|grid| {
                    format!(
                        "{}MHz/{}ant/{}sc/ppdu{}/flags0x{:02x}",
                        grid.center_frequency_mhz,
                        grid.antenna_count,
                        grid.subcarrier_count,
                        grid.ppdu_type,
                        grid.layout_flags,
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let source_bound = !recent.is_empty() && recent.iter().all(|item| item.source_bound);
            let clock_matches = self.clock_epoch_id.as_deref().is_some_and(|epoch| {
                recent.iter().all(|item| item.clock_epoch_id == epoch)
            });
            let fresh = recent
                .last()
                .is_some_and(|item| now_ns.saturating_sub(item.monotonic_ns) <= PREFLIGHT_FRESH_NS);
            gates.push(PreflightGate {
                id: match index {
                    0 => "rx1_25s_ready",
                    1 => "rx2_25s_ready",
                    2 => "rx3_25s_ready",
                    _ => "rx4_25s_ready",
                },
                pass: recent.len() >= PREFLIGHT_MIN_FRAMES_PER_RX
                    && coverage_age >= PREFLIGHT_WINDOW_NS.saturating_sub(PREFLIGHT_FRESH_NS)
                    && stable_grid
                    && source_bound
                    && clock_matches
                    && fresh,
                detail: format!(
                    "RX{} frames={} span_ms={} coverage_ms={} stable_grid={} grids=[{}] source_bound={} clock_matches={} fresh={}",
                    index + 1,
                    recent.len(),
                    span / 1_000_000,
                    coverage_age / 1_000_000,
                    stable_grid,
                    grid_detail,
                    source_bound,
                    clock_matches,
                    fresh
                ),
            });
        }
        PreflightStatus {
            ready: gates.iter().all(|gate| gate.pass),
            observation_window_ms: PREFLIGHT_WINDOW_NS / 1_000_000,
            gates,
        }
    }

    fn recommended_zone_id(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let incomplete = |zone: &Zone| match session.kind {
            SessionKind::Calibration => zone.training_blocks < BLOCKS_PER_ZONE,
            SessionKind::Blind => zone.blind_visits < BLIND_VISITS_PER_ZONE,
            SessionKind::MmwaveOnly => false,
        };
        let zones = self.zones.iter().filter(|zone| incomplete(zone));
        let zone = match self.target_position_mm {
            Some(position) => zones.min_by_key(|zone| squared_distance(zone.center_mm, position)),
            None => zones.min_by(|left, right| left.id.cmp(&right.id)),
        }?;
        Some(zone.id.clone())
    }
}

fn session_status(session: &Session, now_ns: u64) -> SessionStatus {
    let empty_duration_seconds = matches!(session.kind, SessionKind::Calibration | SessionKind::MmwaveOnly)
        .then_some(session.empty_duration_ns / 1_000_000_000);
    let empty_remaining_seconds = matches!(session.phase, SessionPhase::EmptyCalibration | SessionPhase::MmwaveOnly).then(|| {
        let elapsed_ns = session
            .empty_started_at_ns
            .map(|started| now_ns.saturating_sub(started))
            .unwrap_or(0);
        session
            .empty_duration_ns
            .saturating_sub(elapsed_ns)
            .div_ceil(1_000_000_000)
    });
    let next_instruction = match session.phase {
        SessionPhase::EmptyCalibration => {
            format!(
                "Leave the room empty until the {}-second reference is complete.",
                empty_duration_seconds.unwrap_or(DEFAULT_EMPTY_CALIBRATION_SECONDS)
            )
        }
        SessionPhase::Coverage => "Walk through every accessible part of the room.".to_string(),
        SessionPhase::Training => {
            "Follow the highlighted zones and pause for five seconds.".to_string()
        }
        SessionPhase::Blind => {
            "Visit every zone twice; WiFi predictions remain frozen.".to_string()
        }
        SessionPhase::MmwaveOnly => {
            "Keep the room empty while the radar-only transport and geometry are checked.".to_string()
        }
        SessionPhase::Complete => {
            "Collection is complete. Stop the session to seal its recording.".to_string()
        }
    };
    SessionStatus {
        id: session.id.clone(),
        kind: session.kind,
        phase: session.phase,
        lifecycle: if session.io_error.is_some() {
            SessionLifecycle::Error
        } else if session.phase == SessionPhase::Complete {
            SessionLifecycle::Complete
        } else {
            SessionLifecycle::Active
        },
        started_at_unix_ns: session.started_at_ns,
        recording_path: session.recording_path.display().to_string(),
        csi_recording_path: session.csi_recording_path.display().to_string(),
        aligned_samples: session.aligned_samples,
        rejected_samples: session.rejected_samples,
        empty_duration_seconds,
        empty_remaining_seconds,
        empty_validity: session.empty_validity.clone(),
        mmwave_only_validity: session.mmwave_only_validity.clone(),
        error: session.io_error.clone(),
        next_instruction,
    }
}

fn session_manifest_from_session(
    session: &Session,
    lifecycle: SessionLifecycle,
    updated_at_unix_ns: u64,
    error: Option<String>,
) -> SessionManifest {
    SessionManifest {
        schema_version: SESSION_MANIFEST_SCHEMA_VERSION,
        id: session.id.clone(),
        kind: session.kind,
        lifecycle,
        phase: session.phase,
        started_at_unix_ns: session.started_at_ns,
        updated_at_unix_ns,
        recording_path: session.recording_path.display().to_string(),
        csi_recording_path: session.csi_recording_path.display().to_string(),
        aligned_samples: session.aligned_samples,
        rejected_samples: session.rejected_samples,
        empty_duration_seconds: matches!(session.kind, SessionKind::Calibration | SessionKind::MmwaveOnly)
            .then_some(session.empty_duration_ns / 1_000_000_000),
        empty_validity: session.empty_validity.clone(),
        mmwave_only_validity: session.mmwave_only_validity.clone(),
        error,
    }
}

fn write_session_manifest(path: &Path, manifest: &SessionManifest) -> Result<(), String> {
    // Session manifests are internal recovery metadata. Unlike published
    // calibration artifacts, they intentionally contain the two recording
    // paths needed to recover an interrupted session.
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("could not serialize session manifest: {error}"))?;
    let temporary_path = path.with_extension(format!(
        "manifest.tmp-{}-{}",
        std::process::id(),
        now_unix_ns()
    ));
    let result = (|| {
        let mut file = File::create(&temporary_path)
            .map_err(|error| format!("could not create {}: {error}", temporary_path.display()))?;
        file.write_all(&bytes)
            .map_err(|error| format!("could not write {}: {error}", temporary_path.display()))?;
        file.sync_all()
            .map_err(|error| format!("could not sync {}: {error}", temporary_path.display()))?;
        std::fs::rename(&temporary_path, path).map_err(|error| {
            format!(
                "could not replace session manifest {}: {error}",
                path.display()
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

fn session_status_from_manifest(manifest: &SessionManifest) -> SessionStatus {
    let next_instruction = match manifest.phase {
        SessionPhase::EmptyCalibration => format!(
            "Leave the room empty until the {}-second reference is complete.",
            manifest.empty_duration_seconds.unwrap_or(DEFAULT_EMPTY_CALIBRATION_SECONDS)
        ),
        SessionPhase::Coverage => "Walk through every accessible part of the room.".to_string(),
        SessionPhase::Training => {
            "Follow the highlighted zones and pause for five seconds.".to_string()
        }
        SessionPhase::Blind => "Visit every zone twice; WiFi predictions remain frozen.".to_string(),
        SessionPhase::MmwaveOnly => {
            "Keep the room empty while the radar-only transport and geometry are checked.".to_string()
        }
        SessionPhase::Complete => {
            "Collection is complete. Stop the session to seal its recording.".to_string()
        }
    };
    SessionStatus {
        id: manifest.id.clone(),
        kind: manifest.kind,
        phase: manifest.phase,
        lifecycle: manifest.lifecycle,
        started_at_unix_ns: manifest.started_at_unix_ns,
        recording_path: manifest.recording_path.clone(),
        csi_recording_path: manifest.csi_recording_path.clone(),
        aligned_samples: manifest.aligned_samples,
        rejected_samples: manifest.rejected_samples,
        empty_duration_seconds: manifest.empty_duration_seconds,
        empty_remaining_seconds: None,
        empty_validity: manifest.empty_validity.clone(),
        mmwave_only_validity: manifest.mmwave_only_validity.clone(),
        error: manifest.error.clone(),
        next_instruction,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_blind_report(
    setup_sha256: &str,
    index_sha256: &str,
    predictions_sha256: &str,
    truth_sha256: &str,
    predictions: &[GuidedBlindPrediction],
    truth: &[GuidedBlindTruth],
    trajectory: &[[i32; 2]],
    zones: &[Zone],
) -> Result<GuidedBlindReport, String> {
    if predictions.len() != truth.len() {
        return Err("blind predictions and truth have different lengths".to_string());
    }
    let mut decided = 0usize;
    let mut correct = 0usize;
    let mut errors = Vec::new();
    let mut receiver_correct = [0usize; 4];
    let mut receiver_errors: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::new());
    for (prediction, truth) in predictions.iter().zip(truth) {
        if prediction.visit_id != truth.visit_id {
            return Err("blind prediction/truth visit IDs do not match".to_string());
        }
        let bytes = deterministic_pretty_json(prediction)
            .map_err(|error| format!("could not verify frozen prediction: {error}"))?;
        if sha256_bytes(&bytes) != truth.prediction_sha256 {
            return Err("blind truth does not match its frozen prediction".to_string());
        }
        if prediction.receiver_ablation_predictions.len() != 4 {
            return Err("blind prediction must freeze exactly RX1-RX4 ablations".to_string());
        }
        for (receiver_index, receiver) in
            prediction.receiver_ablation_predictions.iter().enumerate()
        {
            let expected_rx_id = u8::try_from(receiver_index + 1)
                .map_err(|_| "receiver index does not fit u8".to_string())?;
            if receiver.rx_id != expected_rx_id
                || receiver
                    .coordinates_m
                    .iter()
                    .any(|coordinate| !coordinate.is_finite())
            {
                return Err("blind receiver ablations must be finite RX1-RX4".to_string());
            }
            if receiver.point_id == truth.expected_point_id {
                receiver_correct[receiver_index] += 1;
            }
            receiver_errors[receiver_index].push(floor_distance_m(
                receiver.coordinates_m,
                truth.radar_coordinates_m,
            ));
        }
        if let GuidedPredictionDecision::Position {
            point_id,
            coordinates_m,
        } = &prediction.decision
        {
            decided += 1;
            if point_id == &truth.expected_point_id {
                correct += 1;
            }
            errors.push(floor_distance_m(*coordinates_m, truth.radar_coordinates_m));
        }
    }
    let total = predictions.len();
    let abstentions = total.saturating_sub(decided);
    let accuracy_decided = (decided > 0).then_some(correct as f64 / decided as f64);
    let median_floor_error_m = percentile(&errors, 0.5);
    let maximum_floor_error_m = errors.iter().copied().reduce(f64::max);
    let trajectory_errors: Vec<f64> = trajectory
        .iter()
        .filter_map(|position| {
            zones
                .iter()
                .map(|zone| squared_distance(zone.center_mm, *position) as f64)
                .reduce(f64::min)
                .map(|distance_squared| distance_squared.sqrt() / 1000.0)
        })
        .collect();
    let trajectory_cells = trajectory
        .iter()
        .map(|position| {
            (
                position[0].div_euclid(COVERAGE_CELL_MM),
                position[1].div_euclid(COVERAGE_CELL_MM),
            )
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let expected_visits = zones.len() * usize::from(BLIND_VISITS_PER_ZONE);
    let minimum_decided = (expected_visits * 8 + 8) / 9;
    let minimum_correct = (expected_visits * 5 + 5) / 6;
    let maximum_abstentions = (expected_visits + 8) / 9;
    let receiver_ablation_metrics = receiver_errors
        .iter()
        .enumerate()
        .map(|(receiver_index, errors)| GuidedReceiverMetrics {
            rx_id: u8::try_from(receiver_index + 1).expect("RX1-RX4 fit u8"),
            total,
            correct: receiver_correct[receiver_index],
            nearest_accuracy: (total > 0)
                .then_some(receiver_correct[receiver_index] as f64 / total as f64),
            median_floor_error_m: percentile(errors, 0.5),
            maximum_floor_error_m: errors.iter().copied().reduce(f64::max),
        })
        .collect();
    let gates = GuidedBlindGates {
        expected_visit_count_met: total == expected_visits,
        minimum_decided_count_met: decided >= minimum_decided,
        minimum_correct_count_met: correct >= minimum_correct,
        decided_accuracy_at_least_ninety_percent: accuracy_decided
            .is_some_and(|accuracy| accuracy >= 0.90),
        abstention_limit_met: abstentions <= maximum_abstentions,
        median_error_at_most_0_75_m: median_floor_error_m.is_some_and(|error| error <= 0.75),
        maximum_error_at_most_1_30_m: maximum_floor_error_m.is_some_and(|error| error <= 1.30),
    };
    let pass = gates.expected_visit_count_met
        && gates.minimum_decided_count_met
        && gates.minimum_correct_count_met
        && gates.decided_accuracy_at_least_ninety_percent
        && gates.abstention_limit_met
        && gates.median_error_at_most_0_75_m
        && gates.maximum_error_at_most_1_30_m;
    Ok(GuidedBlindReport {
        schema_version: 2,
        kind: "ruview.mmwave-guided-position-evaluation".to_string(),
        setup_sha256: setup_sha256.to_string(),
        index_sha256: index_sha256.to_string(),
        predictions_sha256: predictions_sha256.to_string(),
        truth_sha256: truth_sha256.to_string(),
        total,
        decided,
        correct,
        abstentions,
        accuracy_decided,
        median_floor_error_m,
        maximum_floor_error_m,
        trajectory_coverage_cells: trajectory_cells,
        trajectory_median_zone_error_m: percentile(&trajectory_errors, 0.5),
        trajectory_p95_zone_error_m: percentile(&trajectory_errors, 0.95),
        receiver_ablation_metrics,
        gates,
        verdict: if pass { "PASS" } else { "FAIL" }.to_string(),
    })
}

fn floor_distance_m(left: [f64; 3], right: [f64; 3]) -> f64 {
    ((left[0] - right[0]).powi(2) + (left[2] - right[2]).powi(2)).sqrt()
}

fn guided_blind_truth(
    zone: &Zone,
    visit_id: String,
    radar_position_mm: [i32; 2],
    prediction_bytes: &[u8],
) -> GuidedBlindTruth {
    GuidedBlindTruth {
        visit_id,
        expected_point_id: zone.id.clone(),
        radar_coordinates_m: [
            f64::from(radar_position_mm[0]) / 1000.0,
            0.0,
            f64::from(radar_position_mm[1]) / 1000.0,
        ],
        prediction_sha256: sha256_bytes(prediction_bytes),
    }
}

fn median_position_mm(positions: &[[i32; 2]]) -> Option<[i32; 2]> {
    if positions.is_empty() {
        return None;
    }
    let mut x: Vec<i32> = positions.iter().map(|position| position[0]).collect();
    let mut z: Vec<i32> = positions.iter().map(|position| position[1]).collect();
    x.sort_unstable();
    z.sort_unstable();
    Some([x[x.len() / 2], z[z.len() / 2]])
}

fn normalize_yaw_mdeg(value: i32) -> i32 {
    value.rem_euclid(360_000)
}

fn normalize_signed_yaw_mdeg(value: i32) -> i32 {
    let normalized = value.rem_euclid(360_000);
    if normalized > 180_000 {
        normalized - 360_000
    } else {
        normalized
    }
}

fn yaw_errors_mm(
    points: &[(String, [i32; 2], &KnownPointCheck)],
    transform: &CoordinateFrame,
    yaw_mdeg: i32,
) -> Vec<f64> {
    let yaw = (f64::from(yaw_mdeg) / 1_000.0).to_radians();
    points
        .iter()
        .map(|(_, expected, check)| {
            let right = f64::from(check.observed_raw_position_mm[0])
                * if transform.raw_x_inverted { -1.0 } else { 1.0 };
            let forward = f64::from(check.observed_raw_position_mm[1]);
            let x = f64::from(transform.origin_x_mm)
                + forward * yaw.cos()
                - right * yaw.sin();
            let z = f64::from(transform.origin_z_mm)
                + forward * yaw.sin()
                + right * yaw.cos();
            ((x - f64::from(expected[0])).powi(2)
                + (z - f64::from(expected[1])).powi(2))
            .sqrt()
        })
        .collect()
}

fn rms_error_mm(errors: &[f64]) -> f64 {
    if errors.is_empty() {
        return 0.0;
    }
    (errors.iter().map(|error| error.powi(2)).sum::<f64>() / errors.len() as f64).sqrt()
}

fn error_summary_mm(errors: &[f64]) -> (u32, u32) {
    if errors.is_empty() {
        return (0, 0);
    }
    let mut sorted = errors.to_vec();
    sorted.sort_by(f64::total_cmp);
    (
        sorted[sorted.len() / 2].round().max(0.0) as u32,
        sorted.last().copied().unwrap_or(0.0).round().max(0.0) as u32,
    )
}

fn percentile(values: &[f64], quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index = ((sorted.len() - 1) as f64 * quantile).ceil() as usize;
    sorted.get(index).copied()
}

fn validate_packet_shape(packet: &RadarPacket) -> Result<(), String> {
    if packet.schema != MMWAVE_SCHEMA {
        return Err(format!("unsupported mmWave schema {:?}", packet.schema));
    }
    if packet.node_id.trim().is_empty() || packet.node_id.len() > 64 {
        return Err("invalid node_id".to_string());
    }
    if packet.sensor_time_us < 0 || packet.targets.len() != 3 {
        return Err(
            "packet must contain a non-negative sensor time and exactly three target slots"
                .to_string(),
        );
    }
    if packet.coordinate_frame.local != "x_right_y_forward_mm"
        || packet.coordinate_frame.room != "x_length_z_width_mm"
        || packet.coordinate_frame.yaw_mdeg.abs() > 360_000
    {
        return Err("unsupported coordinate frame".to_string());
    }
    for (index, target) in packet.targets.iter().enumerate() {
        if target.slot != index as u8 + 1 {
            return Err("target slots must be exactly 1, 2, 3 in order".to_string());
        }
        if !target.present
            && (target.x_mm != 0
                || target.y_mm != 0
                || target.room_x_mm != 0
                || target.room_z_mm != 0
                || target.speed_cm_s != 0
                || target.resolution_mm != 0)
        {
            return Err(format!(
                "absent target slot {} contains measurements",
                target.slot
            ));
        }
    }
    Ok(())
}

fn select_zones(
    coverage: &BTreeMap<(i32, i32), u32>,
    zone_count: usize,
) -> Result<Vec<Zone>, String> {
    let mut candidates: Vec<([i32; 2], u32)> = coverage
        .iter()
        .filter(|(_, count)| **count >= MIN_CELL_OBSERVATIONS)
        .map(|(&(x, z), &count)| {
            (
                [
                    x * COVERAGE_CELL_MM + COVERAGE_CELL_MM / 2,
                    z * COVERAGE_CELL_MM + COVERAGE_CELL_MM / 2,
                ],
                count,
            )
        })
        .collect();
    candidates
        .sort_by_key(|(position, count)| (std::cmp::Reverse(*count), position[1], position[0]));
    if candidates.len() < zone_count {
        return Err(format!(
            "only {} sufficiently observed cells are available",
            candidates.len()
        ));
    }

    let mut selected = vec![candidates.remove(0).0];
    while selected.len() < zone_count {
        let next = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, (candidate, count))| {
                let minimum = selected
                    .iter()
                    .map(|existing| squared_distance(*existing, *candidate))
                    .min()?;
                (minimum >= i64::from(MIN_ZONE_SEPARATION_MM).pow(2))
                    .then_some((index, minimum, *count, *candidate))
            })
            .max_by_key(|(_, minimum, count, candidate)| {
                (
                    *minimum,
                    *count,
                    std::cmp::Reverse(candidate[1]),
                    std::cmp::Reverse(candidate[0]),
                )
            });
        let Some((index, _, _, position)) = next else {
            return Err(
                format!("accessible coverage cannot provide {zone_count} zones at least 0.75 m apart"),
            );
        };
        selected.push(position);
        candidates.remove(index);
    }
    selected.sort_by_key(|position| (position[1], position[0]));
    Ok(selected
        .into_iter()
        .enumerate()
        .map(|(index, center_mm)| Zone {
            id: format!("Z{:03}", index + 1),
            center_mm,
            training_blocks: 0,
            blind_visits: 0,
        })
        .collect())
}

fn nearest_zone(zones: &[Zone], position: [i32; 2]) -> Option<usize> {
    zones
        .iter()
        .enumerate()
        .map(|(index, zone)| (index, squared_distance(zone.center_mm, position)))
        .filter(|(_, distance)| *distance <= i64::from(ZONE_RADIUS_MM).pow(2))
        .min_by_key(|(_, distance)| *distance)
        .map(|(index, _)| index)
}

fn squared_distance(left: [i32; 2], right: [i32; 2]) -> i64 {
    let dx = i64::from(left[0]) - i64::from(right[0]);
    let dz = i64::from(left[1]) - i64::from(right[1]);
    dx * dx + dz * dz
}

fn metres_to_mm(value: f64) -> Option<i32> {
    value
        .is_finite()
        .then(|| (value * 1000.0).round())
        .filter(|value| *value >= f64::from(i32::MIN) && *value <= f64::from(i32::MAX))
        .map(|value| value as i32)
}

pub(crate) fn now_unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn format_node_request_error(operation: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, _) => format!("{operation} returned HTTP {status}"),
        ureq::Error::Transport(error) => format!("{operation} transport error: {error}"),
    }
}

pub(crate) fn set_node_mode(control: &NodeControl, mode: MeasurementMode) -> Result<(), String> {
    let url = format!("{}/mode", control.base_url.trim_end_matches('/'));
    let response = ureq::put(&url)
        .set("Authorization", &format!("Bearer {}", control.bearer_token))
        .set("Content-Type", "text/plain")
        .send_string(mode.as_str())
        .map_err(|error| format_node_request_error("mmWave mode request", error))?;
    if response.status() != 200 {
        return Err(format!(
            "mmWave mode request returned HTTP {}",
            response.status()
        ));
    }
    Ok(())
}

pub(crate) fn get_node_diagnostics(control: &NodeControl) -> Result<NodeDiagnostics, String> {
    let url = format!("{}/ota/status", control.base_url.trim_end_matches('/'));
    let response = ureq::get(&url)
        .set("Connection", "close")
        .timeout(NODE_STATUS_TIMEOUT)
        .call()
        .map_err(|error| format_node_request_error("mmWave node status request", error))?;
    if response.status() != 200 {
        return Err(format!(
            "mmWave node status request returned HTTP {}",
            response.status()
        ));
    }
    response
        .into_json::<NodeStatusResponse>()
        .map(|status| status.diagnostics)
        .map_err(|error| format!("invalid mmWave node status: {error}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransformRequest {
    pub(crate) origin_x_mm: i32,
    pub(crate) origin_z_mm: i32,
    pub(crate) yaw_mdeg: i32,
    pub(crate) raw_x_inverted: bool,
}

pub(crate) fn set_node_transform(
    control: &NodeControl,
    transform: &TransformRequest,
) -> Result<(), String> {
    if transform.yaw_mdeg.abs() > 360_000
        || transform.origin_x_mm.abs() > 100_000
        || transform.origin_z_mm.abs() > 100_000
    {
        return Err("transform is outside the firmware safety bounds".to_string());
    }
    let url = format!("{}/transform", control.base_url.trim_end_matches('/'));
    let response = ureq::put(&url)
        .set("Authorization", &format!("Bearer {}", control.bearer_token))
        .send_json(serde_json::json!({
            "origin_x_mm": transform.origin_x_mm,
            "origin_z_mm": transform.origin_z_mm,
            "yaw_mdeg": transform.yaw_mdeg,
            "raw_x_inverted": transform.raw_x_inverted,
        }))
        .map_err(|error| format_node_request_error("mmWave transform request", error))?;
    if response.status() != 200 {
        return Err(format!(
            "mmWave transform request returned HTTP {}",
            response.status()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_csi_recording::{
        IqPair, SourceBinding, RAW_CSI_SCHEMA_VERSION, SOURCE_BINDING_REQUIRED_FLAGS,
        TX_SOURCE_BINDING_SCHEME, TX_SOURCE_BINDING_VERSION,
    };
    use std::io::Read as _;

    fn packet(sequence: u32, targets: usize, mode: MeasurementMode) -> Vec<u8> {
        let target = |slot: u8, present: bool| {
            serde_json::json!({
                "slot": slot,
                "present": present,
                "x_mm": if present { 100 } else { 0 },
                "y_mm": if present { 1000 } else { 0 },
                "room_x_mm": if present { 1000 + i32::from(slot) } else { 0 },
                "room_z_mm": if present { 1500 } else { 0 },
                "speed_cm_s": 0,
                "resolution_mm": if present { 10 } else { 0 },
            })
        };
        serde_json::to_vec(&serde_json::json!({
            "schema": MMWAVE_SCHEMA,
            "node_id": "radar-01",
            "mode": mode,
            "boot_id": 7,
            "sequence": sequence,
            "sensor_time_us": i64::from(sequence) * 100_000,
            "unix_time_ms": 0,
            "coordinate_frame": {
                "local": "x_right_y_forward_mm",
                "room": "x_length_z_width_mm",
                "origin_x_mm": 0,
                "origin_z_mm": 0,
                "yaw_mdeg": 0,
                "raw_x_inverted": false
            },
            "targets": [target(1, targets >= 1), target(2, targets >= 2), target(3, targets >= 3)]
        }))
        .unwrap()
    }

    fn packet_with_raw(sequence: u32, raw: [i32; 2]) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::from_slice(&packet(
            sequence,
            1,
            MeasurementMode::Calibration,
        ))
        .unwrap();
        value["targets"][0]["x_mm"] = serde_json::json!(raw[0]);
        value["targets"][0]["y_mm"] = serde_json::json!(raw[1]);
        value["targets"][0]["room_x_mm"] = serde_json::json!(raw[1]);
        value["targets"][0]["room_z_mm"] = serde_json::json!(raw[0]);
        serde_json::to_vec(&value).unwrap()
    }

    fn manager() -> MmwaveManager {
        MmwaveManager::new(DEFAULT_UDP_PORT, Some([4.02, 2.59, 3.44]), None, None, None)
    }

    fn cad_profile(mount: [f64; 3], revision: &str) -> super::super::experiment::SetupProfile {
        super::super::experiment::SetupProfile {
            id: "cad-test".to_string(), label: "CAD test".to_string(), version: 1,
            revision_id: revision.to_string(), profile_sha256: revision.to_string(),
            profile_context_sha256: "test".to_string(), created_at: String::new(), updated_at: String::new(),
            document: serde_json::json!({
                "room_dimensions_m": [4.02, 2.59, 3.44],
                "transmitter": { "id": "TX", "position_m": [2.0, 1.0, 1.5] },
                "receivers": [
                    { "id": "RX1", "position_m": [2.0, 1.0, 1.5] },
                    { "id": "RX2", "position_m": [2.0, 1.0, 1.5] },
                    { "id": "RX3", "position_m": [2.0, 1.0, 1.5] },
                    { "id": "RX4", "position_m": [2.0, 1.0, 1.5] }
                ],
                "mmwave": { "mounting_position_m": mount }
            }),
        }
    }

    #[test]
    fn cad_profile_drives_unsealed_radar_without_rx_and_updates_after_save() {
        let mut manager = MmwaveManager::new(DEFAULT_UDP_PORT, None, None, None, None);
        let bytes = packet(0, 1, MeasurementMode::Calibration);
        manager.apply_cad_profile(&cad_profile([1.0, 1.2, 0.5], "v1"));
        manager.ingest_json(&bytes, 1_000_000_000).unwrap();
        let status = manager.status(1_000_000_000);
        assert_eq!(status.target_raw_position_mm, Some([100, 1000]));
        assert_eq!(status.target_position_mm, Some([2000, 600]));
        assert_eq!(status.room_dimensions_m, Some([4.02, 2.59, 3.44]));
        assert!(!status.setup_sealed);
        assert!(!status.preflight_ready());
        assert!(!status.position_live_approved);

        manager.apply_cad_profile(&cad_profile([2.0, 1.2, 1.0], "v2"));
        assert_eq!(manager.status(1_000_000_000).target_position_mm, None);
        manager.ingest_json(&packet(1, 1, MeasurementMode::Calibration), 1_100_000_000).unwrap();
        let status = manager.status(1_100_000_000);
        assert_eq!(status.target_position_mm, Some([3000, 1100]));
        assert_eq!(status.cad_profile.unwrap().revision_id, "v2");
        // The original firmware packet is untouched and still uses its own origin.
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["coordinate_frame"]["origin_x_mm"], 0);
    }

    #[test]
    fn mmwave_only_session_requires_cad_geometry_but_not_rx_or_sealed_setup() {
        let mut manager = MmwaveManager::new(DEFAULT_UDP_PORT, None, None, None, None);
        assert!(manager
            .validate_session_start(SessionKind::MmwaveOnly)
            .is_err());
        manager.apply_cad_profile(&cad_profile([3.9, 1.5, 3.3], "radar-only-v1"));
        assert!(manager
            .validate_session_start(SessionKind::MmwaveOnly)
            .is_ok());
    }

    #[test]
    fn fixed_point_phase_accepts_persisted_cad_geometry_without_csi_setup() {
        let mut manager = MmwaveManager::new(
            DEFAULT_UDP_PORT,
            None,
            None,
            None,
            None,
        );
        manager.apply_cad_profile(&cad_profile([1.0, 1.2, 0.5], "cad-fixed-points"));
        manager
            .ingest_json(
                &packet(0, 1, MeasurementMode::Calibration),
                1_000_000_000,
            )
            .expect("fresh CAD-preview packet");

        let status = manager
            .start_fixed_point_calibration(1_000_000_000)
            .expect("start fixed points from CAD profile");

        assert_eq!(status.current_anchor_id.as_deref(), Some("RX1"));
        assert_eq!(status.anchors.len(), 5);
        assert!(manager.experiment.is_none());
    }

    #[test]
    fn known_point_check_uses_fresh_median_radar_observations() {
        let mut manager = manager();
        for (sequence, timestamp) in [(0, 1_000_000_000), (1, 1_100_000_000), (2, 1_200_000_000)] {
            manager
                .ingest_json(
                    &packet(sequence, 1, MeasurementMode::Calibration),
                    timestamp,
                )
                .expect("synthetic radar packet");
        }
        let check = manager
            .check_known_point([1_001, 1_500], 10, 500, 1_300_000_000)
            .expect("fresh radar observations");
        assert!(check.pass);
        assert_eq!(check.sample_count, 3);
        assert_eq!(check.observed_position_mm, [1_001, 1_500]);
        assert_eq!(check.median_error_mm, 0);
    }

    #[test]
    fn fixed_point_phase_enforces_rx_order_before_csi_start() {
        let mut manager = sealed_manager();
        manager.control = Some(NodeControl {
            base_url: "http://radar.invalid".to_string(),
            bearer_token: "test-only".to_string(),
        });
        let experiment = manager.experiment.as_mut().expect("sealed experiment");
        experiment.geometry.rx_positions_m = vec![[1.001, 1.0, 1.5]; 4];
        experiment.geometry.tx_position_m = [1.001, 1.0, 1.5];
        manager
            .ingest_json(
                &packet(0, 1, MeasurementMode::Calibration),
                1_000_000_000,
            )
            .expect("fresh radar packet");

        assert!(manager.validate_calibration_phase_two_start().is_err());
        let started = manager
            .start_fixed_point_calibration(1_000_000_000)
            .expect("start fixed-point phase");
        assert_eq!(started.current_anchor_id.as_deref(), Some("RX1"));

        for expected_next in [Some("RX2"), Some("RX3"), Some("RX4"), Some("TX"), None] {
            let status = manager
                .check_current_fixed_point(10, 500, 1_100_000_000)
                .expect("check current fixed point");
            assert_eq!(status.current_anchor_id.as_deref(), expected_next);
        }

        let completed = manager.fixed_point_status().expect("fixed-point status");
        assert_eq!(completed.state, "complete");
        assert!(completed
            .anchors
            .iter()
            .all(|anchor| anchor.state == "complete" && anchor.check.as_ref().is_some_and(|check| check.pass)));
        assert!(manager.validate_calibration_phase_two_start().is_ok());
    }

    #[test]
    fn fixed_points_fit_one_yaw_across_all_rx_and_tx_observations() {
        let mut manager = sealed_manager();
        let raw_points = [
            [-1_000, 500],
            [-1_500, 1_000],
            [-2_000, 500],
            [-1_000, 1_500],
            [-2_000, 1_500],
        ];
        let expected = raw_points
            .map(|raw| [f64::from(-raw[0]) / 1_000.0, 1.0, f64::from(raw[1]) / 1_000.0]);
        let experiment = manager.experiment.as_mut().expect("sealed experiment");
        experiment.geometry.rx_positions_m = expected[..4].to_vec();
        experiment.geometry.tx_position_m = expected[4];
        manager
            .ingest_json(
                &packet(0, 0, MeasurementMode::Calibration),
                1_000_000_000,
            )
            .unwrap();
        manager.start_fixed_point_calibration(1_000_000_000).unwrap();

        for (index, raw) in raw_points.into_iter().enumerate() {
            let now = (u64::try_from(index).unwrap() + 2) * 1_000_000_000;
            assert!(manager
                .ingest_json(
                    &packet_with_raw(u32::try_from(index + 1).unwrap(), raw),
                    now,
                )
                .is_err());
            manager
                .check_current_fixed_point(350, 500, now)
                .expect("outside-room observation remains calibratable");
        }

        let yaw = manager.yaw_calibration.as_ref().expect("yaw calibration");
        assert_eq!(yaw.point_count, 5);
        assert_eq!(yaw.optimized_yaw_mdeg, 90_000);
        assert!(yaw.applied);
        assert!(yaw.after_rms_error_mm < yaw.before_rms_error_mm);
        assert!(yaw.anchors.iter().all(|anchor| anchor.after_error_mm == 0));

        manager
            .ingest_json(
                &packet_with_raw(6, raw_points[0]),
                7_000_000_000,
            )
            .expect("calibrated transform brings the target into the room");
        assert_eq!(manager.status(7_000_000_000).target_position_mm, Some([1_000, 500]));
    }

    #[test]
    fn yaw_calibration_is_restored_only_for_the_same_sealed_setup() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        let calibration = YawCalibrationStatus {
            schema_version: YAW_CALIBRATION_SCHEMA_VERSION,
            source_kind: "setup".to_string(),
            source_id: "setup-01".to_string(),
            source_sha256: "a".repeat(64),
            created_at_unix_ns: 42,
            base_transform: manager.expected_node.as_ref().unwrap().transform.clone(),
            base_yaw_mdeg: 0,
            previous_effective_yaw_mdeg: 0,
            optimized_yaw_mdeg: 12_500,
            correction_mdeg: 12_500,
            point_count: 5,
            before_rms_error_mm: 900,
            after_rms_error_mm: 120,
            before_median_error_mm: 850,
            after_median_error_mm: 100,
            before_maximum_error_mm: 1_100,
            after_maximum_error_mm: 180,
            improvement_percent: 86.7,
            applied: true,
            persisted: false,
            anchors: Vec::new(),
        };
        manager.yaw_calibration = Some(calibration);
        manager.persist_yaw_calibration(directory.path()).unwrap();

        let mut restored = sealed_manager();
        restored.restore_yaw_calibration(directory.path()).unwrap();
        assert_eq!(
            restored.yaw_calibration.as_ref().map(|value| value.optimized_yaw_mdeg),
            Some(12_500)
        );

        restored.experiment.as_mut().unwrap().setup_sha256 = "b".repeat(64);
        restored.yaw_calibration = None;
        restored.restore_yaw_calibration(directory.path()).unwrap();
        assert!(restored.yaw_calibration.is_none());
    }

    #[test]
    fn rejection_status_exposes_a_fresh_window_separately_from_lifetime_counts() {
        let mut manager = manager();
        let mut outside: serde_json::Value =
            serde_json::from_slice(&packet(0, 1, MeasurementMode::Calibration)).unwrap();
        outside["targets"][0]["room_x_mm"] = serde_json::json!(5_000);
        let outside = serde_json::to_vec(&outside).unwrap();
        assert!(manager.ingest_json(&outside, 1_000_000_000).is_err());
        assert!(manager.ingest_json(b"{", 1_100_000_000).is_err());

        let status = manager.status(1_100_000_000);
        assert_eq!(status.packets_rejected, 2);
        assert_eq!(status.packets_rejected_window, 2);
        assert_eq!(status.reject_reasons_window.get("room_bounds"), Some(&1));
        assert_eq!(status.reject_reasons_window.get("invalid_json"), Some(&1));
        let expired = manager.status(27_000_000_001);
        assert_eq!(expired.packets_rejected, 2);
        assert_eq!(expired.packets_rejected_window, 0);
        assert!(expired.reject_reasons_window.is_empty());
    }

    #[test]
    fn cad_profile_orientation_overrides_packet_transform_for_preview() {
        let mut manager = MmwaveManager::new(DEFAULT_UDP_PORT, None, None, None, None);
        let mut profile = cad_profile([1.0, 1.2, 0.5], "orientation-v1");
        profile.document["mmwave"]["yaw_mdeg"] = serde_json::json!(180000);
        profile.document["mmwave"]["raw_x_inverted"] = serde_json::json!(false);
        manager.apply_cad_profile(&profile);
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        let status = manager.status(1_000_000_000);
        assert_eq!(status.target_position_mm, Some([0, 400]));
        assert_eq!(status.transform.unwrap().yaw_mdeg, 180000);

        let mut mirrored = cad_profile([1.0, 1.2, 0.5], "orientation-v2");
        mirrored.document["mmwave"]["yaw_mdeg"] = serde_json::json!(0);
        mirrored.document["mmwave"]["raw_x_inverted"] = serde_json::json!(true);
        manager.apply_cad_profile(&mirrored);
        manager
            .ingest_json(&packet(1, 1, MeasurementMode::Calibration), 1_100_000_000)
            .unwrap();
        let status = manager.status(1_100_000_000);
        assert_eq!(status.target_position_mm, Some([2000, 400]));
        let transform = status.transform.unwrap();
        assert_eq!(transform.yaw_mdeg, 0);
        assert!(transform.raw_x_inverted);
    }

    #[test]
    fn cad_profile_cannot_rewrite_a_sealed_experiment() {
        let mut manager = sealed_manager();
        let before = manager.room_dimensions_mm;
        manager.apply_cad_profile(&cad_profile([3.9, 0.0, 3.3], "v5"));
        assert!(manager.cad_profile.is_none());
        assert!(manager.cad_profile_error.is_some());
        assert_eq!(manager.room_dimensions_mm, before);
    }

    #[test]
    fn cad_preview_still_rejects_outside_room_targets() {
        let mut manager = manager();
        manager.apply_cad_profile(&cad_profile([3.9, 0.0, 3.3], "v5"));
        assert!(manager.ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000).is_err());
        assert_eq!(manager.status(1_000_000_000).last_rejection.unwrap().category, "room_bounds");
    }

    #[test]
    fn node_status_errors_are_classified_without_exposing_credentials() {
        assert_eq!(classify_node_status_error("request timed out"), "timeout");
        assert_eq!(
            classify_node_status_error("invalid mmWave node status: invalid JSON"),
            "invalid_json"
        );
        assert_eq!(
            classify_node_status_error("mmWave node status request returned HTTP 401"),
            "http_error"
        );
        assert_eq!(classify_node_status_error("connection refused"), "unreachable");
    }

    #[test]
    fn reject_categories_keep_transport_and_content_failures_separate() {
        let mut initial_manager = manager();
        let mut out_of_bounds: serde_json::Value =
            serde_json::from_slice(&packet(0, 1, MeasurementMode::Calibration)).unwrap();
        out_of_bounds["targets"][0]["room_x_mm"] = serde_json::json!(5_000);
        let out_of_bounds = serde_json::to_vec(&out_of_bounds).unwrap();

        assert!(initial_manager
            .ingest_json(&out_of_bounds, 1_000_000_000)
            .is_err());
        assert!(initial_manager.ingest_json(b"{", 1_100_000_000).is_err());

        let status = initial_manager.status(1_100_000_000);
        assert_eq!(status.packets_received, 0);
        assert_eq!(status.packets_rejected, 2);
        assert_eq!(status.reject_reasons.get("room_bounds"), Some(&1));
        assert_eq!(status.reject_reasons.get("invalid_json"), Some(&1));
        assert_eq!(status.last_rejection.as_ref().unwrap().category, "invalid_json");
        assert_eq!(status.last_rejection.as_ref().unwrap().position_mm, None);
        let mut room_manager = manager();
        assert!(room_manager
            .ingest_json(&out_of_bounds, 1_000_000_000)
            .is_err());
        let rejection = room_manager
            .status(1_000_000_000)
            .last_rejection
            .unwrap();
        assert_eq!(rejection.raw_position_mm, Some([100, 1_000]));
        assert_eq!(rejection.position_mm, Some([5_000, 1_500]));
        let outside_status = room_manager.status(1_000_000_000);
        assert_eq!(outside_status.mode, Some(MeasurementMode::Calibration));
        assert_eq!(outside_status.targets.len(), 1);
        assert_eq!(outside_status.targets[0].position_mm, [5_000, 1_500]);
        assert!(!outside_status.targets[0].inside_room);
        assert!(status
            .preflight
            .gates
            .iter()
            .find(|gate| gate.id == "radar_stream_fresh")
            .is_some_and(|gate| gate.pass));
    }

    #[test]
    fn valid_status_keeps_raw_and_room_coordinates_separate() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();

        let status = manager.status(1_000_000_000);
        assert_eq!(status.target_raw_position_mm, Some([100, 1_000]));
        assert_eq!(status.target_position_mm, Some([1_001, 1_500]));
    }

    #[test]
    fn sequence_gap_is_exposed_with_expected_and_received_numbers() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 0, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        manager
            .ingest_json(&packet(2, 0, MeasurementMode::Calibration), 1_500_000_000)
            .unwrap();

        let status = manager.status(1_500_000_000);
        let gap = status.last_sequence_gap.expect("sequence gap");
        assert_eq!(gap.expected_sequence, 1);
        assert_eq!(gap.received_sequence, 2);
        assert_eq!(gap.missing_packets, 1);
        assert_eq!(status.packets_lost, 1);
        assert!(!status
            .preflight
            .gates
            .iter()
            .find(|gate| gate.id == "radar_sequence_loss_free")
            .expect("sequence gate")
            .pass);
    }

    #[test]
    fn transport_metrics_expose_queue_pressure_and_processing_delay() {
        let metrics = MmwaveTransportMetrics::default();
        metrics.note_received();
        metrics.note_received();
        metrics.note_dequeued();
        metrics.note_duplicate();
        metrics.note_sequence_discard();
        metrics.note_received();
        metrics.note_processed(1_000_000, 4_500_000, 3, true);
        metrics.note_processed(101_000_000, 105_000_000, 4, true);
        metrics.note_processed(301_000_000, 309_000_000, 8, false);

        let status = metrics.snapshot();
        assert_eq!(status.raw_udp_packets, 3);
        assert_eq!(status.queue_length, 2);
        assert_eq!(status.queue_peak, 2);
        assert_eq!(status.last_receive_to_process_delay_ms, Some(8));
        assert_eq!(status.max_processing_duration_ms, Some(8));
        assert_eq!(status.duplicate_packets, 1);
        assert_eq!(status.sequence_discards, 1);
        assert_eq!(status.window_samples, 3);
        assert_eq!(status.valid_samples, 2);
        assert_eq!(status.valid_rate_hz, Some(10.0));
        assert_eq!(status.inter_arrival_median_ms, Some(100));
        assert_eq!(status.inter_arrival_p95_ms, Some(200));
        assert_eq!(status.receive_to_process_median_ms, Some(4));
        assert_eq!(status.receive_to_process_p95_ms, Some(8));
    }

    #[test]
    fn active_session_manifest_is_marked_interrupted_after_restart() {
        let root = std::env::temp_dir().join(format!(
            "bll-mmwave-manifest-test-{}-{}",
            std::process::id(),
            now_unix_ns()
        ));
        let directory = root.join("mmwave");
        std::fs::create_dir_all(&directory).unwrap();
        let manifest_path = directory.join("session.manifest.json");
        let manifest = SessionManifest {
            schema_version: SESSION_MANIFEST_SCHEMA_VERSION,
            id: "session".to_string(),
            kind: SessionKind::Calibration,
            lifecycle: SessionLifecycle::Active,
            phase: SessionPhase::EmptyCalibration,
            started_at_unix_ns: 10,
            updated_at_unix_ns: 10,
            recording_path: directory.join("session.mmwave.jsonl").display().to_string(),
            csi_recording_path: directory.join("session.raw-csi.v2.jsonl").display().to_string(),
            aligned_samples: 4,
            rejected_samples: 1,
            empty_duration_seconds: Some(65),
            empty_validity: None,
            mmwave_only_validity: None,
            error: None,
        };
        write_session_manifest(&manifest_path, &manifest).unwrap();

        let mut manager = manager();
        manager.restore_session_manifests(&root).unwrap();
        let status = manager.status(20);
        let session = status.session.expect("restored session");
        assert_eq!(session.lifecycle, SessionLifecycle::Interrupted);
        assert_eq!(session.id, "session");

        let persisted: SessionManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(persisted.lifecycle, SessionLifecycle::Interrupted);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn failed_session_start_leaves_no_partial_session_or_recordings() {
        let root = std::env::temp_dir().join(format!(
            "bll-mmwave-start-test-{}-{}",
            std::process::id(),
            now_unix_ns()
        ));
        let mut manager = sealed_manager();

        let error = manager
            .start_session(
                SessionKind::Calibration,
                &root,
                10,
                CalibrationPolicy::default(),
            )
            .unwrap_err();

        assert!(error.contains("preflight"));
        assert!(manager.session.is_none());
        assert!(manager.expected_mode.is_none());
        assert!(!root.exists());
    }

    #[test]
    fn node_diagnostics_tolerates_slow_esp_status_response() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 512];
            while !request.ends_with(b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            std::thread::sleep(std::time::Duration::from_millis(900));
            let body = r#"{"diagnostics":{"uart_bytes_received":1,"radar_frames_valid":2,"udp_packets_sent":3,"udp_send_failures":0}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let diagnostics = get_node_diagnostics(&NodeControl {
            base_url: format!("http://{address}"),
            bearer_token: "unused".to_string(),
        })
        .unwrap();
        server.join().unwrap();

        assert_eq!(diagnostics.udp_packets_sent, 3);
        assert_eq!(diagnostics.udp_send_failures, 0);
    }

    fn sealed_manager() -> MmwaveManager {
        MmwaveManager::new(
            DEFAULT_UDP_PORT,
            Some([4.02, 2.59, 3.44]),
            None,
            Some(ExpectedNode {
                node_id: "radar-01".to_string(),
                mounting_position_m: Some([0.0, 1.2, 1.72]),
                transform: CoordinateFrame {
                    local: "x_right_y_forward_mm".to_string(),
                    room: "x_length_z_width_mm".to_string(),
                    origin_x_mm: 0,
                    origin_z_mm: 0,
                    yaw_mdeg: 0,
                    raw_x_inverted: false,
                },
            }),
            Some(ExperimentContext {
                setup_id: "setup-01".to_string(),
                setup_sha256: "a".repeat(64),
                server_version: "test".to_string(),
                geometry: PositionCaptureGeometry {
                    room_dimensions_m: [4.02, 2.59, 3.44],
                    tx_position_m: [0.0, 1.0, 0.0],
                    rx_positions_m: vec![[0.0, 1.0, 0.0]; 4],
                },
            }),
        )
    }

    fn active_empty_session(root: &Path, started_at_monotonic_ns: u64) -> Session {
        std::fs::create_dir_all(root).expect("create session directory");
        let recording_path = root.join("empty.mmwave.jsonl");
        let csi_recording_path = root.join("empty.raw-csi.v2.jsonl");
        Session {
            id: "mmwave-calibration-test".to_string(),
            kind: SessionKind::Calibration,
            phase: SessionPhase::EmptyCalibration,
            started_at_ns: started_at_monotonic_ns,
            stable_candidate: None,
            must_exit_zone: None,
            recording_path: recording_path.clone(),
            manifest_path: root.join("empty.manifest.json"),
            writer: BufWriter::new(File::create(&recording_path).expect("radar recording")),
            csi_recording_path: csi_recording_path.clone(),
            csi_writer: BufWriter::new(
                File::create(&csi_recording_path).expect("CSI recording"),
            ),
            aligned_samples: 0,
            rejected_samples: 0,
            empty_started_at_ns: Some(started_at_monotonic_ns),
            empty_duration_ns: 65_000_000_000,
            empty_frames: Vec::new(),
            empty_outside_room_targets: 0,
            empty_in_room_targets: 0,
            empty_multi_target_packets: 0,
            empty_invalid_packets: 0,
            empty_sequence_gaps: 0,
            empty_reboots: 0,
            empty_radar_packets: 0,
            empty_last_radar_packet_ns: None,
            empty_max_radar_gap_ns: 0,
            empty_validity: None,
            mmwave_only_validity: None,
            candidate_frames: Vec::new(),
            candidate_positions_mm: Vec::new(),
            candidate_radar: Vec::new(),
            empty_reference: None,
            training_blocks: Vec::new(),
            receiver_grids: None,
            blind_predictions: Vec::new(),
            blind_truth: Vec::new(),
            trajectory: Vec::new(),
            dataset_records: Vec::new(),
            clock_epoch_id: "test-clock".to_string(),
            io_error: None,
            mmwave_only_radar_packets: 0,
            mmwave_only_valid_target_packets: 0,
            mmwave_only_no_target_packets: 0,
            mmwave_only_outside_room_targets: 0,
            mmwave_only_multi_target_packets: 0,
            mmwave_only_invalid_packets: 0,
            mmwave_only_sequence_gaps: 0,
            mmwave_only_reboots: 0,
            mmwave_only_last_radar_packet_ns: None,
            mmwave_only_max_radar_gap_ns: 0,
        }
    }

    #[test]
    fn empty_calibration_keeps_running_for_an_outside_room_target() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        manager.expected_mode = Some(MeasurementMode::Calibration);
        manager.session = Some(active_empty_session(directory.path(), 1_000_000_000));

        let mut outside: serde_json::Value =
            serde_json::from_slice(&packet(0, 1, MeasurementMode::Calibration))
                .expect("synthetic radar packet");
        outside["targets"][0]["room_x_mm"] = serde_json::json!(5_000);
        let outside = serde_json::to_vec(&outside).expect("serialize outside target");

        manager
            .ingest_json(
                &outside,
                HostTimestamp {
                    host_unix_ns: 2_000_000_000,
                    host_monotonic_ns: 2_000_000_000,
                    clock_epoch_id: "test-clock".to_string(),
                },
            )
            .expect("outside-room target is informational during empty calibration");

        let session = manager.session.as_ref().expect("session remains active");
        assert_eq!(session.empty_outside_room_targets, 1);
        assert_eq!(session.empty_started_at_ns, Some(1_000_000_000));
        assert!(session.empty_validity.is_none());
        assert!(manager.reason.contains("ignored"));
    }

    #[test]
    fn mmwave_only_calibration_finishes_without_csi_when_the_room_stays_empty() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        let mut session = active_empty_session(directory.path(), 1_000_000_000);
        session.kind = SessionKind::MmwaveOnly;
        session.phase = SessionPhase::MmwaveOnly;
        session.empty_duration_ns = 1_000_000_000;
        manager.session = Some(session);
        manager.expected_mode = Some(MeasurementMode::Calibration);

        manager
            .ingest_json(
                &packet(0, 0, MeasurementMode::Calibration),
                HostTimestamp {
                    host_unix_ns: 2_000_000_000,
                    host_monotonic_ns: 2_000_000_000,
                    clock_epoch_id: "test-clock".to_string(),
                },
            )
            .expect("empty radar-only packet");

        let status = manager.status(2_000_000_000);
        let session = status.session.expect("radar-only session");
        assert_eq!(session.phase, SessionPhase::Complete);
        let validity = session
            .mmwave_only_validity
            .expect("radar-only validity");
        assert_eq!(validity.verdict, "valid");
        assert_eq!(validity.radar_packets, 1);
        assert_eq!(validity.no_target_packets, 1);
    }

    #[test]
    fn empty_calibration_does_not_reset_for_an_in_room_target() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        manager.expected_mode = Some(MeasurementMode::Calibration);
        manager.session = Some(active_empty_session(directory.path(), 1_000_000_000));

        manager
            .ingest_json(
                &packet(0, 1, MeasurementMode::Calibration),
                HostTimestamp {
                    host_unix_ns: 2_000_000_000,
                    host_monotonic_ns: 2_000_000_000,
                    clock_epoch_id: "test-clock".to_string(),
                },
            )
            .expect("in-room target does not interrupt collection");

        let session = manager.session.as_ref().expect("session remains active");
        assert_eq!(session.empty_in_room_targets, 1);
        assert_eq!(session.empty_started_at_ns, Some(1_000_000_000));
        assert!(session.empty_validity.is_none());
        assert!(manager.reason.contains("continues"));
    }

    #[test]
    fn empty_calibration_ticker_finishes_without_another_radar_packet() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        manager.expected_mode = Some(MeasurementMode::Calibration);
        manager.session = Some(active_empty_session(directory.path(), 1_000_000_000));

        manager
            .tick(HostTimestamp {
                host_unix_ns: 66_000_000_000,
                host_monotonic_ns: 66_000_000_000,
                clock_epoch_id: "test-clock".to_string(),
            })
            .expect("ticker records the completed empty phase");

        let session = manager.session.as_ref().expect("session remains visible");
        assert_eq!(session.phase, SessionPhase::Coverage);
        let validity = session.empty_validity.as_ref().expect("validity report");
        assert_eq!(validity.verdict, "invalid");
        assert!(validity
            .reasons
            .iter()
            .any(|reason| reason.contains("no valid radar transport packet")));
        assert!(validity
            .reasons
            .iter()
            .any(|reason| reason.contains("empty-room CSI reference was rejected")));
    }

    #[test]
    fn empty_calibration_rejects_a_stale_radar_tail_at_the_deadline() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        manager.expected_mode = Some(MeasurementMode::Calibration);
        manager.session = Some(active_empty_session(directory.path(), 1_000_000_000));

        manager
            .ingest_json(
                &packet(0, 0, MeasurementMode::Calibration),
                HostTimestamp {
                    host_unix_ns: 2_000_000_000,
                    host_monotonic_ns: 2_000_000_000,
                    clock_epoch_id: "test-clock".to_string(),
                },
            )
            .expect("initial radar packet");
        manager
            .tick(HostTimestamp {
                host_unix_ns: 66_000_000_000,
                host_monotonic_ns: 66_000_000_000,
                clock_epoch_id: "test-clock".to_string(),
            })
            .expect("ticker records the stale tail");

        let validity = manager
            .session
            .as_ref()
            .and_then(|session| session.empty_validity.as_ref())
            .expect("validity report");
        assert_eq!(validity.verdict, "invalid");
        assert!(validity
            .reasons
            .iter()
            .any(|reason| reason.contains("radar transport gap reached 64000 ms")));
    }

    #[test]
    fn clean_empty_calibration_finishes_valid_after_the_full_duration() {
        let directory = tempfile::tempdir().expect("temporary calibration directory");
        let mut manager = sealed_manager();
        manager.expected_mode = Some(MeasurementMode::Calibration);
        manager.session = Some(active_empty_session(directory.path(), 1_000_000_000));

        for frame in position_test_frames(
            "mmwave-calibration-test-empty",
            1_000_000_000,
            66_000_000_000,
            200_000_000,
            0,
            "empty",
        ) {
            manager.observe_csi(&frame);
        }
        for (sequence, timestamp_ns) in (0_u32..=130)
            .zip((0_u64..=130).map(|step| 1_000_000_000 + step * 500_000_000))
        {
            manager
                .ingest_json(
                    &packet(sequence, 0, MeasurementMode::Calibration),
                    HostTimestamp {
                        host_unix_ns: timestamp_ns,
                        host_monotonic_ns: timestamp_ns,
                        clock_epoch_id: "test-clock".to_string(),
                    },
                )
                .expect("clean radar packet");
        }
        manager
            .tick(HostTimestamp {
                host_unix_ns: 66_000_000_000,
                host_monotonic_ns: 66_000_000_000,
                clock_epoch_id: "test-clock".to_string(),
            })
            .expect("ticker finishes the clean empty phase");

        let session = manager.session.as_ref().expect("session remains visible");
        assert_eq!(session.phase, SessionPhase::Coverage);
        assert!(session.empty_reference.is_some());
        let validity = session.empty_validity.as_ref().expect("validity report");
        assert_eq!(validity.verdict, "valid");
        assert!(validity.reasons.is_empty());
        assert_eq!(validity.radar_packets, 131);
        assert!(validity.csi_frames > 0);
    }

    fn position_test_source_binding() -> SourceBinding {
        SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: SOURCE_BINDING_REQUIRED_FLAGS,
            scheme: TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: "f".repeat(64),
        }
    }

    fn position_test_frame(
        recording_id: &str,
        rx_id: u8,
        timestamp_ns: u64,
        sequence: u32,
        signal_offset: i8,
        label: &str,
    ) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: timestamp_ns,
            host_monotonic_ns: Some(timestamp_ns),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: Some(recording_id.to_string()),
            label: Some(label.to_string()),
            ground_truth: None,
            rx_id,
            antenna_count: 1,
            subcarrier_count: 8,
            center_frequency_mhz: 2_437,
            sequence,
            rssi_dbm: -48 + signal_offset,
            noise_floor_dbm: -92,
            ppdu_type: 0,
            flags: 0,
            mesh_timestamp_us: None,
            source_binding: Some(position_test_source_binding()),
            iq_pairs: (0..8)
                .map(|_| IqPair {
                    i: rx_id as i8 + signal_offset,
                    q: 2,
                })
                .collect(),
        }
    }

    fn position_test_frames(
        recording_id: &str,
        started_at_ns: u64,
        ended_at_ns: u64,
        interval_ns: u64,
        signal_offset: i8,
        label: &str,
    ) -> Vec<RawCsiFrame> {
        let mut frames = Vec::new();
        let mut sequence = 0;
        let mut timestamp_ns = started_at_ns;
        while timestamp_ns < ended_at_ns {
            for rx_id in 1..=4 {
                frames.push(position_test_frame(
                    recording_id,
                    rx_id,
                    timestamp_ns,
                    sequence,
                    signal_offset,
                    label,
                ));
            }
            sequence += 1;
            timestamp_ns += interval_ns;
        }
        frames
    }

    fn blind_test_index(
        context: &ExperimentContext,
        candidate_started_at_ns: u64,
        candidate_ended_at_ns: u64,
    ) -> (MmwavePositionIndexArtifact, Vec<Zone>) {
        let empty_started_at_ns = 1_000_000_000;
        let empty_ended_at_ns = empty_started_at_ns + 65_000_000_000;
        let empty_reference = build_position_empty_reference(
            &PositionCapture {
                recording_id: "synthetic-empty".to_string(),
                setup_id: context.setup_id.clone(),
                setup_sha256: context.setup_sha256.clone(),
                server_version: context.server_version.clone(),
                geometry: context.geometry.clone(),
                started_at_unix_ns: empty_started_at_ns,
                ended_at_unix_ns: empty_ended_at_ns,
                frames: position_test_frames(
                    "synthetic-empty",
                    empty_started_at_ns,
                    empty_ended_at_ns,
                    200_000_000,
                    0,
                    "empty",
                ),
            },
            &context.setup_sha256,
        )
        .expect("build synthetic empty-room reference");

        let candidate_frames = position_test_frames(
            "mmwave-blind-test",
            candidate_started_at_ns + 50_000_000,
            candidate_ended_at_ns,
            100_000_000,
            8,
            "blind",
        );
        let feature_block = extract_position_feature_window(
            &PositionCapture {
                recording_id: "blind-model-source".to_string(),
                setup_id: context.setup_id.clone(),
                setup_sha256: context.setup_sha256.clone(),
                server_version: context.server_version.clone(),
                geometry: context.geometry.clone(),
                started_at_unix_ns: candidate_started_at_ns,
                ended_at_unix_ns: candidate_ended_at_ns,
                frames: candidate_frames,
            },
            &empty_reference,
            candidate_started_at_ns + FEATURE_OFFSET_NS,
        )
        .expect("extract blind model feature block");

        let coordinates_m = [
            [1.001, 0.0, 1.500],
            [0.400, 0.0, 0.400],
            [1.200, 0.0, 0.400],
            [2.000, 0.0, 0.400],
            [2.800, 0.0, 0.400],
            [3.600, 0.0, 0.400],
            [0.400, 0.0, 2.600],
            [2.000, 0.0, 2.600],
            [3.600, 0.0, 2.600],
        ];
        let points: Vec<_> = coordinates_m
            .iter()
            .enumerate()
            .map(|(index, coordinates_m)| FingerprintPosition {
                id: format!("Z{:03}", index + 1),
                coordinates_m: *coordinates_m,
            })
            .collect();
        let samples: Vec<_> = points
            .iter()
            .enumerate()
            .flat_map(|(point_index, point)| {
                let receivers = feature_block.receivers.clone();
                (0..6).map(move |repeat| PositionFingerprintSample {
                    position: point.clone(),
                    rx_features: receivers
                        .iter()
                        .map(|receiver| {
                            receiver
                                .features
                                .iter()
                                .map(|feature| {
                                    feature
                                        + point_index as f64 * 20.0
                                        + repeat as f64 / 1_000.0
                                })
                                .collect()
                        })
                        .collect(),
                })
            })
            .collect();
        let model = PositionFingerprintModel::train(
            &samples,
            PositionFingerprintConfig {
                minimum_samples_per_position: BLOCKS_PER_ZONE as usize,
            },
        )
        .expect("train synthetic WiFi-only blind index");
        let training_blocks: Vec<_> = points
            .iter()
            .enumerate()
            .flat_map(|(point_index, point)| {
                (0_u64..u64::from(BLOCKS_PER_ZONE)).map(move |repeat| {
                    let block_index = point_index as u64 * u64::from(BLOCKS_PER_ZONE) + repeat;
                    TrainingBlockProvenance {
                        zone_id: point.id.clone(),
                        started_at_unix_ns: block_index * 6_000_000_000,
                        ended_at_unix_ns: block_index * 6_000_000_000 + STABLE_BLOCK_NS,
                        csi_signal_sha256: format!("{:064x}", block_index + 1),
                    }
                })
            })
            .collect();
        let receiver_grids = feature_block
            .receivers
            .iter()
            .map(|receiver| receiver.grid)
            .collect();
        let zones = points
            .iter()
            .map(|point| Zone {
                id: point.id.clone(),
                center_mm: [
                    metres_to_mm(point.coordinates_m[0]).expect("finite X"),
                    metres_to_mm(point.coordinates_m[2]).expect("finite Z"),
                ],
                training_blocks: BLOCKS_PER_ZONE,
                blind_visits: 0,
            })
            .collect();
        let artifact = MmwavePositionIndexArtifact::new(
            context.setup_id.clone(),
            context.setup_sha256.clone(),
            context.server_version.clone(),
            context.geometry.clone(),
            "b".repeat(64),
            "c".repeat(64),
            points.len(),
            calibration_dataset::ALIGNMENT_LIMIT_MS,
            receiver_grids,
            points,
            training_blocks,
            empty_reference,
            model,
        )
        .expect("build synthetic blind index");
        (artifact, zones)
    }

    fn radar_packet_at(sequence: u32, room_position_mm: [i32; 2]) -> Vec<u8> {
        let mut value: serde_json::Value =
            serde_json::from_slice(&packet(sequence, 1, MeasurementMode::Reference))
                .expect("synthetic radar packet");
        value["targets"][0]["room_x_mm"] = serde_json::json!(room_position_mm[0]);
        value["targets"][0]["room_z_mm"] = serde_json::json!(room_position_mm[1]);
        serde_json::to_vec(&value).expect("serialize synthetic radar packet")
    }

    #[test]
    fn accepts_single_target_and_retains_zero_target() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        assert_eq!(manager.status(1_000_000_000).state, LinkState::Valid);
        manager
            .ingest_json(&packet(1, 0, MeasurementMode::Calibration), 1_100_000_000)
            .unwrap();
        assert_eq!(manager.status(1_100_000_000).state, LinkState::NoTarget);
    }

    #[test]
    fn multi_target_is_never_valid_label() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 2, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        assert_eq!(manager.status(1_000_000_000).state, LinkState::MultiTarget);
    }

    #[test]
    fn wrong_mode_and_stale_fail_closed() {
        let mut manager = manager();
        manager.expected_mode = Some(MeasurementMode::Reference);
        assert!(manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .is_err());
        manager.expected_mode = None;
        manager
            .ingest_json(&packet(1, 1, MeasurementMode::Calibration), 1_100_000_000)
            .unwrap();
        assert_eq!(manager.status(2_200_000_001).state, LinkState::Stale);
    }

    #[test]
    fn redundant_and_late_duplicate_sequences_are_ignored() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_005_000_000)
            .unwrap();
        manager
            .ingest_json(&packet(1, 1, MeasurementMode::Calibration), 1_100_000_000)
            .unwrap();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_105_000_000)
            .unwrap();

        let status = manager.status(1_105_000_000);
        assert_eq!(status.packets_received, 2);
        assert_eq!(status.packets_rejected, 0);
        assert_eq!(status.packets_lost, 0);
        let sequence_gate = status
            .preflight
            .gates
            .into_iter()
            .find(|gate| gate.id == "radar_sequence_loss_free")
            .expect("sequence gate");
        assert!(sequence_gate.pass);
    }

    #[test]
    fn rejected_frame_does_not_hide_a_fresh_radar_stream() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 0, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        let mut out_of_bounds: serde_json::Value =
            serde_json::from_slice(&packet(1, 1, MeasurementMode::Calibration)).unwrap();
        out_of_bounds["targets"][0]["room_x_mm"] = serde_json::json!(5_000);
        let out_of_bounds = serde_json::to_vec(&out_of_bounds).unwrap();

        assert!(manager
            .ingest_json(&out_of_bounds, 1_500_000_000)
            .is_err());

        let status = manager.status(1_500_000_000);
        assert_eq!(status.state, LinkState::Invalid);
        assert_eq!(status.packets_rejected, 1);
        let radar_gate = status
            .preflight
            .gates
            .into_iter()
            .find(|gate| gate.id == "radar_stream_fresh")
            .expect("radar gate");
        assert!(radar_gate.pass);
    }

    #[test]
    fn sequence_preflight_recovers_after_a_clean_window() {
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .unwrap();
        manager
            .ingest_json(&packet(2, 1, MeasurementMode::Calibration), 2_000_000_000)
            .unwrap();
        manager.reboot_count = 1;
        manager.last_radar_reboot_ns = Some(2_000_000_000);

        let blocked = manager
            .preflight(2_000_000_000)
            .gates
            .into_iter()
            .find(|gate| gate.id == "radar_sequence_loss_free")
            .expect("sequence gate");
        assert!(!blocked.pass);
        assert!(blocked.detail.contains("sequence_fault=true"));
        assert!(blocked.detail.contains("reboot=true"));

        let recovered = manager
            .preflight(28_000_000_001)
            .gates
            .into_iter()
            .find(|gate| gate.id == "radar_sequence_loss_free")
            .expect("sequence gate");
        assert!(recovered.pass);
        assert!(recovered.detail.contains("gaps=1 reboots=1"));
        assert!(recovered.detail.contains("sequence_fault=false"));
        assert!(recovered.detail.contains("reboot=false"));
    }

    #[test]
    fn session_preflight_requires_complete_training_and_frozen_wifi_index() {
        assert!(manager()
            .validate_session_start(SessionKind::Calibration)
            .is_err());
        let mut manager = sealed_manager();
        assert!(manager
            .validate_session_start(SessionKind::Calibration)
            .is_ok());
        assert!(manager
            .validate_session_start(SessionKind::Blind)
            .is_err());
        manager.zones = (0..9)
            .map(|index| Zone {
                id: format!("P{:02}", index + 1),
                center_mm: [index * 800, 0],
                training_blocks: BLOCKS_PER_ZONE,
                blind_visits: 0,
            })
            .collect();
        let error = manager
            .validate_session_start(SessionKind::Blind)
            .expect_err("blind start without the frozen WiFi index must fail closed");
        assert!(error.contains("frozen WiFi position index"));
    }

    #[test]
    fn blind_visit_freezes_global_and_rx_predictions_before_radar_truth() {
        let csi_origin_ns = 100_000_000_000;
        let candidate_started_at_ns = csi_origin_ns + 50_000_000;
        let candidate_ended_at_ns = csi_origin_ns + STABLE_BLOCK_NS + 50_000_000;
        let mut manager = sealed_manager();
        let context = manager
            .experiment
            .clone()
            .expect("sealed experiment context");
        let (artifact, zones) =
            blind_test_index(&context, candidate_started_at_ns, candidate_ended_at_ns);
        let directory = tempfile::tempdir().expect("temporary blind directory");
        let index_path = directory.path().join("frozen-position-index.json");
        let index_sha256 = artifact.write(&index_path).expect("write frozen index");
        manager.blind_index = Some(artifact);
        manager.position_index_sha256 = Some(index_sha256);
        manager.zones = zones;
        assert!(manager.validate_session_start(SessionKind::Blind).is_ok());

        manager.observe_wifi_prediction(&LivePositionState::Position {
            point_id: "Z009".to_string(),
            coordinates_m: [3.6, 0.0, 2.6],
        });
        let recording_path = directory.path().join("blind.mmwave.jsonl");
        let csi_recording_path = directory.path().join("blind.raw-csi.v2.jsonl");
        manager.expected_mode = Some(MeasurementMode::Reference);
        manager.session = Some(Session {
            id: "mmwave-blind-test".to_string(),
            kind: SessionKind::Blind,
            phase: SessionPhase::Blind,
            started_at_ns: csi_origin_ns,
            stable_candidate: None,
            must_exit_zone: None,
            recording_path: recording_path.clone(),
            manifest_path: directory.path().join("blind.manifest.json"),
            writer: BufWriter::new(File::create(&recording_path).expect("radar recording")),
            csi_recording_path: csi_recording_path.clone(),
            csi_writer: BufWriter::new(
                File::create(&csi_recording_path).expect("CSI recording"),
            ),
            aligned_samples: 0,
            rejected_samples: 0,
            empty_started_at_ns: None,
            empty_duration_ns: u64::from(DEFAULT_EMPTY_CALIBRATION_SECONDS) * 1_000_000_000,
            empty_frames: Vec::new(),
            empty_outside_room_targets: 0,
            empty_in_room_targets: 0,
            empty_multi_target_packets: 0,
            empty_invalid_packets: 0,
            empty_sequence_gaps: 0,
            empty_reboots: 0,
            empty_radar_packets: 0,
            empty_last_radar_packet_ns: None,
            empty_max_radar_gap_ns: 0,
            empty_validity: None,
            mmwave_only_validity: None,
            candidate_frames: Vec::new(),
            candidate_positions_mm: Vec::new(),
            candidate_radar: Vec::new(),
            empty_reference: None,
            training_blocks: Vec::new(),
            receiver_grids: None,
            blind_predictions: Vec::new(),
            blind_truth: Vec::new(),
            trajectory: Vec::new(),
            dataset_records: Vec::new(),
            clock_epoch_id: "test-clock".to_string(),
            io_error: None,
            mmwave_only_radar_packets: 0,
            mmwave_only_valid_target_packets: 0,
            mmwave_only_no_target_packets: 0,
            mmwave_only_outside_room_targets: 0,
            mmwave_only_multi_target_packets: 0,
            mmwave_only_invalid_packets: 0,
            mmwave_only_sequence_gaps: 0,
            mmwave_only_reboots: 0,
            mmwave_only_last_radar_packet_ns: None,
            mmwave_only_max_radar_gap_ns: 0,
        });

        let radar_position_mm = [1_001, 1_500];
        for step in 0_u32..=50 {
            let csi_timestamp_ns = csi_origin_ns + u64::from(step) * 100_000_000;
            for rx_id in 1..=4 {
                manager.observe_csi(&position_test_frame(
                    "mmwave-blind-test",
                    rx_id,
                    csi_timestamp_ns,
                    step,
                    8,
                    "blind",
                ));
            }
            let radar_timestamp_ns = csi_timestamp_ns + 50_000_000;
            manager
                .ingest_json(
                    &radar_packet_at(step, radar_position_mm),
                    HostTimestamp {
                        host_unix_ns: radar_timestamp_ns,
                        host_monotonic_ns: radar_timestamp_ns,
                        clock_epoch_id: "test-clock".to_string(),
                    },
                )
                .expect("aligned blind radar packet");
        }

        let session = manager.session.as_ref().expect("active blind session");
        assert_eq!(session.blind_predictions.len(), 1);
        assert_eq!(session.blind_truth.len(), 1);
        let prediction = &session.blind_predictions[0];
        assert!(matches!(
            &prediction.decision,
            GuidedPredictionDecision::Position { point_id, coordinates_m }
                if point_id == "Z009" && *coordinates_m == [3.6, 0.0, 2.6]
        ));
        assert_eq!(
            prediction
                .receiver_ablation_predictions
                .iter()
                .map(|receiver| (receiver.rx_id, receiver.point_id.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "Z001"), (2, "Z001"), (3, "Z001"), (4, "Z001")]
        );
        let truth = &session.blind_truth[0];
        assert_eq!(truth.expected_point_id, "Z001");
        assert_eq!(truth.radar_coordinates_m, [1.001, 0.0, 1.5]);
        let prediction_bytes =
            deterministic_pretty_json(prediction).expect("serialize frozen prediction");
        assert_eq!(truth.prediction_sha256, sha256_bytes(&prediction_bytes));
        assert_eq!(manager.zones[0].blind_visits, 1);
        assert!(session.candidate_frames.is_empty());
    }

    #[test]
    fn deterministic_zone_selection_requires_real_separation() {
        let mut coverage = BTreeMap::new();
        for z in 0..3 {
            for x in 0..3 {
                coverage.insert((x * 4, z * 4), 5);
            }
        }
        let zones = select_zones(&coverage, 9).unwrap();
        assert_eq!(zones.len(), 9);
        assert_eq!(zones.first().unwrap().id, "Z001");
        assert_eq!(zones.last().unwrap().id, "Z009");
        for pair in zones.iter().enumerate() {
            for other in zones.iter().skip(pair.0 + 1) {
                assert!(
                    squared_distance(pair.1.center_mm, other.center_mm)
                        >= i64::from(MIN_ZONE_SEPARATION_MM).pow(2)
                );
            }
        }
    }

    #[test]
    fn insufficient_coverage_fails_closed() {
        let coverage = (0..8).map(|x| ((x * 4, 0), 5)).collect();
        assert!(select_zones(&coverage, 9).is_err());
    }

    #[test]
    fn zone_policy_accepts_supported_counts_and_rejects_out_of_range_values() {
        for zone_count in [3, 9, 12, 32] {
            assert!(CalibrationPolicy {
                zone_count,
                ..CalibrationPolicy::default()
            }
            .validate()
            .is_ok());
        }
        assert!(CalibrationPolicy {
            zone_count: 2,
            ..CalibrationPolicy::default()
        }
        .validate()
        .is_err());
        assert!(CalibrationPolicy {
            zone_count: 33,
            ..CalibrationPolicy::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn calibration_policy_accepts_configurable_empty_room_duration() {
        assert_eq!(
            CalibrationPolicy::default().empty_calibration_seconds,
            DEFAULT_EMPTY_CALIBRATION_SECONDS
        );
        assert!(CalibrationPolicy {
            empty_calibration_seconds: MIN_EMPTY_CALIBRATION_SECONDS,
            ..CalibrationPolicy::default()
        }
        .validate()
        .is_ok());
        assert!(CalibrationPolicy {
            empty_calibration_seconds: MIN_EMPTY_CALIBRATION_SECONDS - 1,
            ..CalibrationPolicy::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn live_session_start_requires_the_full_observation_preflight() {
        let manager = sealed_manager();
        let error = manager
            .validate_live_session_start(SessionKind::Calibration, 30_000_000_000)
            .unwrap_err();
        assert!(error.contains("preflight"));
        assert!(error.contains("rx1_25s_ready"));
    }

    #[test]
    fn preflight_reports_each_individual_blocker_and_stays_fail_closed() {
        let manager = sealed_manager();
        let preflight = manager.preflight(30_000_000_000);
        let gate_ids: Vec<_> = preflight.gates.iter().map(|gate| gate.id).collect();
        assert_eq!(
            gate_ids,
            vec![
                "node_control_configured",
                "setup_and_transform_sealed",
                "radar_stream_fresh",
                "radar_sequence_loss_free",
                "csi_v2_clock",
                "rx1_25s_ready",
                "rx2_25s_ready",
                "rx3_25s_ready",
                "rx4_25s_ready",
            ]
        );
        assert!(!preflight.ready);
        for gate in &preflight.gates {
            assert!(!gate.detail.is_empty(), "gate {} needs a blocker detail", gate.id);
        }
        assert!(!preflight
            .gates
            .iter()
            .find(|gate| gate.id == "setup_and_transform_sealed")
            .expect("setup gate")
            .pass);
        assert!(!preflight
            .gates
            .iter()
            .find(|gate| gate.id == "radar_stream_fresh")
            .expect("radar gate")
            .pass);
        assert!(preflight
            .gates
            .iter()
            .filter(|gate| gate.id.starts_with("rx"))
            .all(|gate| !gate.pass));

        let mut status = manager.status(30_000_000_000);
        status.attach_node_diagnostics(Ok(NodeDiagnostics {
            uart_bytes_received: 0,
            radar_frames_valid: 0,
            udp_packets_sent: 0,
            udp_send_failures: 1,
        }));
        let diagnostics_gate = status
            .preflight
            .gates
            .iter()
            .find(|gate| gate.id == "node_diagnostics_streaming")
            .expect("node diagnostics gate");
        assert!(!diagnostics_gate.pass);
        assert!(diagnostics_gate.detail.contains("udp_failures=1"));
        assert!(!status.preflight.ready);
    }

    #[test]
    fn preflight_tolerates_small_csi_jitter_without_losing_rx_gate() {
        let mut manager = sealed_manager();
        manager.clock_epoch_id = Some("test-clock".to_string());
        let observations = (0..123)
            .map(|index| CsiPreflightObservation {
                monotonic_ns: index * 200_000_000,
                grid: PositionGridIdentity {
                    center_frequency_mhz: 2437,
                    antenna_count: 1,
                    subcarrier_count: 64,
                    ppdu_type: 0,
                    layout_flags: 0,
                },
                source_bound: true,
                clock_epoch_id: "test-clock".to_string(),
            })
            .collect::<std::collections::VecDeque<_>>();
        manager.rx_preflight = std::array::from_fn(|_| observations.clone());

        let preflight = manager.preflight(25_000_000_000);
        assert!(preflight
            .gates
            .iter()
            .filter(|gate| gate.id.starts_with("rx"))
            .all(|gate| gate.pass));
    }

    #[test]
    fn preflight_names_each_grid_when_a_receiver_is_unstable() {
        let mut manager = sealed_manager();
        manager.clock_epoch_id = Some("test-clock".to_string());
        let mut observations = (0..123)
            .map(|index| CsiPreflightObservation {
                monotonic_ns: index * 200_000_000,
                grid: PositionGridIdentity {
                    center_frequency_mhz: 2437,
                    antenna_count: 1,
                    subcarrier_count: 64,
                    ppdu_type: 0,
                    layout_flags: 0,
                },
                source_bound: true,
                clock_epoch_id: "test-clock".to_string(),
            })
            .collect::<std::collections::VecDeque<_>>();
        observations.back_mut().unwrap().grid.subcarrier_count = 128;
        manager.rx_preflight[3] = observations;

        let gate = manager
            .preflight(25_000_000_000)
            .gates
            .into_iter()
            .find(|gate| gate.id == "rx4_25s_ready")
            .expect("RX4 gate");

        assert!(!gate.pass);
        assert!(gate.detail.contains("stable_grid=false"));
        assert!(gate.detail.contains("2437MHz/1ant/64sc/ppdu0/flags0x00"));
        assert!(gate.detail.contains("2437MHz/1ant/128sc/ppdu0/flags0x00"));
    }

    #[test]
    fn preflight_uses_window_coverage_when_the_latest_frame_is_fresh() {
        let mut manager = sealed_manager();
        manager.clock_epoch_id = Some("test-clock".to_string());
        let observations = (0..120)
            .map(|index| CsiPreflightObservation {
                monotonic_ns: index * 200_000_000,
                grid: PositionGridIdentity {
                    center_frequency_mhz: 2437,
                    antenna_count: 1,
                    subcarrier_count: 64,
                    ppdu_type: 0,
                    layout_flags: 0,
                },
                source_bound: true,
                clock_epoch_id: "test-clock".to_string(),
            })
            .collect::<std::collections::VecDeque<_>>();
        manager.rx_preflight = std::array::from_fn(|_| observations.clone());

        let preflight = manager.preflight(24_200_000_000);
        assert!(preflight
            .gates
            .iter()
            .filter(|gate| gate.id.starts_with("rx"))
            .all(|gate| gate.pass));
    }

    #[test]
    fn blind_truth_uses_the_measured_radar_position_not_the_zone_center() {
        let zone = Zone {
            id: "P01".to_string(),
            center_mm: [1_000, 1_500],
            training_blocks: 6,
            blind_visits: 0,
        };
        let truth = guided_blind_truth(
            &zone,
            "P01-visit-1".to_string(),
            [1_240, 1_390],
            b"frozen prediction",
        );
        assert_eq!(truth.expected_point_id, "P01");
        assert_eq!(truth.radar_coordinates_m, [1.24, 0.0, 1.39]);
        assert_ne!(truth.radar_coordinates_m, [1.0, 0.0, 1.5]);
    }

    #[test]
    fn stable_radar_truth_uses_a_median_that_rejects_single_outliers() {
        assert_eq!(
            median_position_mm(&[[1_000, 1_500], [1_010, 1_490], [4_000, 0]]),
            Some([1_010, 1_490])
        );
        assert_eq!(median_position_mm(&[]), None);
    }

    #[test]
    fn generated_candidate_is_not_public_before_a_passing_blind_report() {
        let mut manager = manager();
        assert!(manager.position_publication_allowed());
        manager.position_index_sha256 = Some("a".repeat(64));
        manager.candidate_requires_validation = true;
        assert!(!manager.position_publication_allowed());
        assert!(!manager.status(0).position_live_approved);
        manager.candidate_requires_validation = false;
        assert!(manager.position_publication_allowed());
        assert!(manager.status(0).position_live_approved);
    }

    #[test]
    fn synthetic_server_status_matches_the_ui_full_path_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../ui/tests/fixtures/mmwave-synthetic-pass-status.json"
        ))
        .expect("synthetic UI status fixture");
        let mut manager = manager();
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Reference), 1_000_000_000)
            .expect("synthetic radar packet reaches calibration manager");
        let (zones, predictions, truth) = passing_blind_fixture();
        let report = build_blind_report(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &predictions,
            &truth,
            &[],
            &zones,
        )
        .expect("synthetic blind report");
        manager.zones = zones;
        manager.position_index_sha256 = Some("a".repeat(64));
        manager.blind_report_sha256 = Some("b".repeat(64));
        manager.blind_verdict = Some(report.verdict);
        manager.candidate_requires_validation = false;

        let mut status = manager.status(1_005_000_000);
        status.attach_node_diagnostics(Ok(NodeDiagnostics {
            uart_bytes_received: 5_400,
            radar_frames_valid: 180,
            udp_packets_sent: 180,
            udp_send_failures: 0,
        }));
        let serialized = serde_json::to_value(status).expect("serialize server status");

        for field in [
            "state",
            "zones",
            "position_index_sha256",
            "position_live_approved",
            "blind_report_sha256",
            "blind_verdict",
            "uart_bytes_received",
            "radar_frames_valid",
            "udp_packets_sent",
        ] {
            assert!(
                serialized.get(field).is_some(),
                "server status lacks {field}"
            );
            assert!(fixture.get(field).is_some(), "UI fixture lacks {field}");
        }
        assert_eq!(serialized["blind_verdict"], fixture["blind_verdict"]);
        assert_eq!(serialized["position_live_approved"], true);
    }

    fn passing_blind_fixture() -> (Vec<Zone>, Vec<GuidedBlindPrediction>, Vec<GuidedBlindTruth>) {
        let zones: Vec<Zone> = (0..9)
            .map(|index| Zone {
                id: format!("P{:02}", index + 1),
                center_mm: [index as i32 * 800, 0],
                training_blocks: 6,
                blind_visits: 2,
            })
            .collect();
        let mut predictions = Vec::new();
        let mut truth = Vec::new();
        for zone in &zones {
            for repetition in 1..=2 {
                let visit_id = format!("{}-visit-{repetition}", zone.id);
                let prediction = GuidedBlindPrediction {
                    visit_id: visit_id.clone(),
                    observed_at_unix_ns: repetition,
                    decision: GuidedPredictionDecision::Position {
                        point_id: zone.id.clone(),
                        coordinates_m: [
                            f64::from(zone.center_mm[0]) / 1000.0,
                            0.0,
                            f64::from(zone.center_mm[1]) / 1000.0,
                        ],
                    },
                    receiver_ablation_predictions: (1..=4)
                        .map(|rx_id| GuidedReceiverPrediction {
                            rx_id,
                            point_id: zone.id.clone(),
                            coordinates_m: [
                                f64::from(zone.center_mm[0]) / 1000.0,
                                0.0,
                                f64::from(zone.center_mm[1]) / 1000.0,
                            ],
                        })
                        .collect(),
                };
                let prediction_sha256 = sha256_bytes(
                    &deterministic_pretty_json(&prediction).expect("serialize prediction"),
                );
                truth.push(GuidedBlindTruth {
                    visit_id,
                    expected_point_id: zone.id.clone(),
                    radar_coordinates_m: [
                        f64::from(zone.center_mm[0]) / 1000.0,
                        0.0,
                        f64::from(zone.center_mm[1]) / 1000.0,
                    ],
                    prediction_sha256,
                });
                predictions.push(prediction);
            }
        }
        (zones, predictions, truth)
    }

    #[test]
    fn blind_report_applies_all_predeclared_acceptance_gates() {
        let (zones, predictions, truth) = passing_blind_fixture();
        let report = build_blind_report(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &predictions,
            &truth,
            &[],
            &zones,
        )
        .unwrap();
        assert_eq!(report.verdict, "PASS");
        assert_eq!(report.total, 18);
        assert_eq!(report.correct, 18);
        assert_eq!(report.receiver_ablation_metrics.len(), 4);
        assert!(report.receiver_ablation_metrics.iter().all(|metrics| {
            metrics.total == 18 && metrics.correct == 18 && metrics.nearest_accuracy == Some(1.0)
        }));
        assert!(report.gates.expected_visit_count_met);
        assert!(report.gates.maximum_error_at_most_1_30_m);
    }

    #[test]
    fn blind_truth_cannot_be_changed_after_prediction_freeze() {
        let (zones, predictions, mut truth) = passing_blind_fixture();
        truth[0].prediction_sha256 = "0".repeat(64);
        let result = build_blind_report(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &predictions,
            &truth,
            &[],
            &zones,
        );
        assert!(result.is_err());
    }

    #[test]
    fn blind_report_requires_frozen_rx1_through_rx4_ablations() {
        let (zones, mut predictions, mut truth) = passing_blind_fixture();
        predictions[0].receiver_ablation_predictions.pop();
        truth[0].prediction_sha256 = sha256_bytes(
            &deterministic_pretty_json(&predictions[0]).expect("serialize prediction"),
        );
        let error = build_blind_report(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &predictions,
            &truth,
            &[],
            &zones,
        )
        .expect_err("missing RX4 ablation must fail closed");
        assert!(error.contains("exactly RX1-RX4"));
    }

    #[test]
    fn abstentions_fail_decided_and_accuracy_gates() {
        let (zones, mut predictions, mut truth) = passing_blind_fixture();
        for index in 0..3 {
            predictions[index].decision = GuidedPredictionDecision::Unknown;
            truth[index].prediction_sha256 = sha256_bytes(
                &deterministic_pretty_json(&predictions[index]).expect("serialize prediction"),
            );
        }
        let report = build_blind_report(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &predictions,
            &truth,
            &[],
            &zones,
        )
        .unwrap();
        assert_eq!(report.verdict, "FAIL");
        assert_eq!(report.abstentions, 3);
        assert!(!report.gates.minimum_decided_count_met);
        assert!(!report.gates.abstention_limit_met);
    }
}
