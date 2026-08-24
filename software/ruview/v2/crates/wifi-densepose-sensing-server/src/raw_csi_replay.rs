//! Deterministic offline replay for lossless raw ESP32 CSI recordings.
//!
//! Replay deliberately re-enters the live parser and shared D4/D5/D6 state
//! transition. Every recorded frame is processed in file order. The 1 Hz
//! records below are evaluation snapshots only; they are not down-sampled
//! detector input.

use super::*;

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::time::Instant;

use sha2::{Digest, Sha256};

pub(crate) const REPLAY_REPORT_SCHEMA_VERSION: u16 = 2;
pub(crate) const REPLAY_REPORT_KIND: &str = "ruview.classification-predictions";
const REPLAY_EVALUATION_HZ: u16 = 1;
const REPLAY_WARMUP_SECONDS: u64 = 5;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const EXPECTED_CAPTURE_SCOPE: &str = "validated_udp_csi_all_grids";

/// Complete deterministic result of one empty-room calibration followed by
/// one or more chronological measurement captures.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayReport {
    pub(crate) schema_version: u16,
    pub(crate) kind: String,
    pub(crate) algorithm: String,
    pub(crate) evaluation_hz: u16,
    pub(crate) warmup_seconds: u64,
    pub(crate) geometry: ReplayGeometry,
    pub(crate) calibration: ReplayCalibrationReport,
    pub(crate) measurements: Vec<ReplayMeasurementReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ReplayGeometry {
    room_dimensions_m: [f64; 3],
    tx_position_m: [f64; 3],
    rx_positions_m: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayCaptureInfo {
    pub(crate) recording_id: String,
    pub(crate) label: Option<String>,
    pub(crate) ground_truth: Option<raw_csi_recording::GroundTruth>,
    pub(crate) server_version: String,
    pub(crate) started_at_unix_ns: u64,
    pub(crate) ended_at_unix_ns: u64,
    pub(crate) raw_sha256: String,
    pub(crate) metadata_sha256: String,
    pub(crate) signal_sha256: String,
    pub(crate) setup_id: Option<String>,
    pub(crate) setup_sha256: Option<String>,
    pub(crate) frames_total: u64,
    pub(crate) frames_accepted: u64,
    pub(crate) frames_grid_rejected: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayCalibrationReport {
    pub(crate) capture: ReplayCaptureInfo,
    pub(crate) nodes: Vec<ReplayNodeSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayMeasurementReport {
    pub(crate) capture: ReplayCaptureInfo,
    pub(crate) seconds: Vec<ReplaySecond>,
    pub(crate) summary: ReplayMeasurementSummary,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplaySecond {
    pub(crate) second_index: u64,
    pub(crate) interval_start_unix_ns: u64,
    pub(crate) interval_end_unix_ns: u64,
    pub(crate) warmup: bool,
    pub(crate) frames_total: u64,
    pub(crate) frames_accepted: u64,
    pub(crate) frames_grid_rejected: u64,
    pub(crate) gap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sample_timestamp_unix_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) classification: Option<ClassificationInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) localization: Option<coarse_localization::CoarseLocalizationEstimate>,
    pub(crate) nodes: Vec<ReplayNodeSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayNodeSnapshot {
    node_id: u8,
    motion_level: String,
    raw_motion_score: f64,
    smoothed_motion_score: f64,
    motion_confidence: f64,
    calibration_motion_rejected_frames: u64,
    d5: d5_presence::NodePresenceSnapshot,
    d6: d6_fingerprint::NodeFingerprintSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReplayMeasurementSummary {
    total_seconds: u64,
    evaluated_seconds: u64,
    gap_seconds: u64,
    post_warmup_evaluated_seconds: u64,
    decision_seconds: u64,
    decision_coverage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    occupied_truth: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correct_decision_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    false_positive_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    false_negative_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_accuracy: Option<f64>,
    localization_evaluable_seconds: u64,
    localized_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    unexpected_localized_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    localization_coverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    median_floor_error_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    p95_floor_error_m: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordingMetadata {
    schema_version: u16,
    recording_id: String,
    label: Option<String>,
    ground_truth: Option<raw_csi_recording::GroundTruth>,
    server_version: String,
    started_at_unix_ns: u64,
    ended_at_unix_ns: u64,
    tx_position: Option<[f64; 3]>,
    rx_positions: Vec<[f64; 3]>,
    room_dimensions: Option<[f64; 3]>,
    setup_id: Option<String>,
    setup_sha256: Option<String>,
    capture_scope: String,
    status: String,
    frames_written: u64,
    dropped_frames: u64,
    incomplete: bool,
    writer_error: Option<String>,
}

impl RecordingMetadata {
    fn geometry(&self) -> Result<ReplayGeometry, String> {
        Ok(ReplayGeometry {
            room_dimensions_m: self
                .room_dimensions
                .ok_or_else(|| format!("{} has no room_dimensions", self.recording_id))?,
            tx_position_m: self
                .tx_position
                .ok_or_else(|| format!("{} has no tx_position", self.recording_id))?,
            rx_positions_m: self.rx_positions.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct LoadedFrame {
    line_number: usize,
    frame: raw_csi_recording::RawCsiFrame,
}

#[derive(Debug, Clone)]
struct LoadedCapture {
    metadata: RecordingMetadata,
    geometry: ReplayGeometry,
    frames: Vec<LoadedFrame>,
    raw_sha256: String,
    metadata_sha256: String,
    signal_sha256: String,
}

impl LoadedCapture {
    fn info(&self, frames_accepted: u64, frames_grid_rejected: u64) -> ReplayCaptureInfo {
        ReplayCaptureInfo {
            recording_id: self.metadata.recording_id.clone(),
            label: self.metadata.label.clone(),
            ground_truth: self.metadata.ground_truth.clone(),
            server_version: self.metadata.server_version.clone(),
            started_at_unix_ns: self.metadata.started_at_unix_ns,
            ended_at_unix_ns: self.metadata.ended_at_unix_ns,
            raw_sha256: self.raw_sha256.clone(),
            metadata_sha256: self.metadata_sha256.clone(),
            signal_sha256: self.signal_sha256.clone(),
            setup_id: self.metadata.setup_id.clone(),
            setup_sha256: self.metadata.setup_sha256.clone(),
            frames_total: self.frames.len() as u64,
            frames_accepted,
            frames_grid_rejected,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ReplaySecondAccumulator {
    frames_total: u64,
    frames_accepted: u64,
    frames_grid_rejected: u64,
    last_sample: Option<ReplaySample>,
}

#[derive(Debug, Clone)]
struct ReplaySample {
    timestamp_unix_ns: u64,
    classification: ClassificationInfo,
    localization: coarse_localization::CoarseLocalizationEstimate,
    nodes: Vec<ReplayNodeSnapshot>,
}

/// Replay a completed empty-room calibration and chronological measurement
/// captures through the same parser and D4/D5/D6 logic as live UDP input.
pub(crate) fn run(
    calibration_path: &std::path::Path,
    measurement_paths: &[PathBuf],
) -> Result<ReplayReport, String> {
    if measurement_paths.is_empty() {
        return Err("at least one measurement recording is required".to_string());
    }

    let calibration = load_capture(calibration_path)?;
    validate_empty_room_calibration(&calibration.metadata)?;

    let mut measurements = Vec::with_capacity(measurement_paths.len());
    let mut recording_ids = HashSet::new();
    recording_ids.insert(calibration.metadata.recording_id.clone());
    for path in measurement_paths {
        let capture = load_capture(path)?;
        if !recording_ids.insert(capture.metadata.recording_id.clone()) {
            return Err(format!(
                "recording {} was supplied more than once",
                capture.metadata.recording_id
            ));
        }
        measurements.push(capture);
    }
    validate_capture_sequence(&calibration, &measurements)?;

    let logical_origin = Instant::now();
    let origin_unix_ns = calibration.metadata.started_at_unix_ns;
    let mut node_states = HashMap::<u8, NodeState>::new();
    let mut fusion = d5_presence::PresenceFusionState::default();
    let calibration_report = replay_calibration(
        &calibration,
        origin_unix_ns,
        logical_origin,
        &mut node_states,
        &mut fusion,
    )?;

    let mut measurement_reports = Vec::with_capacity(measurements.len());
    for measurement in &measurements {
        measurement_reports.push(replay_measurement(
            measurement,
            origin_unix_ns,
            logical_origin,
            &calibration.geometry,
            &mut node_states,
            &mut fusion,
        )?);
    }

    Ok(ReplayReport {
        schema_version: REPLAY_REPORT_SCHEMA_VERSION,
        kind: REPLAY_REPORT_KIND.to_string(),
        algorithm: "shared_live_d4_d5_d6_coarse_localization_v1".to_string(),
        evaluation_hz: REPLAY_EVALUATION_HZ,
        warmup_seconds: REPLAY_WARMUP_SECONDS,
        geometry: calibration.geometry.clone(),
        calibration: calibration_report,
        measurements: measurement_reports,
    })
}

fn load_capture(path: &std::path::Path) -> Result<LoadedCapture, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("recording path {} has no UTF-8 filename", path.display()))?;
    let recording_id = file_name
        .strip_suffix(raw_csi_recording::RAW_CSI_FILE_SUFFIX)
        .ok_or_else(|| {
            format!(
                "recording {} must end with {}",
                path.display(),
                raw_csi_recording::RAW_CSI_FILE_SUFFIX
            )
        })?;
    raw_csi_recording::validate_recording_id(recording_id)
        .map_err(|error| format!("invalid recording filename {}: {error}", path.display()))?;
    if !path.is_file() {
        return Err(format!(
            "recording {} is not a regular file",
            path.display()
        ));
    }

    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let metadata_path = parent.join(format!("{recording_id}.raw-csi.v1.meta.json"));
    if !metadata_path.is_file() {
        return Err(format!(
            "recording {} has no sidecar {}",
            path.display(),
            metadata_path.display()
        ));
    }

    let metadata_bytes = std::fs::read(&metadata_path)
        .map_err(|error| format!("could not read {}: {error}", metadata_path.display()))?;
    let metadata: RecordingMetadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("invalid sidecar {}: {error}", metadata_path.display()))?;
    validate_metadata(recording_id, &metadata)?;
    let geometry = metadata.geometry()?;
    validate_geometry(&metadata.recording_id, &geometry)?;
    validate_ground_truth(
        &metadata.recording_id,
        metadata.ground_truth.as_ref(),
        &geometry,
    )?;

    let file = File::open(path)
        .map_err(|error| format!("could not open recording {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut frames = Vec::new();
    let mut previous_timestamp = None;
    let mut line = String::new();
    let mut line_number = 0usize;
    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        line_number += 1;
        let encoded = line.trim_end_matches(['\r', '\n']);
        if encoded.trim().is_empty() {
            return Err(format!("{} line {} is empty", path.display(), line_number));
        }
        let frame = raw_csi_recording::decode_json_line(encoded).map_err(|error| {
            format!(
                "{} line {} is not valid raw CSI: {error}",
                path.display(),
                line_number
            )
        })?;
        validate_frame(
            &metadata,
            &geometry,
            &frame,
            previous_timestamp,
            path,
            line_number,
        )?;
        previous_timestamp = Some(frame.host_timestamp_unix_ns);
        frames.push(LoadedFrame { line_number, frame });
    }

    if frames.is_empty() {
        return Err(format!("recording {} contains no frames", path.display()));
    }
    if metadata.frames_written != frames.len() as u64 {
        return Err(format!(
            "recording {} has {} decoded frames but sidecar frames_written is {}",
            metadata.recording_id,
            frames.len(),
            metadata.frames_written
        ));
    }

    let signal_frames: Vec<_> = frames.iter().map(|loaded| loaded.frame.clone()).collect();
    let signal_sha256 = position_artifact::signal_sha256(&signal_frames)
        .map_err(|error| format!("could not hash {} signal: {error}", path.display()))?;

    Ok(LoadedCapture {
        metadata,
        geometry,
        frames,
        raw_sha256: sha256_file(path)?,
        metadata_sha256: sha256_bytes(&metadata_bytes),
        signal_sha256,
    })
}

fn validate_metadata(recording_id: &str, metadata: &RecordingMetadata) -> Result<(), String> {
    if metadata.schema_version != raw_csi_recording::RAW_CSI_SCHEMA_VERSION {
        return Err(format!(
            "{recording_id} sidecar schema_version {} is unsupported; expected {}",
            metadata.schema_version,
            raw_csi_recording::RAW_CSI_SCHEMA_VERSION
        ));
    }
    if metadata.recording_id != recording_id {
        return Err(format!(
            "{recording_id} sidecar belongs to {}",
            metadata.recording_id
        ));
    }
    if metadata.server_version.trim().is_empty() {
        return Err(format!("{recording_id} has an empty server_version"));
    }
    if metadata.capture_scope != EXPECTED_CAPTURE_SCOPE {
        return Err(format!(
            "{recording_id} capture_scope is {:?}; expected {:?}",
            metadata.capture_scope, EXPECTED_CAPTURE_SCOPE
        ));
    }
    if metadata.status != "completed" {
        return Err(format!(
            "{recording_id} status is {:?}; only completed captures can be replayed",
            metadata.status
        ));
    }
    if metadata.incomplete {
        return Err(format!("{recording_id} is marked incomplete"));
    }
    if metadata.dropped_frames != 0 {
        return Err(format!(
            "{recording_id} dropped {} frame(s)",
            metadata.dropped_frames
        ));
    }
    if let Some(error) = metadata.writer_error.as_deref() {
        return Err(format!("{recording_id} writer_error: {error}"));
    }
    if metadata.frames_written == 0 {
        return Err(format!("{recording_id} sidecar reports zero frames"));
    }
    if metadata.started_at_unix_ns >= metadata.ended_at_unix_ns {
        return Err(format!(
            "{recording_id} has invalid capture bounds: {}..{}",
            metadata.started_at_unix_ns, metadata.ended_at_unix_ns
        ));
    }
    match (&metadata.setup_id, &metadata.setup_sha256) {
        (None, None) => {}
        (Some(setup_id), Some(setup_sha256)) => {
            raw_csi_recording::validate_recording_id(setup_id)
                .map_err(|error| format!("{recording_id} has invalid setup_id: {error}"))?;
            if !is_lowercase_sha256(setup_sha256) {
                return Err(format!(
                    "{recording_id} setup_sha256 must be exactly 64 lowercase hexadecimal characters"
                ));
            }
        }
        _ => {
            return Err(format!(
                "{recording_id} setup_id and setup_sha256 must be supplied together"
            ));
        }
    }
    Ok(())
}

fn validate_geometry(recording_id: &str, geometry: &ReplayGeometry) -> Result<(), String> {
    if geometry
        .room_dimensions_m
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(format!(
            "{recording_id} room_dimensions must contain three finite positive values"
        ));
    }
    if geometry.rx_positions_m.len() < d5_presence::MIN_FRESH_REFERENCES {
        return Err(format!(
            "{recording_id} has geometry for only {} RX nodes; at least {} are required",
            geometry.rx_positions_m.len(),
            d5_presence::MIN_FRESH_REFERENCES
        ));
    }
    validate_position(
        recording_id,
        "tx_position",
        geometry.tx_position_m,
        geometry.room_dimensions_m,
    )?;
    for (index, position) in geometry.rx_positions_m.iter().copied().enumerate() {
        validate_position(
            recording_id,
            &format!("rx_positions[{}]", index),
            position,
            geometry.room_dimensions_m,
        )?;
        if floor_distance(position, geometry.tx_position_m) <= 1e-6 {
            return Err(format!(
                "{recording_id} RX{} shares the TX floor position",
                index + 1
            ));
        }
        for (previous_index, previous) in
            geometry.rx_positions_m[..index].iter().copied().enumerate()
        {
            if floor_distance(position, previous) <= 1e-6 {
                return Err(format!(
                    "{recording_id} RX{} and RX{} share the same floor position",
                    previous_index + 1,
                    index + 1
                ));
            }
        }
    }
    Ok(())
}

fn validate_position(
    recording_id: &str,
    field: &str,
    position: [f64; 3],
    room_dimensions: [f64; 3],
) -> Result<(), String> {
    if position.iter().any(|value| !value.is_finite()) {
        return Err(format!(
            "{recording_id} {field} contains a non-finite value"
        ));
    }
    if position
        .iter()
        .zip(room_dimensions)
        .any(|(coordinate, maximum)| *coordinate < 0.0 || *coordinate > maximum)
    {
        return Err(format!(
            "{recording_id} {field} {:?} lies outside room {:?}",
            position, room_dimensions
        ));
    }
    Ok(())
}

fn validate_ground_truth(
    recording_id: &str,
    ground_truth: Option<&raw_csi_recording::GroundTruth>,
    geometry: &ReplayGeometry,
) -> Result<(), String> {
    let Some(ground_truth) = ground_truth else {
        return Ok(());
    };
    if ground_truth.occupied == Some(false)
        && (ground_truth.person_count.is_some_and(|count| count != 0)
            || ground_truth.position_m.is_some())
    {
        return Err(format!(
            "{recording_id} ground_truth says unoccupied but also contains a person or position"
        ));
    }
    if ground_truth.occupied == Some(true) && ground_truth.person_count == Some(0) {
        return Err(format!(
            "{recording_id} ground_truth says occupied with person_count 0"
        ));
    }
    if let Some(position) = ground_truth.position_m {
        validate_position(
            recording_id,
            "ground_truth.position_m",
            position,
            geometry.room_dimensions_m,
        )?;
    }
    Ok(())
}

fn validate_empty_room_calibration(metadata: &RecordingMetadata) -> Result<(), String> {
    let Some(ground_truth) = metadata.ground_truth.as_ref() else {
        // Guarded real captures deliberately keep truth out of raw files and
        // sidecars. Selecting this file through --replay-calibration declares
        // its role; the later truth manifest binds and verifies that claim.
        return Ok(());
    };
    if ground_truth.occupied != Some(false) {
        return Err(format!(
            "{} calibration ground_truth.occupied must be false",
            metadata.recording_id
        ));
    }
    if ground_truth.person_count.is_some_and(|count| count != 0) {
        return Err(format!(
            "{} calibration ground_truth.person_count must be 0 when supplied",
            metadata.recording_id
        ));
    }
    Ok(())
}

fn validate_frame(
    metadata: &RecordingMetadata,
    geometry: &ReplayGeometry,
    frame: &raw_csi_recording::RawCsiFrame,
    previous_timestamp: Option<u64>,
    path: &std::path::Path,
    line_number: usize,
) -> Result<(), String> {
    let location = format!("{} line {}", path.display(), line_number);
    if frame.host_timestamp_unix_ns < metadata.started_at_unix_ns
        || frame.host_timestamp_unix_ns >= metadata.ended_at_unix_ns
    {
        return Err(format!(
            "{location} timestamp {} is outside capture interval [{}..{})",
            frame.host_timestamp_unix_ns, metadata.started_at_unix_ns, metadata.ended_at_unix_ns
        ));
    }
    if previous_timestamp.is_some_and(|previous| frame.host_timestamp_unix_ns < previous) {
        return Err(format!("{location} timestamp moves backwards"));
    }
    if frame.session_id.as_deref() != Some(metadata.recording_id.as_str()) {
        return Err(format!(
            "{location} session_id {:?} does not match {}",
            frame.session_id, metadata.recording_id
        ));
    }
    if frame.label != metadata.label {
        return Err(format!("{location} label does not match its sidecar"));
    }
    if frame.ground_truth != metadata.ground_truth {
        return Err(format!(
            "{location} ground_truth does not match its sidecar"
        ));
    }
    if frame.rx_id == 0 || usize::from(frame.rx_id) > geometry.rx_positions_m.len() {
        return Err(format!(
            "{location} RX{} has no matching one-based receiver geometry",
            frame.rx_id
        ));
    }
    if frame.center_frequency_mhz == 0 || frame.center_frequency_mhz > u32::from(u16::MAX) {
        return Err(format!(
            "{location} center_frequency_mhz {} cannot enter the live parser",
            frame.center_frequency_mhz
        ));
    }
    Ok(())
}

fn validate_capture_sequence(
    calibration: &LoadedCapture,
    measurements: &[LoadedCapture],
) -> Result<(), String> {
    let mut previous_end = calibration.metadata.ended_at_unix_ns;
    for measurement in measurements {
        if measurement.geometry != calibration.geometry {
            return Err(format!(
                "{} geometry differs from calibration {}",
                measurement.metadata.recording_id, calibration.metadata.recording_id
            ));
        }
        if measurement.metadata.started_at_unix_ns < previous_end {
            return Err(format!(
                "{} begins before the preceding capture ended",
                measurement.metadata.recording_id
            ));
        }
        previous_end = measurement.metadata.ended_at_unix_ns;
    }
    Ok(())
}

fn replay_calibration(
    capture: &LoadedCapture,
    origin_unix_ns: u64,
    logical_origin: Instant,
    node_states: &mut HashMap<u8, NodeState>,
    fusion: &mut d5_presence::PresenceFusionState,
) -> Result<ReplayCalibrationReport, String> {
    let started_at = logical_time(
        origin_unix_ns,
        logical_origin,
        capture.metadata.started_at_unix_ns,
    )?;
    let ended_at = logical_time(
        origin_unix_ns,
        logical_origin,
        capture.metadata.ended_at_unix_ns,
    )?;
    fusion
        .start_calibration(started_at)
        .map_err(str::to_string)?;

    let mut accepted = 0u64;
    let mut rejected = 0u64;
    for recorded in &capture.frames {
        let now = logical_time(
            origin_unix_ns,
            logical_origin,
            recorded.frame.host_timestamp_unix_ns,
        )?;
        if process_frame(
            &capture.metadata.recording_id,
            recorded,
            now,
            d5_presence::CalibrationPhase::Collecting,
            node_states,
        )? {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }

    install_calibration_references(node_states, fusion, started_at, ended_at)?;
    Ok(ReplayCalibrationReport {
        capture: capture.info(accepted, rejected),
        nodes: snapshot_nodes(node_states, ended_at),
    })
}

fn install_calibration_references(
    node_states: &mut HashMap<u8, NodeState>,
    fusion: &mut d5_presence::PresenceFusionState,
    started_at: Instant,
    ended_at: Instant,
) -> Result<(), String> {
    let mut candidates: Vec<(
        u8,
        Result<d5_presence::PresenceReference, String>,
        Result<d6_fingerprint::FingerprintReference, String>,
    )> = node_states
        .iter()
        .map(|(&node_id, node)| {
            let d5_reference = if node.d5_presence.observation_ready(ended_at) {
                node.d5_presence.build_reference(started_at, ended_at)
            } else {
                Err(format!(
                    "accepted D5 input is stale or below {:.1} Hz",
                    d5_presence::MIN_FRAME_RATE_HZ
                ))
            };
            let d6_reference = if node.d6_fingerprint.observation_ready(ended_at) {
                node.d6_fingerprint.build_reference(started_at, ended_at)
            } else {
                Err(format!(
                    "accepted D6 input is stale or below {:.1} Hz",
                    d6_fingerprint::MIN_FRAME_RATE_HZ
                ))
            };
            (node_id, d5_reference, d6_reference)
        })
        .collect();
    candidates.sort_by_key(|(node_id, _, _)| *node_id);

    let ready_count = candidates
        .iter()
        .filter(|(_, _, d6_reference)| d6_reference.is_ok())
        .count();
    if ready_count < d5_presence::MIN_FRESH_REFERENCES {
        let details = candidates
            .iter()
            .map(|(node_id, d5, d6)| {
                format!(
                    "RX{node_id}: D5={}, D6={}",
                    d5.as_ref().err().map(String::as_str).unwrap_or("ready"),
                    d6.as_ref().err().map(String::as_str).unwrap_or("ready")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "calibration produced only {ready_count} usable D6 RX references; need at least {} ({details})",
            d5_presence::MIN_FRESH_REFERENCES
        ));
    }

    for (node_id, d5_reference, d6_reference) in candidates {
        let node = node_states
            .get_mut(&node_id)
            .expect("calibration candidate must refer to an existing node");
        match d5_reference {
            Ok(reference) => node.d5_presence.install_reference(reference),
            Err(_) => node.d5_presence.invalidate_reference(),
        }
        match d6_reference {
            Ok(reference) => node.d6_fingerprint.install_reference(reference),
            Err(_) => node.d6_fingerprint.invalidate_reference(),
        }
    }
    fusion.finish_calibration(ended_at);
    Ok(())
}

fn replay_measurement(
    capture: &LoadedCapture,
    origin_unix_ns: u64,
    logical_origin: Instant,
    geometry: &ReplayGeometry,
    node_states: &mut HashMap<u8, NodeState>,
    fusion: &mut d5_presence::PresenceFusionState,
) -> Result<ReplayMeasurementReport, String> {
    let duration_ns = capture
        .metadata
        .ended_at_unix_ns
        .checked_sub(capture.metadata.started_at_unix_ns)
        .ok_or_else(|| {
            format!(
                "{} capture duration underflowed",
                capture.metadata.recording_id
            )
        })?;
    let second_count = duration_ns
        .checked_sub(1)
        .map(|value| value / NANOS_PER_SECOND + 1)
        .ok_or_else(|| format!("{} has zero duration", capture.metadata.recording_id))?;
    let bucket_count = usize::try_from(second_count).map_err(|_| {
        format!(
            "{} is too long to allocate one-second replay records",
            capture.metadata.recording_id
        )
    })?;
    let mut buckets = vec![ReplaySecondAccumulator::default(); bucket_count];
    let node_positions: Vec<[f32; 3]> = geometry
        .rx_positions_m
        .iter()
        .map(|position| [position[0] as f32, position[1] as f32, position[2] as f32])
        .collect();

    let mut accepted_total = 0u64;
    let mut rejected_total = 0u64;
    for recorded in &capture.frames {
        let bucket_index = second_index(
            capture.metadata.started_at_unix_ns,
            recorded.frame.host_timestamp_unix_ns,
        )?;
        let bucket = buckets
            .get_mut(usize::try_from(bucket_index).map_err(|_| {
                format!(
                    "{} line {} second index does not fit usize",
                    capture.metadata.recording_id, recorded.line_number
                )
            })?)
            .ok_or_else(|| {
                format!(
                    "{} line {} falls outside its 1 Hz report interval",
                    capture.metadata.recording_id, recorded.line_number
                )
            })?;
        bucket.frames_total += 1;

        let now = logical_time(
            origin_unix_ns,
            logical_origin,
            recorded.frame.host_timestamp_unix_ns,
        )?;
        let accepted = process_frame(
            &capture.metadata.recording_id,
            recorded,
            now,
            d5_presence::CalibrationPhase::Ready,
            node_states,
        )?;
        if !accepted {
            bucket.frames_grid_rejected += 1;
            rejected_total += 1;
            continue;
        }

        bucket.frames_accepted += 1;
        accepted_total += 1;
        let classification = aggregate_node_classification(node_states, now, fusion);
        let localization = estimate_live_localization(
            node_states,
            now,
            &classification,
            Some(geometry.tx_position_m),
            Some(geometry.room_dimensions_m),
            &node_positions,
        );
        bucket.last_sample = Some(ReplaySample {
            timestamp_unix_ns: recorded.frame.host_timestamp_unix_ns,
            classification,
            localization,
            nodes: snapshot_nodes(node_states, now),
        });
    }

    let mut seconds = Vec::with_capacity(bucket_count);
    for (index, bucket) in buckets.into_iter().enumerate() {
        let second_index = index as u64;
        let interval_start_unix_ns = capture
            .metadata
            .started_at_unix_ns
            .checked_add(second_index.saturating_mul(NANOS_PER_SECOND))
            .ok_or_else(|| {
                format!(
                    "{} report interval timestamp overflowed",
                    capture.metadata.recording_id
                )
            })?;
        let interval_end_unix_ns = interval_start_unix_ns
            .checked_add(NANOS_PER_SECOND)
            .unwrap_or(u64::MAX)
            .min(capture.metadata.ended_at_unix_ns);
        let gap = bucket.last_sample.is_none();
        let (sample_timestamp_unix_ns, classification, localization, nodes) =
            match bucket.last_sample {
                Some(sample) => (
                    Some(sample.timestamp_unix_ns),
                    Some(sample.classification),
                    Some(sample.localization),
                    sample.nodes,
                ),
                None => (None, None, None, Vec::new()),
            };
        seconds.push(ReplaySecond {
            second_index,
            interval_start_unix_ns,
            interval_end_unix_ns,
            warmup: second_index < REPLAY_WARMUP_SECONDS,
            frames_total: bucket.frames_total,
            frames_accepted: bucket.frames_accepted,
            frames_grid_rejected: bucket.frames_grid_rejected,
            gap,
            sample_timestamp_unix_ns,
            classification,
            localization,
            nodes,
        });
    }

    let summary = summarize_measurement(&seconds, capture.metadata.ground_truth.as_ref());
    Ok(ReplayMeasurementReport {
        capture: capture.info(accepted_total, rejected_total),
        seconds,
        summary,
    })
}

fn process_frame(
    recording_id: &str,
    recorded: &LoadedFrame,
    now: Instant,
    phase: d5_presence::CalibrationPhase,
    node_states: &mut HashMap<u8, NodeState>,
) -> Result<bool, String> {
    let packet = recorded.frame.to_packet().map_err(|error| {
        format!(
            "{recording_id} line {} could not rebuild its wire packet: {error}",
            recorded.line_number
        )
    })?;
    let frame = parse_esp32_frame(&packet).ok_or_else(|| {
        format!(
            "{recording_id} line {} was rejected by the live ESP32 parser",
            recorded.line_number
        )
    })?;
    if frame.node_id != recorded.frame.rx_id
        || frame.n_antennas != recorded.frame.antenna_count
        || frame.n_subcarriers != recorded.frame.subcarrier_count
        || u32::from(frame.freq_mhz) != recorded.frame.center_frequency_mhz
        || frame.sequence != recorded.frame.sequence
    {
        return Err(format!(
            "{recording_id} line {} changed identity while passing through the live parser",
            recorded.line_number
        ));
    }

    let node = node_states
        .entry(frame.node_id)
        .or_insert_with(NodeState::new);
    if !node.accept_grid(frame.grid()) {
        node.observe_csi_frame_arrival(now);
        return Ok(false);
    }
    let _ = observe_frame_for_presence(node, &frame, now, phase);
    Ok(true)
}

fn snapshot_nodes(node_states: &HashMap<u8, NodeState>, now: Instant) -> Vec<ReplayNodeSnapshot> {
    let mut node_ids: Vec<u8> = node_states.keys().copied().collect();
    node_ids.sort_unstable();
    node_ids
        .into_iter()
        .map(|node_id| {
            let node = node_states
                .get(&node_id)
                .expect("node ID came from the same map");
            ReplayNodeSnapshot {
                node_id,
                motion_level: node.current_motion_level.clone(),
                raw_motion_score: node.latest_raw_motion,
                smoothed_motion_score: node.smoothed_motion,
                motion_confidence: node.motion_confidence,
                calibration_motion_rejected_frames: node.calibration_motion_rejected_frames,
                d5: node.d5_presence.snapshot(now),
                d6: node.d6_fingerprint.snapshot(now),
            }
        })
        .collect()
}

fn summarize_measurement(
    seconds: &[ReplaySecond],
    truth: Option<&raw_csi_recording::GroundTruth>,
) -> ReplayMeasurementSummary {
    let evaluated_seconds = seconds.iter().filter(|second| !second.gap).count() as u64;
    let post_warmup: Vec<&ReplaySecond> = seconds
        .iter()
        .filter(|second| !second.warmup && !second.gap)
        .collect();
    let decided: Vec<&ReplaySecond> = post_warmup
        .iter()
        .copied()
        .filter(|second| {
            second
                .classification
                .as_ref()
                .is_some_and(|classification| {
                    !matches!(
                        classification.motion_level.as_str(),
                        "unknown" | "calibrating"
                    )
                })
        })
        .collect();
    let decision_coverage = ratio(decided.len() as u64, post_warmup.len() as u64);

    let occupied_truth = truth.and_then(|truth| truth.occupied);
    let (correct_decision_seconds, false_positive_seconds, false_negative_seconds, accuracy) =
        if let Some(expected_occupied) = occupied_truth {
            let correct = decided
                .iter()
                .filter(|second| {
                    second
                        .classification
                        .as_ref()
                        .is_some_and(|classification| classification.presence == expected_occupied)
                })
                .count() as u64;
            let false_positive = decided
                .iter()
                .filter(|second| {
                    !expected_occupied
                        && second
                            .classification
                            .as_ref()
                            .is_some_and(|classification| classification.presence)
                })
                .count() as u64;
            let false_negative = decided
                .iter()
                .filter(|second| {
                    expected_occupied
                        && second
                            .classification
                            .as_ref()
                            .is_some_and(|classification| !classification.presence)
                })
                .count() as u64;
            (
                Some(correct),
                Some(false_positive),
                Some(false_negative),
                (!decided.is_empty()).then(|| correct as f64 / decided.len() as f64),
            )
        } else {
            (None, None, None, None)
        };

    let truth_position = truth.and_then(|truth| truth.position_m);
    let localization_evaluable_seconds = if truth_position.is_some() {
        post_warmup.len() as u64
    } else {
        0
    };
    let mut floor_errors = Vec::new();
    if let Some(expected) = truth_position {
        for second in &post_warmup {
            if let Some(position) = second
                .localization
                .as_ref()
                .and_then(|estimate| estimate.position)
            {
                let error = ((position.x - expected[0]).powi(2)
                    + (position.z - expected[2]).powi(2))
                .sqrt();
                if error.is_finite() {
                    floor_errors.push(error);
                }
            }
        }
    }
    floor_errors.sort_by(f64::total_cmp);
    let localized_seconds = floor_errors.len() as u64;
    let unexpected_localized_seconds = (occupied_truth == Some(false)).then(|| {
        post_warmup
            .iter()
            .filter(|second| {
                second
                    .localization
                    .as_ref()
                    .is_some_and(|estimate| estimate.position.is_some())
            })
            .count() as u64
    });
    let localization_coverage = (localization_evaluable_seconds > 0)
        .then(|| ratio(localized_seconds, localization_evaluable_seconds));

    ReplayMeasurementSummary {
        total_seconds: seconds.len() as u64,
        evaluated_seconds,
        gap_seconds: seconds.len() as u64 - evaluated_seconds,
        post_warmup_evaluated_seconds: post_warmup.len() as u64,
        decision_seconds: decided.len() as u64,
        decision_coverage,
        occupied_truth,
        correct_decision_seconds,
        false_positive_seconds,
        false_negative_seconds,
        presence_accuracy: accuracy,
        localization_evaluable_seconds,
        localized_seconds,
        unexpected_localized_seconds,
        localization_coverage,
        median_floor_error_m: percentile(&floor_errors, 0.5),
        p95_floor_error_m: percentile(&floor_errors, 0.95),
    }
}

fn second_index(started_at_unix_ns: u64, timestamp_unix_ns: u64) -> Result<u64, String> {
    timestamp_unix_ns
        .checked_sub(started_at_unix_ns)
        .map(|elapsed| elapsed / NANOS_PER_SECOND)
        .ok_or_else(|| {
            format!(
                "frame timestamp {timestamp_unix_ns} precedes capture start {started_at_unix_ns}"
            )
        })
}

fn logical_time(
    origin_unix_ns: u64,
    logical_origin: Instant,
    timestamp_unix_ns: u64,
) -> Result<Instant, String> {
    let elapsed_ns = timestamp_unix_ns
        .checked_sub(origin_unix_ns)
        .ok_or_else(|| {
            format!("timestamp {timestamp_unix_ns} precedes replay origin {origin_unix_ns}")
        })?;
    logical_origin
        .checked_add(Duration::from_nanos(elapsed_ns))
        .ok_or_else(|| format!("logical Instant overflow for timestamp {timestamp_unix_ns}"))
}

fn floor_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    ((left[0] - right[0]).powi(2) + (left[2] - right[2]).powi(2)).sqrt()
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn percentile(sorted_values: &[f64], quantile: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let rank = (quantile * sorted_values.len() as f64).ceil() as usize;
    Some(sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)])
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("could not hash {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|error| format!("could not hash {}: {error}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn test_geometry() -> ReplayGeometry {
        ReplayGeometry {
            room_dimensions_m: [4.0, 2.5, 3.0],
            tx_position_m: [2.0, 1.0, 1.5],
            rx_positions_m: vec![
                [0.0, 0.5, 0.0],
                [4.0, 0.5, 0.0],
                [0.0, 0.5, 3.0],
                [4.0, 0.5, 3.0],
            ],
        }
    }

    fn test_raw_frame(recording_id: &str) -> raw_csi_recording::RawCsiFrame {
        let mut packet = Vec::new();
        packet.extend_from_slice(&raw_csi_recording::ESP32_CSI_MAGIC.to_le_bytes());
        packet.push(1);
        packet.push(1);
        packet.extend_from_slice(&4u16.to_le_bytes());
        packet.extend_from_slice(&2_437u32.to_le_bytes());
        packet.extend_from_slice(&1u32.to_le_bytes());
        packet.push((-48i8) as u8);
        packet.push((-92i8) as u8);
        packet.push(0);
        packet.push(0);
        packet.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        raw_csi_recording::RawCsiFrame::from_packet(
            &packet,
            raw_csi_recording::RawCsiFrameContext {
                host_timestamp_unix_ns: 1_500_000_000,
                host_monotonic_ns: Some(1_500_000_000),
                clock_epoch_id: Some("test-clock".to_string()),
                session_id: Some(recording_id.to_string()),
                label: Some("still".to_string()),
                ground_truth: Some(raw_csi_recording::GroundTruth {
                    occupied: Some(true),
                    person_count: Some(1),
                    position_m: Some([1.0, 1.0, 1.0]),
                    activity: Some("still".to_string()),
                }),
                mesh_timestamp_us: None,
            },
        )
        .expect("test packet must be valid")
    }

    fn write_capture_fixture(
        directory: &std::path::Path,
        recording_id: &str,
        frames_written: u64,
    ) -> PathBuf {
        let path = directory.join(format!(
            "{recording_id}{}",
            raw_csi_recording::RAW_CSI_FILE_SUFFIX
        ));
        let frame = test_raw_frame(recording_id);
        let mut file = File::create(&path).expect("fixture raw file");
        file.write_all(
            raw_csi_recording::encode_json_line(&frame)
                .expect("fixture frame")
                .as_bytes(),
        )
        .expect("fixture write");
        let geometry = test_geometry();
        let metadata_path = directory.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        std::fs::write(
            metadata_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
                "recording_id": recording_id,
                "label": frame.label,
                "ground_truth": frame.ground_truth,
                "server_version": "test",
                "started_at_unix_ns": 1_000_000_000u64,
                "ended_at_unix_ns": 2_000_000_000u64,
                "tx_position": geometry.tx_position_m,
                "rx_positions": geometry.rx_positions_m,
                "room_dimensions": geometry.room_dimensions_m,
                "capture_scope": EXPECTED_CAPTURE_SCOPE,
                "status": "completed",
                "frames_written": frames_written,
                "dropped_frames": 0,
                "incomplete": false,
                "writer_error": null
            }))
            .expect("fixture metadata"),
        )
        .expect("fixture metadata write");
        path
    }

    fn write_replay_fixture(
        directory: &std::path::Path,
        recording_id: &str,
        label: &str,
        ground_truth: raw_csi_recording::GroundTruth,
        started_at_unix_ns: u64,
        duration_seconds: u64,
    ) -> PathBuf {
        let path = directory.join(format!(
            "{recording_id}{}",
            raw_csi_recording::RAW_CSI_FILE_SUFFIX
        ));
        let mut file = File::create(&path).expect("fixture raw file");
        let samples_per_second = 6u64;
        let sample_count = duration_seconds * samples_per_second;
        let mut frames_written = 0u64;
        for sample_index in 0..sample_count {
            let timestamp =
                started_at_unix_ns + sample_index * NANOS_PER_SECOND / samples_per_second;
            for node_id in 1..=3u8 {
                let mut packet = Vec::new();
                packet.extend_from_slice(&raw_csi_recording::ESP32_CSI_MAGIC.to_le_bytes());
                packet.push(node_id);
                packet.push(1);
                packet.extend_from_slice(&4u16.to_le_bytes());
                packet.extend_from_slice(&2_437u32.to_le_bytes());
                packet.extend_from_slice(&(sample_index as u32).to_le_bytes());
                packet.push((-48i8) as u8);
                packet.push((-92i8) as u8);
                packet.push(0);
                packet.push(0);
                packet.extend_from_slice(&[10, 2, 20, 3, 30, 4, 40, 5]);
                let frame = raw_csi_recording::RawCsiFrame::from_packet(
                    &packet,
                    raw_csi_recording::RawCsiFrameContext {
                        host_timestamp_unix_ns: timestamp,
                        host_monotonic_ns: Some(timestamp),
                        clock_epoch_id: Some("test-clock".to_string()),
                        session_id: Some(recording_id.to_string()),
                        label: Some(label.to_string()),
                        ground_truth: Some(ground_truth.clone()),
                        mesh_timestamp_us: None,
                    },
                )
                .expect("generated raw frame");
                file.write_all(
                    raw_csi_recording::encode_json_line(&frame)
                        .expect("generated frame encoding")
                        .as_bytes(),
                )
                .expect("generated frame write");
                frames_written += 1;
            }
        }

        let geometry = test_geometry();
        let metadata_path = directory.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        std::fs::write(
            metadata_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
                "recording_id": recording_id,
                "label": label,
                "ground_truth": ground_truth,
                "server_version": "test",
                "started_at_unix_ns": started_at_unix_ns,
                "ended_at_unix_ns": started_at_unix_ns
                    + duration_seconds * NANOS_PER_SECOND,
                "tx_position": geometry.tx_position_m,
                "rx_positions": geometry.rx_positions_m,
                "room_dimensions": geometry.room_dimensions_m,
                "capture_scope": EXPECTED_CAPTURE_SCOPE,
                "status": "completed",
                "frames_written": frames_written,
                "dropped_frames": 0,
                "incomplete": false,
                "writer_error": null
            }))
            .expect("generated metadata"),
        )
        .expect("generated metadata write");
        path
    }

    #[test]
    fn strict_integrity_rejects_frame_count_mismatch() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = write_capture_fixture(directory.path(), "count-mismatch", 2);
        let error = load_capture(&path).expect_err("mismatch must fail");
        assert!(error.contains("frames_written"), "{error}");
    }

    #[test]
    fn one_hz_bucket_boundaries_are_exact() {
        let start = 10_000_000_000;
        assert_eq!(second_index(start, start).unwrap(), 0);
        assert_eq!(
            second_index(start, start + NANOS_PER_SECOND - 1).unwrap(),
            0
        );
        assert_eq!(second_index(start, start + NANOS_PER_SECOND).unwrap(), 1);
    }

    #[test]
    fn node_snapshots_are_sorted_by_numeric_id() {
        let now = Instant::now();
        let mut nodes = HashMap::new();
        nodes.insert(4, NodeState::new());
        nodes.insert(1, NodeState::new());
        nodes.insert(3, NodeState::new());
        let ids: Vec<u8> = snapshot_nodes(&nodes, now)
            .into_iter()
            .map(|node| node.node_id)
            .collect();
        assert_eq!(ids, vec![1, 3, 4]);
    }

    #[test]
    fn identical_loaded_input_serializes_identically() {
        let directory = tempfile::tempdir().expect("tempdir");
        let empty_truth = raw_csi_recording::GroundTruth {
            occupied: Some(false),
            person_count: Some(0),
            position_m: None,
            activity: Some("empty".to_string()),
        };
        let calibration_path = write_replay_fixture(
            directory.path(),
            "deterministic-calibration",
            "empty-calibration",
            empty_truth.clone(),
            NANOS_PER_SECOND,
            60,
        );
        let measurement_path = write_replay_fixture(
            directory.path(),
            "deterministic-measurement",
            "empty-measurement",
            empty_truth,
            61 * NANOS_PER_SECOND,
            7,
        );

        let first = run(&calibration_path, std::slice::from_ref(&measurement_path))
            .expect("first deterministic replay");
        let second = run(&calibration_path, std::slice::from_ref(&measurement_path))
            .expect("second deterministic replay");
        assert_eq!(
            serde_json::to_vec(&first).expect("first report JSON"),
            serde_json::to_vec(&second).expect("second report JSON")
        );
    }
}
