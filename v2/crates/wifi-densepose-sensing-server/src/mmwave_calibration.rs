//! HLK-LD2450 teacher/reference ingestion for the guided D6 calibration flow.
//!
//! This module never contributes radar coordinates to WiFi prediction. During
//! calibration it selects accessible discrete zones and gates stable CSI
//! blocks. During blind evaluation it retains radar observations as separate
//! truth. The separation is explicit in the session mode and API snapshots.

use std::collections::{BTreeMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
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
const MIN_CELL_OBSERVATIONS: u32 = 3;
const EMPTY_CALIBRATION_NS: u64 = 65_000_000_000;
const FEATURE_OFFSET_NS: u64 = 1_000_000_000;
const PREFLIGHT_WINDOW_NS: u64 = 25_000_000_000;
const PREFLIGHT_FRESH_NS: u64 = 1_000_000_000;
const PREFLIGHT_MIN_FRAMES_PER_RX: usize = 125;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationPolicy {
    #[serde(default = "default_zone_count")]
    pub(crate) zone_count: usize,
}

impl Default for CalibrationPolicy {
    fn default() -> Self {
        Self {
            zone_count: DEFAULT_ZONE_COUNT,
        }
    }
}

fn default_zone_count() -> usize {
    DEFAULT_ZONE_COUNT
}

impl CalibrationPolicy {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if !(MIN_ZONE_COUNT..=MAX_ZONE_COUNT).contains(&self.zone_count) {
            return Err(format!("zone_count must be in {MIN_ZONE_COUNT}..={MAX_ZONE_COUNT}"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SessionPhase {
    EmptyCalibration,
    Coverage,
    Training,
    Blind,
    Complete,
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
    writer: BufWriter<File>,
    csi_recording_path: PathBuf,
    csi_writer: BufWriter<File>,
    aligned_samples: u64,
    rejected_samples: u64,
    empty_started_at_ns: Option<u64>,
    empty_frames: Vec<RawCsiFrame>,
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
}

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
    gates: GuidedBlindGates,
    verdict: String,
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
    pub(crate) udp_port: u16,
    pub(crate) state: LinkState,
    pub(crate) reason: String,
    pub(crate) configured: bool,
    pub(crate) setup_sealed: bool,
    pub(crate) node_id: Option<String>,
    pub(crate) mode: Option<MeasurementMode>,
    pub(crate) expected_mode: Option<MeasurementMode>,
    pub(crate) boot_id: Option<u32>,
    pub(crate) sequence: Option<u32>,
    pub(crate) packet_age_ms: Option<u64>,
    pub(crate) target_count: usize,
    pub(crate) target_position_mm: Option<[i32; 2]>,
    pub(crate) packets_received: u64,
    pub(crate) packets_rejected: u64,
    pub(crate) packets_lost: u64,
    pub(crate) reboot_count: u64,
    pub(crate) uart_bytes_received: Option<u64>,
    pub(crate) radar_frames_valid: Option<u64>,
    pub(crate) udp_packets_sent: Option<u64>,
    pub(crate) udp_send_failures: Option<u64>,
    pub(crate) node_status_error: Option<String>,
    pub(crate) transform: Option<CoordinateFrame>,
    pub(crate) coverage_cells: usize,
    pub(crate) zones: Vec<Zone>,
    pub(crate) zone_count: usize,
    pub(crate) recommended_zone_id: Option<String>,
    pub(crate) session: Option<SessionStatus>,
    pub(crate) position_index_sha256: Option<String>,
    pub(crate) position_live_approved: bool,
    pub(crate) blind_report_sha256: Option<String>,
    pub(crate) blind_verdict: Option<String>,
    pub(crate) preflight: PreflightStatus,
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

#[derive(Debug, Clone, Deserialize)]
struct NodeStatusResponse {
    diagnostics: NodeDiagnostics,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeDiagnostics {
    pub(crate) uart_bytes_received: u64,
    pub(crate) radar_frames_valid: u64,
    pub(crate) udp_packets_sent: u64,
    pub(crate) udp_send_failures: u64,
}

impl MmwaveStatus {
    pub(crate) fn attach_node_diagnostics(&mut self, diagnostics: Result<NodeDiagnostics, String>) {
        match diagnostics {
            Ok(diagnostics) => {
                self.uart_bytes_received = Some(diagnostics.uart_bytes_received);
                self.radar_frames_valid = Some(diagnostics.radar_frames_valid);
                self.udp_packets_sent = Some(diagnostics.udp_packets_sent);
                self.udp_send_failures = Some(diagnostics.udp_send_failures);
                self.node_status_error = None;
                self.preflight.gates.push(PreflightGate {
                    id: "node_diagnostics_streaming",
                    pass: diagnostics.uart_bytes_received > 0
                        && diagnostics.radar_frames_valid > 0
                        && diagnostics.udp_packets_sent > 0
                        && diagnostics.udp_send_failures == 0,
                    detail: format!(
                        "uart={} valid={} udp_sent={} udp_failures={}",
                        diagnostics.uart_bytes_received,
                        diagnostics.radar_frames_valid,
                        diagnostics.udp_packets_sent,
                        diagnostics.udp_send_failures
                    ),
                });
            }
            Err(error) => {
                self.node_status_error = Some(error.clone());
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionStatus {
    id: String,
    kind: SessionKind,
    phase: SessionPhase,
    started_at_unix_ns: u64,
    recording_path: String,
    aligned_samples: u64,
    rejected_samples: u64,
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
}

#[derive(Debug)]
pub(crate) struct MmwaveManager {
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
    last_packet_ns: Option<u64>,
    last_transform: Option<CoordinateFrame>,
    target_count: usize,
    target_position_mm: Option<[i32; 2]>,
    packets_received: u64,
    packets_rejected: u64,
    packets_lost: u64,
    reboot_count: u64,
    coverage: BTreeMap<(i32, i32), u32>,
    zones: Vec<Zone>,
    last_csi_ns: [Option<u64>; 4],
    session: Option<Session>,
    recent_observations: VecDeque<(u64, [i32; 2])>,
    pending_index: Option<(PathBuf, String)>,
    position_index_sha256: Option<String>,
    current_wifi_prediction: GuidedPredictionDecision,
    blind_report_sha256: Option<String>,
    blind_verdict: Option<String>,
    candidate_requires_validation: bool,
    zone_count: usize,
    rx_preflight: [VecDeque<CsiPreflightObservation>; 4],
    csi_clock_rejections: u64,
    clock_epoch_id: Option<String>,
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
        Self {
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
            last_packet_ns: None,
            last_transform: None,
            target_count: 0,
            target_position_mm: None,
            packets_received: 0,
            packets_rejected: 0,
            packets_lost: 0,
            reboot_count: 0,
            coverage: BTreeMap::new(),
            zones: Vec::new(),
            last_csi_ns: [None; 4],
            session: None,
            recent_observations: VecDeque::new(),
            pending_index: None,
            position_index_sha256: None,
            current_wifi_prediction: GuidedPredictionDecision::Uncalibrated,
            blind_report_sha256: None,
            blind_verdict: None,
            candidate_requires_validation: false,
            zone_count: DEFAULT_ZONE_COUNT,
            rx_preflight: std::array::from_fn(|_| VecDeque::new()),
            csi_clock_rejections: 0,
            clock_epoch_id: None,
        }
    }

    pub(crate) fn control(&self) -> Option<NodeControl> {
        self.control.clone()
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
        let packet: RadarPacket = serde_json::from_slice(bytes)
            .map_err(|error| self.reject(format!("invalid JSON packet: {error}")))?;
        self.ingest(packet, bytes, host_time)
    }

    fn ingest(
        &mut self,
        packet: RadarPacket,
        original_bytes: &[u8],
        host_time: HostTimestamp,
    ) -> Result<(), String> {
        validate_packet_shape(&packet).map_err(|error| self.reject(error))?;
        self.validate_identity_and_sequence(&packet)
            .map_err(|error| self.reject(error))?;
        if let Some(expected) = self.expected_mode {
            if packet.mode != expected {
                return Err(self.reject(format!(
                    "expected {} mode, received {}",
                    expected.as_str(),
                    packet.mode.as_str()
                )));
            }
        }
        if let Some(transform) = &self.last_transform {
            if transform != &packet.coordinate_frame {
                return Err(
                    self.reject("coordinate transform changed inside the active setup".to_string())
                );
            }
        }

        let present: Vec<&RadarTarget> = packet
            .targets
            .iter()
            .filter(|target| target.present)
            .collect();
        if let Some(target) = present.first() {
            self.validate_room_bounds(target)
                .map_err(|error| self.reject(error))?;
        }

        self.packets_received += 1;
        self.node_id = Some(packet.node_id.clone());
        self.mode = Some(packet.mode);
        self.boot_id = Some(packet.boot_id);
        self.sequence = Some(packet.sequence);
        self.last_packet_ns = Some(host_time.host_monotonic_ns);
        self.clock_epoch_id = Some(host_time.clock_epoch_id.clone());
        self.last_transform = Some(packet.coordinate_frame.clone());
        self.target_count = present.len();

        match present.as_slice() {
            [] => {
                self.target_position_mm = None;
                self.state = LinkState::NoTarget;
                self.reason = "Radar is connected but currently sees no target.".to_string();
                self.reset_stability();
                self.observe_empty_room(&host_time)?;
            }
            [target] => {
                self.target_position_mm = Some([target.room_x_mm, target.room_z_mm]);
                self.state = LinkState::Valid;
                if self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.phase == SessionPhase::EmptyCalibration)
                {
                    self.reason =
                        "Empty-room calibration paused because a target is visible.".to_string();
                    self.reset_empty_calibration();
                } else {
                    self.reason = "One valid radar target is available.".to_string();
                    self.observe_target(target, &host_time)?;
                }
            }
            _ => {
                self.target_position_mm = None;
                self.state = LinkState::MultiTarget;
                self.reason =
                    "Multiple radar targets are visible; no label is emitted.".to_string();
                self.reset_stability();
            }
        }
        self.write_session_packet(original_bytes, &host_time)?;
        let completed_kind = self
            .session
            .as_ref()
            .and_then(|session| (session.phase == SessionPhase::Complete).then_some(session.kind));
        match completed_kind {
            Some(SessionKind::Calibration) if self.position_index_sha256.is_none() => {
                self.finalize_training_index()?;
            }
            Some(SessionKind::Blind) if self.blind_report_sha256.is_none() => {
                self.finalize_blind_evaluation()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_identity_and_sequence(&mut self, packet: &RadarPacket) -> Result<(), String> {
        if let Some(expected) = &self.expected_node {
            if packet.node_id != expected.node_id {
                return Err(format!(
                    "sealed setup requires mmWave node {:?}, received {:?}",
                    expected.node_id, packet.node_id
                ));
            }
            if packet.coordinate_frame != expected.transform {
                return Err("packet transform does not match the sealed setup".to_string());
            }
        }
        if let Some(node_id) = &self.node_id {
            if node_id != &packet.node_id {
                return Err(format!(
                    "node changed from {node_id:?} to {:?}",
                    packet.node_id
                ));
            }
        }
        match (self.boot_id, self.sequence) {
            (Some(boot_id), Some(previous)) if boot_id == packet.boot_id => {
                let expected = previous.wrapping_add(1);
                if packet.sequence == previous {
                    return Err("duplicate packet sequence".to_string());
                }
                if packet.sequence != expected {
                    let gap = packet.sequence.wrapping_sub(expected);
                    if gap > u32::MAX / 2 {
                        return Err("out-of-order packet sequence".to_string());
                    }
                    self.packets_lost += u64::from(gap);
                }
            }
            (Some(_), _) => {
                self.reboot_count += 1;
                self.reset_stability();
            }
            _ => {}
        }
        Ok(())
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

    fn reject(&mut self, reason: String) -> String {
        self.packets_rejected += 1;
        self.state = LinkState::Invalid;
        self.reason = reason.clone();
        self.reset_stability();
        reason
    }

    fn observe_target(&mut self, target: &RadarTarget, host_time: &HostTimestamp) -> Result<(), String> {
        let position = [target.room_x_mm, target.room_z_mm];
        self.recent_observations
            .push_back((host_time.host_monotonic_ns, position));
        while self.recent_observations.len() > MAX_RECENT_OBSERVATIONS {
            self.recent_observations.pop_front();
        }

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

    fn observe_empty_room(&mut self, host_time: &HostTimestamp) -> Result<(), String> {
        let should_finish = if let Some(session) = &mut self.session {
            if session.phase != SessionPhase::EmptyCalibration {
                return Ok(());
            }
            let started = *session
                .empty_started_at_ns
                .get_or_insert(host_time.host_monotonic_ns);
            host_time.host_monotonic_ns.saturating_sub(started) >= EMPTY_CALIBRATION_NS
        } else {
            false
        };
        if should_finish {
            self.finish_empty_calibration(host_time.host_unix_ns)?;
        }
        Ok(())
    }

    fn reset_empty_calibration(&mut self) {
        if let Some(session) = &mut self.session {
            if session.phase == SessionPhase::EmptyCalibration {
                session.empty_started_at_ns = None;
                session.empty_frames.clear();
            }
        }
    }

    fn finish_empty_calibration(&mut self, ended_at_ns: u64) -> Result<(), String> {
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
            .empty_started_at_ns
            .ok_or_else(|| "empty calibration has no start time".to_string())?;
        let started_at_ns = ended_at_ns.saturating_sub(EMPTY_CALIBRATION_NS);
        let capture = PositionCapture {
            recording_id: format!("{}-empty", session.id),
            setup_id: context.setup_id,
            setup_sha256: context.setup_sha256.clone(),
            server_version: context.server_version,
            geometry: context.geometry,
            started_at_unix_ns: started_at_ns,
            ended_at_unix_ns: ended_at_ns,
            frames: std::mem::take(&mut session.empty_frames),
        };
        let reference = build_position_empty_reference(&capture, &context.setup_sha256)
            .map_err(|error| format!("empty-room CSI reference was rejected: {error}"))?;
        session.empty_reference = Some(reference);
        session.phase = SessionPhase::Coverage;
        session.empty_started_at_ns = None;
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
        let zone = &self.zones[zone_index];
        let visit_id = format!("{}-visit-{}", zone.id, zone.blind_visits + 1);
        let prediction = GuidedBlindPrediction {
            visit_id: visit_id.clone(),
            observed_at_unix_ns: observed_at_ns,
            decision: self.current_wifi_prediction.clone(),
        };
        let prediction_bytes = deterministic_pretty_json(&prediction)
            .map_err(|error| format!("could not freeze blind prediction: {error}"))?;
        let truth = guided_blind_truth(zone, visit_id, radar_position_mm, &prediction_bytes);
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
            schema_version: 1,
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
        let id = format!(
            "mmwave-{}-{}",
            match kind {
                SessionKind::Calibration => "calibration",
                SessionKind::Blind => "blind",
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
            .map_err(|error| format!("could not create {}: {error}", csi_recording_path.display()))?;

        self.expected_mode = Some(match kind {
            SessionKind::Calibration => MeasurementMode::Calibration,
            SessionKind::Blind => MeasurementMode::Reference,
        });
        if kind == SessionKind::Calibration {
            self.zone_count = policy.zone_count;
            self.coverage.clear();
            self.zones.clear();
            self.position_index_sha256 = None;
            // Starting a new calibration invalidates any earlier public result
            // immediately. Aborting or failing later must not silently restore
            // the previous deployment model.
            self.candidate_requires_validation = true;
        } else {
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
            },
            started_at_ns: now.host_unix_ns,
            stable_candidate: None,
            must_exit_zone: None,
            recording_path,
            writer: BufWriter::new(file),
            csi_recording_path,
            csi_writer: BufWriter::new(csi_file),
            aligned_samples: 0,
            rejected_samples: 0,
            empty_started_at_ns: None,
            empty_frames: Vec::new(),
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
        if self.experiment.is_none() {
            return Err("a sealed schema-v2 setup with mmWave identity is required".to_string());
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
        Ok(())
    }

    pub(crate) fn validate_live_session_start(
        &self,
        kind: SessionKind,
        now_monotonic_ns: u64,
    ) -> Result<(), String> {
        self.validate_session_start(kind)?;
        let preflight = self.preflight(now_monotonic_ns);
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
        let Some(mut session) = self.session.take() else {
            return Err("no mmWave session is active".to_string());
        };
        session
            .writer
            .flush()
            .map_err(|error| format!("could not flush radar recording: {error}"))?;
        session
            .csi_writer
            .flush()
            .map_err(|error| format!("could not flush CSI recording: {error}"))?;
        if let Some(error) = session.io_error {
            return Err(error);
        }
        self.expected_mode = None;
        Ok(())
    }

    pub(crate) fn status(&self, now_ns: u64) -> MmwaveStatus {
        let age_ns = self.last_packet_ns.map(|seen| now_ns.saturating_sub(seen));
        let stale = age_ns.is_some_and(|age| age > STALE_AFTER_NS);
        MmwaveStatus {
            udp_port: self.udp_port,
            state: if stale { LinkState::Stale } else { self.state },
            reason: if stale {
                "The last mmWave packet is stale.".to_string()
            } else {
                self.reason.clone()
            },
            configured: self.control.is_some(),
            setup_sealed: self.expected_node.is_some() && self.experiment.is_some(),
            node_id: self.node_id.clone(),
            mode: self.mode,
            expected_mode: self.expected_mode,
            boot_id: self.boot_id,
            sequence: self.sequence,
            packet_age_ms: age_ns.map(|age| age / 1_000_000),
            target_count: self.target_count,
            target_position_mm: self.target_position_mm,
            packets_received: self.packets_received,
            packets_rejected: self.packets_rejected,
            packets_lost: self.packets_lost,
            reboot_count: self.reboot_count,
            uart_bytes_received: None,
            radar_frames_valid: None,
            udp_packets_sent: None,
            udp_send_failures: None,
            node_status_error: None,
            transform: self.last_transform.clone(),
            coverage_cells: self.coverage.len(),
            zones: self.zones.clone(),
            zone_count: self.zone_count,
            recommended_zone_id: self.recommended_zone_id(),
            session: self.session.as_ref().map(session_status),
            position_index_sha256: self.position_index_sha256.clone(),
            position_live_approved: self.position_index_sha256.is_some()
                && self.position_publication_allowed(),
            blind_report_sha256: self.blind_report_sha256.clone(),
            blind_verdict: self.blind_verdict.clone(),
            preflight: self.preflight(now_ns),
        }
    }

    fn preflight(&self, now_ns: u64) -> PreflightStatus {
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
                pass: self.last_packet_ns.is_some_and(|seen| {
                    now_ns.saturating_sub(seen) <= PREFLIGHT_FRESH_NS
                        && !matches!(self.state, LinkState::Disconnected | LinkState::Invalid | LinkState::Stale)
                }),
                detail: "a valid radar packet must have arrived within one second".to_string(),
            },
            PreflightGate {
                id: "radar_sequence_loss_free",
                pass: self.packets_lost == 0 && self.reboot_count == 0,
                detail: format!(
                    "radar packet gaps={} reboots={}",
                    self.packets_lost, self.reboot_count
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
            let grid = recent.first().map(|item| item.grid);
            let stable_grid = grid.is_some()
                && recent.iter().all(|item| Some(item.grid) == grid);
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
                    && span >= PREFLIGHT_WINDOW_NS.saturating_sub(PREFLIGHT_FRESH_NS)
                    && stable_grid
                    && source_bound
                    && clock_matches
                    && fresh,
                detail: format!(
                    "RX{} frames={} span_ms={} stable_grid={} source_bound={} clock_matches={} fresh={}",
                    index + 1,
                    recent.len(),
                    span / 1_000_000,
                    stable_grid,
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
        };
        let zones = self.zones.iter().filter(|zone| incomplete(zone));
        let zone = match self.target_position_mm {
            Some(position) => zones.min_by_key(|zone| squared_distance(zone.center_mm, position)),
            None => zones.min_by(|left, right| left.id.cmp(&right.id)),
        }?;
        Some(zone.id.clone())
    }
}

fn session_status(session: &Session) -> SessionStatus {
    let next_instruction = match session.phase {
        SessionPhase::EmptyCalibration => {
            "Leave the room empty until the 65-second reference is complete.".to_string()
        }
        SessionPhase::Coverage => "Walk through every accessible part of the room.".to_string(),
        SessionPhase::Training => {
            "Follow the highlighted zones and pause for five seconds.".to_string()
        }
        SessionPhase::Blind => {
            "Visit every zone twice; WiFi predictions remain frozen.".to_string()
        }
        SessionPhase::Complete => {
            "Collection is complete. Stop the session to seal its recording.".to_string()
        }
    };
    SessionStatus {
        id: session.id.clone(),
        kind: session.kind,
        phase: session.phase,
        started_at_unix_ns: session.started_at_ns,
        recording_path: session.recording_path.display().to_string(),
        aligned_samples: session.aligned_samples,
        rejected_samples: session.rejected_samples,
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
    for (prediction, truth) in predictions.iter().zip(truth) {
        if prediction.visit_id != truth.visit_id {
            return Err("blind prediction/truth visit IDs do not match".to_string());
        }
        let bytes = deterministic_pretty_json(prediction)
            .map_err(|error| format!("could not verify frozen prediction: {error}"))?;
        if sha256_bytes(&bytes) != truth.prediction_sha256 {
            return Err("blind truth does not match its frozen prediction".to_string());
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
        schema_version: 1,
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

pub(crate) fn set_node_mode(control: &NodeControl, mode: MeasurementMode) -> Result<(), String> {
    let url = format!("{}/mode", control.base_url.trim_end_matches('/'));
    let response = ureq::put(&url)
        .set("Authorization", &format!("Bearer {}", control.bearer_token))
        .set("Content-Type", "text/plain")
        .send_string(mode.as_str())
        .map_err(|error| format!("mmWave mode request failed: {error}"))?;
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
        .timeout(std::time::Duration::from_millis(750))
        .call()
        .map_err(|error| format!("mmWave node status request failed: {error}"))?;
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
        .map_err(|error| format!("mmWave transform request failed: {error}"))?;
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

    fn manager() -> MmwaveManager {
        MmwaveManager::new(DEFAULT_UDP_PORT, Some([4.02, 2.59, 3.44]), None, None, None)
    }

    fn sealed_manager() -> MmwaveManager {
        MmwaveManager::new(
            DEFAULT_UDP_PORT,
            Some([4.02, 2.59, 3.44]),
            None,
            Some(ExpectedNode {
                node_id: "radar-01".to_string(),
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
    fn wrong_mode_duplicate_and_stale_fail_closed() {
        let mut manager = manager();
        manager.expected_mode = Some(MeasurementMode::Reference);
        assert!(manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_000_000_000)
            .is_err());
        manager.expected_mode = None;
        manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_100_000_000)
            .unwrap();
        assert!(manager
            .ingest_json(&packet(0, 1, MeasurementMode::Calibration), 1_200_000_000)
            .is_err());
        assert_eq!(manager.status(2_200_000_001).state, LinkState::Stale);
    }

    #[test]
    fn session_preflight_rejects_invalid_state_before_node_mode_changes() {
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
        assert!(manager.validate_session_start(SessionKind::Blind).is_ok());
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
            assert!(CalibrationPolicy { zone_count }.validate().is_ok());
        }
        assert!(CalibrationPolicy { zone_count: 2 }.validate().is_err());
        assert!(CalibrationPolicy { zone_count: 33 }.validate().is_err());
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
