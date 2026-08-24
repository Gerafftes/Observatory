//! Deterministic feature extraction for fixed-room position captures.
//!
//! This module does not train or score a position model. It converts complete,
//! lossless raw-CSI captures into overlapping, quality-gated feature blocks.
//! The empty-room projection remains behind [`EmptyProjectionReference`] so the
//! D6 normalization and reference rules keep a single implementation.

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::d6_fingerprint::{self, FingerprintProjectionReference, NodeFingerprintState};
use super::raw_csi_recording::{self, RawCsiFrame, SourceBinding};

const EXPECTED_RX_IDS: [u8; 4] = [1, 2, 3, 4];
const EXPECTED_CAPTURE_SCOPE: &str = "validated_udp_csi_all_grids";
const SETTLING_NS: u64 = 5_000_000_000;
pub(crate) const WINDOW_NS: u64 = 3_000_000_000;
pub(crate) const WINDOW_STEP_NS: u64 = 1_000_000_000;
const MIN_COMMON_COVERAGE_NS: u64 = 2_500_000_000;
const MAX_FRAME_GAP_NS: u64 = 1_000_000_000;
const CALIBRATION_BLOCK_EDGE_TOLERANCE_NS: u64 = 500_000_000;
const MIN_FRAMES_PER_RX: usize = 15;
const MIN_RATE_HZ: u64 = 5;
const FREQUENCY_BANDS: usize = 8;
const POSITION_LAYOUT_FLAGS_MASK: u8 = !0x10;
pub(crate) const POSITION_FEATURE_COUNT: usize = 28;

/// Exact CSI grid identity used by position features.
///
/// The layout-relevant flag bits are bandwidth, STBC, and LDPC. The transient
/// time-sync-valid bit (`0x10`) is deliberately excluded because it does not
/// change subcarrier layout or feature comparability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct PositionGridIdentity {
    pub(crate) center_frequency_mhz: u32,
    pub(crate) antenna_count: u8,
    pub(crate) subcarrier_count: u16,
    pub(crate) ppdu_type: u8,
    pub(crate) layout_flags: u8,
}

impl PositionGridIdentity {
    pub(crate) fn from_frame(frame: &RawCsiFrame) -> Self {
        Self {
            center_frequency_mhz: frame.center_frequency_mhz,
            antenna_count: frame.antenna_count,
            subcarrier_count: frame.subcarrier_count,
            ppdu_type: frame.ppdu_type,
            layout_flags: frame.flags & POSITION_LAYOUT_FLAGS_MASK,
        }
    }
}

/// Output of projecting one raw frame through an explicit empty-room D6
/// reference.
///
/// The future D6 adapter owns gain normalization, stable-bin masking, and
/// subtraction of the empty reference. This module only performs robust
/// temporal and frequency-band aggregation over the resulting residuals.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmptyProjection {
    /// One signed empty-room residual per antenna-major CSI bin.
    pub(crate) signed_bin_residuals: Vec<f64>,
    /// D6's stable-bin decision for every residual. Masked bins must never
    /// enter a position feature as synthetic zero evidence.
    pub(crate) stable_bins: Vec<bool>,
    /// Frame RSSI minus the reference RSSI location estimate.
    pub(crate) rssi_delta_db: f64,
    /// `ln(frame CSI RMS) - ln(reference CSI RMS)`.
    pub(crate) log_csi_rms_delta: f64,
}

/// Narrow boundary between D6's empty-room reference and position extraction.
///
/// Implementations must reject frames for which no exact RX/grid reference
/// exists. Returning already-projected residuals avoids duplicating D6's
/// normalization or stable-bin rules here.
pub(crate) trait EmptyProjectionReference {
    fn project(&self, frame: &RawCsiFrame) -> Result<EmptyProjection, String>;

    fn validate_capture_context(&self, _capture: &PositionCapture) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionEmptyReference {
    pub(crate) schema_version: u16,
    pub(crate) algorithm: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    /// Exact runtime TX identity proven by every calibration frame.
    pub(crate) source_binding: SourceBinding,
    pub(crate) calibration_recording_id: String,
    pub(crate) server_version: String,
    pub(crate) geometry: PositionCaptureGeometry,
    receivers: Vec<RxEmptyReference>,
}

impl PositionEmptyReference {
    /// Validate every private component before a deserialized reference is
    /// trusted by binary search or used to project live/raw CSI.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "empty-reference schema must be 1, got {}",
                self.schema_version
            ));
        }
        if self.algorithm != "d6_empty_projection_v1" {
            return Err(format!(
                "empty-reference algorithm must be {:?}, got {:?}",
                "d6_empty_projection_v1", self.algorithm
            ));
        }
        raw_csi_recording::validate_recording_id(&self.setup_id)
            .map_err(|error| format!("invalid empty-reference setup_id: {error}"))?;
        validate_sha256("empty-reference setup_sha256", &self.setup_sha256)?;
        validate_required_source_binding("empty reference", &self.source_binding)?;
        raw_csi_recording::validate_recording_id(&self.calibration_recording_id)
            .map_err(|error| format!("invalid empty-reference calibration ID: {error}"))?;
        if self.server_version.trim().is_empty() {
            return Err("empty reference has an empty server version".to_string());
        }
        validate_position_geometry(&self.calibration_recording_id, &self.geometry)?;

        let receiver_ids: Vec<u8> = self
            .receivers
            .iter()
            .map(|receiver| receiver.rx_id)
            .collect();
        if receiver_ids != EXPECTED_RX_IDS {
            return Err(format!(
                "empty reference receivers must be exactly {:?} in order, got {:?}",
                EXPECTED_RX_IDS, receiver_ids
            ));
        }
        for receiver in &self.receivers {
            if receiver.grid.center_frequency_mhz == 0
                || receiver.grid.antenna_count == 0
                || receiver.grid.subcarrier_count == 0
                || receiver.grid.layout_flags & !POSITION_LAYOUT_FLAGS_MASK != 0
            {
                return Err(format!(
                    "empty reference RX{} has invalid grid {:?}",
                    receiver.rx_id, receiver.grid
                ));
            }
            let expected_dimensions = usize::from(receiver.grid.antenna_count)
                .checked_mul(usize::from(receiver.grid.subcarrier_count))
                .ok_or_else(|| {
                    format!(
                        "empty reference RX{} CSI dimensions overflow",
                        receiver.rx_id
                    )
                })?;
            receiver
                .d6_projection
                .validate()
                .map_err(|error| format!("empty reference RX{}: {error}", receiver.rx_id))?;
            if receiver.d6_projection.dimensions() != expected_dimensions {
                return Err(format!(
                    "empty reference RX{} grid requires {} CSI bins but D6 has {}",
                    receiver.rx_id,
                    expected_dimensions,
                    receiver.d6_projection.dimensions()
                ));
            }
            validate_stable_band_coverage(&receiver.d6_projection)
                .map_err(|error| format!("empty reference RX{}: {error}", receiver.rx_id))?;
            if !receiver.rssi_median_dbm.is_finite() || !receiver.log_csi_rms_median.is_finite() {
                return Err(format!(
                    "empty reference RX{} contains a non-finite signal baseline",
                    receiver.rx_id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RxEmptyReference {
    rx_id: u8,
    grid: PositionGridIdentity,
    d6_projection: FingerprintProjectionReference,
    rssi_median_dbm: f64,
    log_csi_rms_median: f64,
}

/// Complete capture interval and its validated lossless frames.
#[derive(Debug, Clone)]
pub(crate) struct PositionCapture {
    pub(crate) recording_id: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) server_version: String,
    pub(crate) geometry: PositionCaptureGeometry,
    pub(crate) started_at_unix_ns: u64,
    pub(crate) ended_at_unix_ns: u64,
    pub(crate) frames: Vec<RawCsiFrame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionCaptureGeometry {
    pub(crate) room_dimensions_m: [f64; 3],
    pub(crate) tx_position_m: [f64; 3],
    pub(crate) rx_positions_m: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PositionFeatureExtraction {
    pub(crate) schema_version: u16,
    pub(crate) algorithm: String,
    pub(crate) recording_id: String,
    pub(crate) started_at_unix_ns: u64,
    pub(crate) ended_at_unix_ns: u64,
    pub(crate) settling_ns: u64,
    pub(crate) window_ns: u64,
    pub(crate) window_step_ns: u64,
    pub(crate) feature_count_per_rx: usize,
    pub(crate) accepted_blocks: Vec<PositionFeatureBlock>,
    pub(crate) rejected_windows: Vec<RejectedPositionWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PositionFeatureBlock {
    pub(crate) window_start_unix_ns: u64,
    pub(crate) window_end_unix_ns: u64,
    pub(crate) common_coverage_ns: u64,
    /// Always sorted numerically and exactly `[RX1, RX2, RX3, RX4]`.
    pub(crate) receivers: Vec<RxPositionFeatures>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct RxPositionFeatures {
    pub(crate) rx_id: u8,
    pub(crate) grid: PositionGridIdentity,
    pub(crate) frame_count: usize,
    pub(crate) observed_rate_millihz: u64,
    pub(crate) coverage_ns: u64,
    pub(crate) maximum_gap_ns: u64,
    /// Stable order documented by [`build_feature_vector`].
    pub(crate) features: [f64; POSITION_FEATURE_COUNT],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct RejectedPositionWindow {
    pub(crate) window_start_unix_ns: u64,
    pub(crate) window_end_unix_ns: u64,
    pub(crate) reasons: Vec<PositionWindowRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(crate) enum PositionWindowRejection {
    IncompleteWindow {
        capture_end_unix_ns: u64,
    },
    MissingReceiver {
        rx_id: u8,
    },
    TooFewFrames {
        rx_id: u8,
        actual: usize,
        minimum: usize,
    },
    DuplicateOrNonIncreasingTimestamp {
        rx_id: u8,
        timestamp_unix_ns: u64,
    },
    LowFrameRate {
        rx_id: u8,
        observed_rate_millihz: u64,
        minimum_rate_millihz: u64,
    },
    FrameGap {
        rx_id: u8,
        maximum_gap_ns: u64,
        required_less_than_ns: u64,
    },
    InsufficientReceiverCoverage {
        rx_id: u8,
        actual_ns: u64,
        minimum_ns: u64,
    },
    InsufficientCommonCoverage {
        actual_ns: u64,
        minimum_ns: u64,
    },
    MixedGrid {
        rx_id: u8,
        grids: Vec<PositionGridIdentity>,
    },
    ProjectionFailed {
        rx_id: u8,
        sequence: u32,
        message: String,
    },
    InvalidProjection {
        rx_id: u8,
        sequence: u32,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionCaptureSidecar {
    schema_version: u16,
    recording_id: String,
    setup_id: Option<String>,
    setup_sha256: Option<String>,
    server_version: String,
    started_at_unix_seconds: u64,
    started_at_unix_ns: u64,
    ended_at_unix_seconds: u64,
    ended_at_unix_ns: u64,
    duration_secs: u64,
    tx_position: Option<[f64; 3]>,
    rx_positions: Vec<[f64; 3]>,
    room_dimensions: Option<[f64; 3]>,
    capture_scope: String,
    max_duration_seconds: Option<u64>,
    status: String,
    frames_written: u64,
    rx_summaries: Vec<raw_csi_recording::RawCsiRxSummary>,
    dropped_frames: u64,
    incomplete: bool,
    writer_error: Option<serde_json::Value>,
    label: Option<String>,
    ground_truth: Option<raw_csi_recording::GroundTruth>,
}

#[derive(Debug)]
struct ProjectedFrame {
    signed_bin_residuals: Vec<f64>,
    stable_bins: Vec<bool>,
    rssi_delta_db: f64,
    log_csi_rms_delta: f64,
}

/// Load one complete raw capture locally.
///
/// The replay module's loader is intentionally private today. Keeping this
/// small strict loader local makes the feature extractor independently
/// testable; it can later be replaced with a shared capture loader without
/// changing the public extraction boundary.
pub(crate) fn load_position_capture(path: &Path) -> Result<PositionCapture, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("capture path {} has no UTF-8 filename", path.display()))?;
    let recording_id = file_name
        .strip_suffix(raw_csi_recording::RAW_CSI_FILE_SUFFIX)
        .ok_or_else(|| {
            format!(
                "capture {} must end with {}",
                path.display(),
                raw_csi_recording::RAW_CSI_FILE_SUFFIX
            )
        })?;
    raw_csi_recording::validate_recording_id(recording_id)
        .map_err(|error| format!("invalid capture filename {}: {error}", path.display()))?;
    if !path.is_file() {
        return Err(format!("capture {} is not a regular file", path.display()));
    }

    let metadata_path = sidecar_path(path, recording_id);
    let metadata_bytes = std::fs::read(&metadata_path)
        .map_err(|error| format!("could not read {}: {error}", metadata_path.display()))?;
    let metadata: PositionCaptureSidecar = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("invalid sidecar {}: {error}", metadata_path.display()))?;
    validate_sidecar(recording_id, &metadata)?;

    let file = File::open(path)
        .map_err(|error| format!("could not open capture {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut frames = Vec::new();
    let mut previous_timestamp = None;
    let mut line_number = 0usize;
    let mut encoded = String::new();
    loop {
        encoded.clear();
        let bytes_read = reader
            .read_line(&mut encoded)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        line_number += 1;
        let line = encoded.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Err(format!("{} line {} is empty", path.display(), line_number));
        }
        let frame = raw_csi_recording::decode_json_line(line).map_err(|error| {
            format!(
                "{} line {} is invalid raw CSI: {error}",
                path.display(),
                line_number
            )
        })?;
        validate_loaded_frame(&metadata, &frame, previous_timestamp, path, line_number)?;
        previous_timestamp = Some(frame.host_timestamp_unix_ns);
        frames.push(frame);
    }

    if frames.len() as u64 != metadata.frames_written {
        return Err(format!(
            "{} contains {} frames but sidecar frames_written is {}",
            metadata.recording_id,
            frames.len(),
            metadata.frames_written
        ));
    }
    if frames.is_empty() {
        return Err(format!("{} contains no frames", metadata.recording_id));
    }
    validate_sidecar_rx_summaries(&metadata, &frames)?;

    let capture = PositionCapture {
        recording_id: metadata.recording_id,
        setup_id: metadata
            .setup_id
            .ok_or_else(|| format!("{recording_id} has no setup_id"))?,
        setup_sha256: metadata
            .setup_sha256
            .ok_or_else(|| format!("{recording_id} has no setup_sha256"))?,
        server_version: metadata.server_version,
        geometry: PositionCaptureGeometry {
            room_dimensions_m: metadata
                .room_dimensions
                .ok_or_else(|| format!("{recording_id} has no room_dimensions"))?,
            tx_position_m: metadata
                .tx_position
                .ok_or_else(|| format!("{recording_id} has no tx_position"))?,
            rx_positions_m: metadata.rx_positions,
        },
        started_at_unix_ns: metadata.started_at_unix_ns,
        ended_at_unix_ns: metadata.ended_at_unix_ns,
        frames,
    };
    validate_capture(&capture)
        .map_err(|error| format!("position capture {recording_id} is invalid: {error}"))?;
    Ok(capture)
}

/// Extract deterministic, quality-gated position feature blocks.
pub(crate) fn extract_position_feature_blocks(
    capture: &PositionCapture,
    empty_reference: &dyn EmptyProjectionReference,
) -> Result<PositionFeatureExtraction, String> {
    validate_capture(capture)?;
    empty_reference.validate_capture_context(capture)?;
    let frames = canonical_frames(capture)?;
    let first_window_start = capture
        .started_at_unix_ns
        .checked_add(SETTLING_NS)
        .ok_or_else(|| format!("{} settling timestamp overflowed", capture.recording_id))?;

    let mut accepted_blocks = Vec::new();
    let mut rejected_windows = Vec::new();
    if first_window_start >= capture.ended_at_unix_ns {
        rejected_windows.push(RejectedPositionWindow {
            window_start_unix_ns: first_window_start,
            window_end_unix_ns: first_window_start.saturating_add(WINDOW_NS),
            reasons: vec![PositionWindowRejection::IncompleteWindow {
                capture_end_unix_ns: capture.ended_at_unix_ns,
            }],
        });
    } else {
        let mut window_start = first_window_start;
        while window_start < capture.ended_at_unix_ns {
            let window_end = window_start.checked_add(WINDOW_NS).ok_or_else(|| {
                format!(
                    "{} feature window timestamp overflowed",
                    capture.recording_id
                )
            })?;
            if window_end > capture.ended_at_unix_ns {
                rejected_windows.push(RejectedPositionWindow {
                    window_start_unix_ns: window_start,
                    window_end_unix_ns: window_end,
                    reasons: vec![PositionWindowRejection::IncompleteWindow {
                        capture_end_unix_ns: capture.ended_at_unix_ns,
                    }],
                });
            } else {
                match extract_complete_window(&frames, window_start, window_end, empty_reference) {
                    Ok(block) => accepted_blocks.push(block),
                    Err(reasons) => rejected_windows.push(RejectedPositionWindow {
                        window_start_unix_ns: window_start,
                        window_end_unix_ns: window_end,
                        reasons,
                    }),
                }
            }
            window_start = window_start.checked_add(WINDOW_STEP_NS).ok_or_else(|| {
                format!("{} feature window step overflowed", capture.recording_id)
            })?;
        }
    }

    accepted_blocks.sort_by_key(|block| block.window_start_unix_ns);
    rejected_windows.sort_by_key(|window| window.window_start_unix_ns);
    Ok(PositionFeatureExtraction {
        schema_version: 1,
        algorithm: "d6_empty_projection_8band_robust_v1".to_string(),
        recording_id: capture.recording_id.clone(),
        started_at_unix_ns: capture.started_at_unix_ns,
        ended_at_unix_ns: capture.ended_at_unix_ns,
        settling_ns: SETTLING_NS,
        window_ns: WINDOW_NS,
        window_step_ns: WINDOW_STEP_NS,
        feature_count_per_rx: POSITION_FEATURE_COUNT,
        accepted_blocks,
        rejected_windows,
    })
}

/// Extract one explicit three-second position window with the same validation,
/// D6 projection, timing gates, grid checks, and feature builder used offline.
///
/// The live path calls this boundary instead of maintaining a second feature
/// implementation. The window must be completely contained in `capture`.
pub(crate) fn extract_position_feature_window(
    capture: &PositionCapture,
    empty_reference: &dyn EmptyProjectionReference,
    window_start_unix_ns: u64,
) -> Result<PositionFeatureBlock, String> {
    validate_capture(capture)?;
    empty_reference.validate_capture_context(capture)?;
    let frames = canonical_frames(capture)?;
    let window_end_unix_ns = window_start_unix_ns.checked_add(WINDOW_NS).ok_or_else(|| {
        format!(
            "{} feature window timestamp overflowed",
            capture.recording_id
        )
    })?;
    if window_start_unix_ns < capture.started_at_unix_ns
        || window_end_unix_ns > capture.ended_at_unix_ns
    {
        return Err(format!(
            "{} live feature window [{}..{}) is outside capture bounds [{}..{})",
            capture.recording_id,
            window_start_unix_ns,
            window_end_unix_ns,
            capture.started_at_unix_ns,
            capture.ended_at_unix_ns
        ));
    }
    extract_complete_window(
        &frames,
        window_start_unix_ns,
        window_end_unix_ns,
        empty_reference,
    )
    .map_err(|reasons| {
        format!(
            "{} rejected position window [{}..{}): {reasons:?}",
            capture.recording_id, window_start_unix_ns, window_end_unix_ns
        )
    })
}

/// Build the position extractor's empty-room reference while reusing D6's
/// exact calibration, stable-bin mask, gain normalization, and robust scale.
pub(crate) fn build_position_empty_reference(
    capture: &PositionCapture,
    setup_sha256: &str,
) -> Result<PositionEmptyReference, String> {
    validate_capture(capture)?;
    validate_sha256("setup_sha256", setup_sha256)?;
    if capture.setup_sha256 != setup_sha256 {
        return Err(format!(
            "{} setup SHA does not match the requested experiment setup",
            capture.recording_id
        ));
    }
    let frames = canonical_frames(capture)?;
    let calibration_start_ns = capture
        .started_at_unix_ns
        .checked_add(SETTLING_NS)
        .ok_or_else(|| format!("{} settling timestamp overflowed", capture.recording_id))?;
    let minimum_duration = d6_fingerprint::CALIBRATION_BLOCK
        .checked_mul(d6_fingerprint::MIN_CALIBRATION_BLOCKS as u32)
        .ok_or_else(|| "D6 calibration duration overflowed".to_string())?;
    let minimum_duration_ns = duration_ns(minimum_duration)?;
    let available_ns = capture
        .ended_at_unix_ns
        .checked_sub(calibration_start_ns)
        .ok_or_else(|| format!("{} ends before calibration settling", capture.recording_id))?;
    if available_ns < minimum_duration_ns {
        return Err(format!(
            "{} needs at least {} seconds after settling for an empty D6 reference",
            capture.recording_id,
            minimum_duration.as_secs()
        ));
    }
    let calibration_end_ns = calibration_start_ns
        .checked_add(minimum_duration_ns)
        .ok_or_else(|| format!("{} calibration end overflowed", capture.recording_id))?;

    let logical_start = Instant::now();
    let logical_end = logical_start + minimum_duration;
    let mut receivers = Vec::with_capacity(EXPECTED_RX_IDS.len());
    for rx_id in EXPECTED_RX_IDS {
        let rx_frames: Vec<&RawCsiFrame> = frames
            .iter()
            .copied()
            .filter(|frame| {
                frame.rx_id == rx_id
                    && frame.host_timestamp_unix_ns >= calibration_start_ns
                    && frame.host_timestamp_unix_ns < calibration_end_ns
            })
            .collect();
        if rx_frames.is_empty() {
            return Err(format!(
                "{} has no RX{rx_id} frames after settling",
                capture.recording_id
            ));
        }
        let grids = distinct_grids(&rx_frames);
        if grids.len() != 1 {
            return Err(format!(
                "{} RX{rx_id} uses {} incompatible CSI grids during empty calibration",
                capture.recording_id,
                grids.len()
            ));
        }
        validate_empty_calibration_quality(
            &capture.recording_id,
            rx_id,
            &rx_frames,
            calibration_start_ns,
            calibration_end_ns,
        )?;

        let mut state = NodeFingerprintState::default();
        let mut rssi_values = Vec::with_capacity(rx_frames.len());
        let mut log_rms_values = Vec::with_capacity(rx_frames.len());
        for frame in rx_frames {
            let amplitudes = frame_amplitudes(frame);
            let log_rms = frame_log_csi_rms(&amplitudes).ok_or_else(|| {
                format!(
                    "{} RX{rx_id} sequence {} has zero or invalid CSI RMS",
                    capture.recording_id, frame.sequence
                )
            })?;
            let offset_ns = frame
                .host_timestamp_unix_ns
                .checked_sub(calibration_start_ns)
                .expect("frame was filtered at the calibration start");
            state.observe_calibration(logical_start + Duration::from_nanos(offset_ns), &amplitudes);
            rssi_values.push(frame_rssi_dbm(frame));
            log_rms_values.push(log_rms);
        }

        let reference = state
            .build_reference(logical_start, logical_end)
            .map_err(|error| format!("{} RX{rx_id}: {error}", capture.recording_id))?;
        state.install_reference(reference);
        let d6_projection = state.projection_reference().ok_or_else(|| {
            format!(
                "{} RX{rx_id} D6 projection is missing",
                capture.recording_id
            )
        })?;
        validate_stable_band_coverage(&d6_projection)
            .map_err(|error| format!("{} RX{rx_id}: {error}", capture.recording_id))?;
        receivers.push(RxEmptyReference {
            rx_id,
            grid: grids[0],
            d6_projection,
            rssi_median_dbm: median(&rssi_values),
            log_csi_rms_median: median(&log_rms_values),
        });
    }

    receivers.sort_by_key(|receiver| receiver.rx_id);
    let source_binding = position_source_binding(capture)?;
    let empty_reference = PositionEmptyReference {
        schema_version: 1,
        algorithm: "d6_empty_projection_v1".to_string(),
        setup_id: capture.setup_id.clone(),
        setup_sha256: setup_sha256.to_string(),
        source_binding,
        calibration_recording_id: capture.recording_id.clone(),
        server_version: capture.server_version.clone(),
        geometry: capture.geometry.clone(),
        receivers,
    };
    empty_reference.validate()?;
    Ok(empty_reference)
}

fn validate_empty_calibration_quality(
    recording_id: &str,
    rx_id: u8,
    frames: &[&RawCsiFrame],
    calibration_start_ns: u64,
    calibration_end_ns: u64,
) -> Result<(), String> {
    let block_ns = duration_ns(d6_fingerprint::CALIBRATION_BLOCK)?;
    for block_index in 0..d6_fingerprint::MIN_CALIBRATION_BLOCKS {
        let block_start = calibration_start_ns
            .checked_add(block_index as u64 * block_ns)
            .ok_or_else(|| format!("{recording_id} calibration block start overflowed"))?;
        let block_end = block_start
            .checked_add(block_ns)
            .ok_or_else(|| format!("{recording_id} calibration block end overflowed"))?;
        debug_assert!(block_end <= calibration_end_ns);
        let block_frames: Vec<&RawCsiFrame> = frames
            .iter()
            .copied()
            .filter(|frame| {
                frame.host_timestamp_unix_ns >= block_start
                    && frame.host_timestamp_unix_ns < block_end
            })
            .collect();
        if block_frames.is_empty() {
            return Err(format!(
                "{recording_id} RX{rx_id} calibration block {} has no frames",
                block_index + 1
            ));
        }

        let mut reasons = Vec::new();
        let _ = inspect_receiver_timing(rx_id, &block_frames, &mut reasons);
        let first = block_frames
            .first()
            .expect("calibration block was checked as non-empty")
            .host_timestamp_unix_ns;
        let last = block_frames
            .last()
            .expect("calibration block was checked as non-empty")
            .host_timestamp_unix_ns;
        let starts_on_time =
            first.saturating_sub(block_start) <= CALIBRATION_BLOCK_EDGE_TOLERANCE_NS;
        let ends_on_time = block_end.saturating_sub(last) <= CALIBRATION_BLOCK_EDGE_TOLERANCE_NS;
        if !starts_on_time || !ends_on_time {
            return Err(format!(
                "{recording_id} RX{rx_id} calibration block {} does not span the full block",
                block_index + 1
            ));
        }
        if !reasons.is_empty() {
            return Err(format!(
                "{recording_id} RX{rx_id} calibration block {} failed timing quality: {reasons:?}",
                block_index + 1
            ));
        }
    }
    Ok(())
}

impl EmptyProjectionReference for PositionEmptyReference {
    fn project(&self, frame: &RawCsiFrame) -> Result<EmptyProjection, String> {
        let receiver = self
            .receivers
            .binary_search_by_key(&frame.rx_id, |receiver| receiver.rx_id)
            .ok()
            .map(|index| &self.receivers[index])
            .ok_or_else(|| format!("empty reference has no RX{}", frame.rx_id))?;
        let grid = PositionGridIdentity::from_frame(frame);
        if grid != receiver.grid {
            return Err(format!(
                "RX{} grid {:?} does not match empty reference {:?}",
                frame.rx_id, grid, receiver.grid
            ));
        }

        let amplitudes = frame_amplitudes(frame);
        let projected = receiver
            .d6_projection
            .project(&amplitudes)
            .ok_or_else(|| format!("RX{} cannot enter the D6 projection", frame.rx_id))?;
        let log_csi_rms = frame_log_csi_rms(&amplitudes)
            .ok_or_else(|| format!("RX{} frame has zero or invalid CSI RMS", frame.rx_id))?;
        Ok(EmptyProjection {
            signed_bin_residuals: projected.signed_residuals,
            stable_bins: receiver.d6_projection.stable_bins().to_vec(),
            rssi_delta_db: frame_rssi_dbm(frame) - receiver.rssi_median_dbm,
            log_csi_rms_delta: log_csi_rms - receiver.log_csi_rms_median,
        })
    }

    fn validate_capture_context(&self, capture: &PositionCapture) -> Result<(), String> {
        self.validate()?;
        if capture.setup_id != self.setup_id || capture.setup_sha256 != self.setup_sha256 {
            return Err(format!(
                "{} setup identity differs from empty reference {}",
                capture.recording_id, self.calibration_recording_id
            ));
        }
        let source_binding = position_source_binding(capture)?;
        if source_binding != self.source_binding {
            return Err(format!(
                "{} TX-source binding differs from empty reference {}",
                capture.recording_id, self.calibration_recording_id
            ));
        }
        if capture.server_version != self.server_version {
            return Err(format!(
                "{} server version {:?} differs from empty reference {:?}",
                capture.recording_id, capture.server_version, self.server_version
            ));
        }
        if capture.geometry != self.geometry {
            return Err(format!(
                "{} geometry differs from empty reference {}",
                capture.recording_id, self.calibration_recording_id
            ));
        }
        Ok(())
    }
}

fn extract_complete_window(
    frames: &[&RawCsiFrame],
    window_start: u64,
    window_end: u64,
    empty_reference: &dyn EmptyProjectionReference,
) -> Result<PositionFeatureBlock, Vec<PositionWindowRejection>> {
    let mut reasons = Vec::new();
    let mut ready_receivers = Vec::new();
    let mut receiver_bounds = Vec::new();

    for rx_id in EXPECTED_RX_IDS {
        let rx_frames: Vec<&RawCsiFrame> = frames
            .iter()
            .copied()
            .filter(|frame| {
                frame.rx_id == rx_id
                    && frame.host_timestamp_unix_ns >= window_start
                    && frame.host_timestamp_unix_ns < window_end
            })
            .collect();
        if rx_frames.is_empty() {
            reasons.push(PositionWindowRejection::MissingReceiver { rx_id });
            continue;
        }

        let timing = inspect_receiver_timing(rx_id, &rx_frames, &mut reasons);
        let grids = distinct_grids(&rx_frames);
        if grids.len() != 1 {
            reasons.push(PositionWindowRejection::MixedGrid {
                rx_id,
                grids: grids.clone(),
            });
        }
        let Some((coverage_ns, maximum_gap_ns, observed_rate_millihz)) = timing else {
            continue;
        };
        receiver_bounds.push((
            rx_frames
                .first()
                .expect("non-empty receiver window")
                .host_timestamp_unix_ns,
            rx_frames
                .last()
                .expect("non-empty receiver window")
                .host_timestamp_unix_ns,
        ));

        let reason_count_before_projection = reasons.len();
        let mut projected = Vec::with_capacity(rx_frames.len());
        for frame in &rx_frames {
            match empty_reference.project(frame) {
                Ok(projection) => match validate_projection(frame, projection) {
                    Ok(projection) => projected.push(projection),
                    Err(message) => reasons.push(PositionWindowRejection::InvalidProjection {
                        rx_id,
                        sequence: frame.sequence,
                        message,
                    }),
                },
                Err(message) => reasons.push(PositionWindowRejection::ProjectionFailed {
                    rx_id,
                    sequence: frame.sequence,
                    message,
                }),
            }
        }
        if reasons.len() != reason_count_before_projection {
            continue;
        }
        let grid = grids[0];
        match build_feature_vector(&projected) {
            Ok(features) => ready_receivers.push(RxPositionFeatures {
                rx_id,
                grid,
                frame_count: rx_frames.len(),
                observed_rate_millihz,
                coverage_ns,
                maximum_gap_ns,
                features,
            }),
            Err(message) => reasons.push(PositionWindowRejection::InvalidProjection {
                rx_id,
                sequence: rx_frames[0].sequence,
                message,
            }),
        }
    }

    let common_coverage_ns = if receiver_bounds.len() == EXPECTED_RX_IDS.len() {
        let common_start = receiver_bounds
            .iter()
            .map(|(start, _)| *start)
            .max()
            .expect("four receiver bounds");
        let common_end = receiver_bounds
            .iter()
            .map(|(_, end)| *end)
            .min()
            .expect("four receiver bounds");
        common_end.saturating_sub(common_start)
    } else {
        0
    };
    if common_coverage_ns < MIN_COMMON_COVERAGE_NS {
        reasons.push(PositionWindowRejection::InsufficientCommonCoverage {
            actual_ns: common_coverage_ns,
            minimum_ns: MIN_COMMON_COVERAGE_NS,
        });
    }

    if !reasons.is_empty() {
        return Err(reasons);
    }
    ready_receivers.sort_by_key(|receiver| receiver.rx_id);
    if ready_receivers
        .iter()
        .map(|receiver| receiver.rx_id)
        .ne(EXPECTED_RX_IDS)
    {
        return Err(vec![PositionWindowRejection::InsufficientCommonCoverage {
            actual_ns: common_coverage_ns,
            minimum_ns: MIN_COMMON_COVERAGE_NS,
        }]);
    }
    Ok(PositionFeatureBlock {
        window_start_unix_ns: window_start,
        window_end_unix_ns: window_end,
        common_coverage_ns,
        receivers: ready_receivers,
    })
}

fn inspect_receiver_timing(
    rx_id: u8,
    frames: &[&RawCsiFrame],
    reasons: &mut Vec<PositionWindowRejection>,
) -> Option<(u64, u64, u64)> {
    if frames.len() < MIN_FRAMES_PER_RX {
        reasons.push(PositionWindowRejection::TooFewFrames {
            rx_id,
            actual: frames.len(),
            minimum: MIN_FRAMES_PER_RX,
        });
    }
    let first = frames.first()?.host_timestamp_unix_ns;
    let last = frames.last()?.host_timestamp_unix_ns;
    let coverage_ns = last.saturating_sub(first);
    if coverage_ns < MIN_COMMON_COVERAGE_NS {
        reasons.push(PositionWindowRejection::InsufficientReceiverCoverage {
            rx_id,
            actual_ns: coverage_ns,
            minimum_ns: MIN_COMMON_COVERAGE_NS,
        });
    }

    let mut maximum_gap_ns = 0u64;
    for pair in frames.windows(2) {
        let previous = pair[0].host_timestamp_unix_ns;
        let current = pair[1].host_timestamp_unix_ns;
        if current <= previous {
            reasons.push(PositionWindowRejection::DuplicateOrNonIncreasingTimestamp {
                rx_id,
                timestamp_unix_ns: current,
            });
            continue;
        }
        maximum_gap_ns = maximum_gap_ns.max(current - previous);
    }
    if maximum_gap_ns >= MAX_FRAME_GAP_NS {
        reasons.push(PositionWindowRejection::FrameGap {
            rx_id,
            maximum_gap_ns,
            required_less_than_ns: MAX_FRAME_GAP_NS,
        });
    }

    let observed_rate_millihz = if coverage_ns == 0 || frames.len() < 2 {
        0
    } else {
        let numerator = (frames.len() as u128 - 1) * 1_000_000_000_000u128;
        u64::try_from(numerator / u128::from(coverage_ns)).unwrap_or(u64::MAX)
    };
    let minimum_rate_millihz = MIN_RATE_HZ * 1_000;
    if observed_rate_millihz < minimum_rate_millihz {
        reasons.push(PositionWindowRejection::LowFrameRate {
            rx_id,
            observed_rate_millihz,
            minimum_rate_millihz,
        });
    }
    Some((coverage_ns, maximum_gap_ns, observed_rate_millihz))
}

fn distinct_grids(frames: &[&RawCsiFrame]) -> Vec<PositionGridIdentity> {
    let mut grids: Vec<PositionGridIdentity> = frames
        .iter()
        .map(|frame| PositionGridIdentity::from_frame(frame))
        .collect();
    grids.sort_unstable();
    grids.dedup();
    grids
}

fn validate_projection(
    frame: &RawCsiFrame,
    projection: EmptyProjection,
) -> Result<ProjectedFrame, String> {
    let expected_bins = usize::from(frame.antenna_count)
        .checked_mul(usize::from(frame.subcarrier_count))
        .ok_or_else(|| "frame CSI dimensions overflow usize".to_string())?;
    if expected_bins < FREQUENCY_BANDS {
        return Err(format!(
            "projection needs at least {FREQUENCY_BANDS} bins, got {expected_bins}"
        ));
    }
    if projection.signed_bin_residuals.len() != expected_bins {
        return Err(format!(
            "projection returned {} residuals for {expected_bins} CSI bins",
            projection.signed_bin_residuals.len()
        ));
    }
    if projection.stable_bins.len() != expected_bins {
        return Err(format!(
            "projection returned {} stable-bin flags for {expected_bins} CSI bins",
            projection.stable_bins.len()
        ));
    }
    if projection
        .stable_bins
        .iter()
        .filter(|stable| **stable)
        .count()
        < FREQUENCY_BANDS
    {
        return Err(format!(
            "projection needs at least {FREQUENCY_BANDS} stable CSI bins"
        ));
    }
    if projection
        .signed_bin_residuals
        .iter()
        .any(|value| !value.is_finite())
        || !projection.rssi_delta_db.is_finite()
        || !projection.log_csi_rms_delta.is_finite()
    {
        return Err("projection contains a non-finite value".to_string());
    }
    Ok(ProjectedFrame {
        signed_bin_residuals: projection.signed_bin_residuals,
        stable_bins: projection.stable_bins,
        rssi_delta_db: projection.rssi_delta_db,
        log_csi_rms_delta: projection.log_csi_rms_delta,
    })
}

/// Feature order:
///
/// - indices `0..24`: eight contiguous antenna-major frequency bands, each
///   `(median signed residual, median absolute residual,
///   median temporal MAD per bin)`;
/// - index 24: RSSI-delta median;
/// - index 25: RSSI-delta MAD;
/// - index 26: log-CSI-RMS-delta median;
/// - index 27: log-CSI-RMS-delta MAD.
fn build_feature_vector(
    projected: &[ProjectedFrame],
) -> Result<[f64; POSITION_FEATURE_COUNT], String> {
    let dimensions = projected
        .first()
        .map(|frame| frame.signed_bin_residuals.len())
        .ok_or_else(|| "cannot build features from an empty projected window".to_string())?;
    if dimensions < FREQUENCY_BANDS
        || projected
            .iter()
            .any(|frame| frame.signed_bin_residuals.len() != dimensions)
    {
        return Err("projected residual dimensions are inconsistent".to_string());
    }

    let mut features = [0.0; POSITION_FEATURE_COUNT];
    for band in 0..FREQUENCY_BANDS {
        let start = band * dimensions / FREQUENCY_BANDS;
        let end = (band + 1) * dimensions / FREQUENCY_BANDS;
        if start == end {
            return Err(format!("frequency band {band} contains no CSI bins"));
        }

        let mut signed_bin_medians = Vec::with_capacity(end - start);
        let mut absolute_bin_medians = Vec::with_capacity(end - start);
        let mut temporal_bin_mads = Vec::with_capacity(end - start);
        for bin in start..end {
            if !projected.iter().all(|frame| frame.stable_bins[bin]) {
                continue;
            }
            let temporal: Vec<f64> = projected
                .iter()
                .map(|frame| frame.signed_bin_residuals[bin])
                .collect();
            let signed_median = median(&temporal);
            let absolute: Vec<f64> = temporal.iter().map(|value| value.abs()).collect();
            signed_bin_medians.push(signed_median);
            absolute_bin_medians.push(median(&absolute));
            temporal_bin_mads.push(median_absolute_deviation(&temporal, signed_median));
        }
        if signed_bin_medians.is_empty() {
            return Err(format!("frequency band {band} contains no stable CSI bins"));
        }

        let feature_offset = band * 3;
        features[feature_offset] = median(&signed_bin_medians);
        features[feature_offset + 1] = median(&absolute_bin_medians);
        features[feature_offset + 2] = median(&temporal_bin_mads);
    }

    let rssi_deltas: Vec<f64> = projected.iter().map(|frame| frame.rssi_delta_db).collect();
    let rssi_median = median(&rssi_deltas);
    features[24] = rssi_median;
    features[25] = median_absolute_deviation(&rssi_deltas, rssi_median);

    let log_rms_deltas: Vec<f64> = projected
        .iter()
        .map(|frame| frame.log_csi_rms_delta)
        .collect();
    let log_rms_median = median(&log_rms_deltas);
    features[26] = log_rms_median;
    features[27] = median_absolute_deviation(&log_rms_deltas, log_rms_median);
    Ok(features)
}

fn validate_capture(capture: &PositionCapture) -> Result<(), String> {
    raw_csi_recording::validate_recording_id(&capture.recording_id)
        .map_err(|error| error.to_string())?;
    raw_csi_recording::validate_recording_id(&capture.setup_id)
        .map_err(|error| format!("invalid setup_id: {error}"))?;
    validate_sha256("setup_sha256", &capture.setup_sha256)?;
    if capture.server_version.trim().is_empty() {
        return Err(format!(
            "{} has an empty server version",
            capture.recording_id
        ));
    }
    validate_position_geometry(&capture.recording_id, &capture.geometry)?;
    if capture.started_at_unix_ns >= capture.ended_at_unix_ns {
        return Err(format!(
            "{} has invalid capture bounds {}..{}",
            capture.recording_id, capture.started_at_unix_ns, capture.ended_at_unix_ns
        ));
    }
    if capture.frames.is_empty() {
        return Err(format!("{} contains no frames", capture.recording_id));
    }
    for frame in &capture.frames {
        frame.validate().map_err(|error| {
            format!(
                "{} contains an invalid raw frame: {error}",
                capture.recording_id
            )
        })?;
        if !EXPECTED_RX_IDS.contains(&frame.rx_id) {
            return Err(format!(
                "{} contains unexpected RX{}; position extraction requires exactly RX1..RX4",
                capture.recording_id, frame.rx_id
            ));
        }
        if frame.host_timestamp_unix_ns < capture.started_at_unix_ns
            || frame.host_timestamp_unix_ns >= capture.ended_at_unix_ns
        {
            return Err(format!(
                "{} contains frame timestamp {} outside [{}..{})",
                capture.recording_id,
                frame.host_timestamp_unix_ns,
                capture.started_at_unix_ns,
                capture.ended_at_unix_ns
            ));
        }
    }
    position_source_binding(capture)?;
    Ok(())
}

/// Return the one complete TX-source proof shared by every frame in a position
/// capture. Generic raw-CSI-v1 decoding remains backward compatible with
/// historical frames where `source_binding` is absent; this position-specific
/// boundary deliberately rejects those frames as experiment evidence.
pub(crate) fn position_source_binding(capture: &PositionCapture) -> Result<SourceBinding, String> {
    let mut expected: Option<&SourceBinding> = None;
    for frame in &capture.frames {
        let binding = frame.source_binding.as_ref().ok_or_else(|| {
            format!(
                "{} RX{} sequence {} has no TX-source binding",
                capture.recording_id, frame.rx_id, frame.sequence
            )
        })?;
        validate_required_source_binding(
            &format!(
                "{} RX{} sequence {}",
                capture.recording_id, frame.rx_id, frame.sequence
            ),
            binding,
        )?;
        if let Some(expected) = expected {
            if binding != expected {
                return Err(format!(
                    "{} contains inconsistent TX-source bindings",
                    capture.recording_id
                ));
            }
        } else {
            expected = Some(binding);
        }
    }
    expected
        .cloned()
        .ok_or_else(|| format!("{} contains no frames", capture.recording_id))
}

fn validate_required_source_binding(context: &str, binding: &SourceBinding) -> Result<(), String> {
    binding
        .validate()
        .map_err(|error| format!("{context} has invalid TX-source binding: {error}"))?;
    if !binding.has_required_flags() {
        return Err(format!(
            "{context} has partial TX-source flags 0x{:02x}; exactly 0x{:02x} is required",
            binding.flags,
            raw_csi_recording::SOURCE_BINDING_REQUIRED_FLAGS
        ));
    }
    Ok(())
}

fn canonical_frames(capture: &PositionCapture) -> Result<Vec<&RawCsiFrame>, String> {
    let mut frames: Vec<&RawCsiFrame> = capture.frames.iter().collect();
    frames.sort_by(|left, right| {
        (
            left.host_timestamp_unix_ns,
            left.rx_id,
            left.sequence,
            PositionGridIdentity::from_frame(left),
        )
            .cmp(&(
                right.host_timestamp_unix_ns,
                right.rx_id,
                right.sequence,
                PositionGridIdentity::from_frame(right),
            ))
    });
    let mut identities = HashSet::new();
    for frame in &frames {
        let identity = (
            frame.host_timestamp_unix_ns,
            frame.rx_id,
            frame.sequence,
            PositionGridIdentity::from_frame(frame),
        );
        if !identities.insert(identity) {
            return Err(format!(
                "{} contains a duplicate RX{} sequence {} frame at {}",
                capture.recording_id, frame.rx_id, frame.sequence, frame.host_timestamp_unix_ns
            ));
        }
    }
    Ok(frames)
}

fn validate_sidecar(recording_id: &str, metadata: &PositionCaptureSidecar) -> Result<(), String> {
    if metadata.schema_version != raw_csi_recording::RAW_CSI_SCHEMA_VERSION {
        return Err(format!(
            "{recording_id} sidecar schema {} is unsupported",
            metadata.schema_version
        ));
    }
    if metadata.recording_id != recording_id {
        return Err(format!(
            "{recording_id} sidecar belongs to {}",
            metadata.recording_id
        ));
    }
    let setup_id = metadata
        .setup_id
        .as_deref()
        .ok_or_else(|| format!("{recording_id} has no setup_id"))?;
    raw_csi_recording::validate_recording_id(setup_id)
        .map_err(|error| format!("{recording_id} has invalid setup_id: {error}"))?;
    let setup_sha256 = metadata
        .setup_sha256
        .as_deref()
        .ok_or_else(|| format!("{recording_id} has no setup_sha256"))?;
    validate_sha256("setup_sha256", setup_sha256)
        .map_err(|error| format!("{recording_id}: {error}"))?;
    if metadata.server_version.trim().is_empty() {
        return Err(format!("{recording_id} has an empty server_version"));
    }
    let geometry = PositionCaptureGeometry {
        room_dimensions_m: metadata
            .room_dimensions
            .ok_or_else(|| format!("{recording_id} has no room_dimensions"))?,
        tx_position_m: metadata
            .tx_position
            .ok_or_else(|| format!("{recording_id} has no tx_position"))?,
        rx_positions_m: metadata.rx_positions.clone(),
    };
    validate_position_geometry(recording_id, &geometry)?;
    if metadata.started_at_unix_ns >= metadata.ended_at_unix_ns {
        return Err(format!("{recording_id} has invalid capture bounds"));
    }
    if metadata
        .started_at_unix_seconds
        .abs_diff(metadata.started_at_unix_ns / 1_000_000_000)
        > 1
        || metadata
            .ended_at_unix_seconds
            .abs_diff(metadata.ended_at_unix_ns / 1_000_000_000)
            > 1
    {
        return Err(format!(
            "{recording_id} second and nanosecond timestamps disagree"
        ));
    }
    let elapsed_seconds = (metadata.ended_at_unix_ns - metadata.started_at_unix_ns) / 1_000_000_000;
    if metadata.duration_secs.abs_diff(elapsed_seconds) > 1 {
        return Err(format!(
            "{recording_id} duration_secs disagrees with its timestamp bounds"
        ));
    }
    if metadata
        .max_duration_seconds
        .is_some_and(|maximum| !(1..=3_600).contains(&maximum) || maximum < metadata.duration_secs)
    {
        return Err(format!(
            "{recording_id} has an invalid max_duration_seconds watchdog"
        ));
    }
    if metadata.capture_scope != EXPECTED_CAPTURE_SCOPE {
        return Err(format!(
            "{recording_id} capture_scope {:?} is unsupported",
            metadata.capture_scope
        ));
    }
    if metadata.status != "completed"
        || metadata.incomplete
        || metadata.dropped_frames != 0
        || metadata
            .writer_error
            .as_ref()
            .is_some_and(|value| !value.is_null())
    {
        return Err(format!(
            "{recording_id} is incomplete or has recording loss"
        ));
    }
    if metadata.frames_written == 0 {
        return Err(format!("{recording_id} reports zero frames"));
    }
    if metadata.label.is_some() || metadata.ground_truth.is_some() {
        return Err(format!(
            "{recording_id} embeds a label or ground truth; position captures must be unlabelled"
        ));
    }
    Ok(())
}

fn validate_sidecar_rx_summaries(
    metadata: &PositionCaptureSidecar,
    frames: &[RawCsiFrame],
) -> Result<(), String> {
    let summary_ids: Vec<u8> = metadata
        .rx_summaries
        .iter()
        .map(|summary| summary.rx_id)
        .collect();
    if summary_ids != EXPECTED_RX_IDS {
        return Err(format!(
            "{} sidecar rx_summaries must be exactly RX1-RX4 in order, got {:?}",
            metadata.recording_id, summary_ids
        ));
    }

    let mut actual = BTreeMap::<u8, raw_csi_recording::RawCsiRxSummary>::new();
    for frame in frames {
        match actual.get_mut(&frame.rx_id) {
            Some(summary) => {
                summary
                    .validate_next_frame(frame)
                    .map_err(|error| format!("{}: {error}", metadata.recording_id))?;
                summary.observe_written_frame(frame);
            }
            None => {
                actual.insert(
                    frame.rx_id,
                    raw_csi_recording::RawCsiRxSummary::first_written_frame(frame),
                );
            }
        }
    }
    let actual: Vec<_> = actual.into_values().collect();
    if actual != metadata.rx_summaries {
        return Err(format!(
            "{} sidecar rx_summaries do not match the raw frames",
            metadata.recording_id
        ));
    }
    Ok(())
}

fn validate_position_geometry(
    recording_id: &str,
    geometry: &PositionCaptureGeometry,
) -> Result<(), String> {
    if geometry
        .room_dimensions_m
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(format!(
            "{recording_id} room dimensions must be finite and positive"
        ));
    }
    if geometry.rx_positions_m.len() != EXPECTED_RX_IDS.len() {
        return Err(format!(
            "{recording_id} needs geometry for exactly {} RX nodes, got {}",
            EXPECTED_RX_IDS.len(),
            geometry.rx_positions_m.len()
        ));
    }
    validate_room_position(
        recording_id,
        "tx_position",
        geometry.tx_position_m,
        geometry.room_dimensions_m,
    )?;
    for (index, position) in geometry.rx_positions_m.iter().copied().enumerate() {
        validate_room_position(
            recording_id,
            &format!("rx_positions[{index}]"),
            position,
            geometry.room_dimensions_m,
        )?;
    }
    Ok(())
}

fn validate_room_position(
    recording_id: &str,
    field: &str,
    position: [f64; 3],
    room_dimensions_m: [f64; 3],
) -> Result<(), String> {
    if position.iter().any(|value| !value.is_finite())
        || position
            .iter()
            .zip(room_dimensions_m)
            .any(|(coordinate, maximum)| *coordinate < 0.0 || *coordinate > maximum)
    {
        return Err(format!(
            "{recording_id} {field} {:?} lies outside room {:?}",
            position, room_dimensions_m
        ));
    }
    Ok(())
}

fn validate_loaded_frame(
    metadata: &PositionCaptureSidecar,
    frame: &RawCsiFrame,
    previous_timestamp: Option<u64>,
    path: &Path,
    line_number: usize,
) -> Result<(), String> {
    if frame.session_id.as_deref() != Some(metadata.recording_id.as_str()) {
        return Err(format!(
            "{} line {} session_id does not match {}",
            path.display(),
            line_number,
            metadata.recording_id
        ));
    }
    if frame.label.is_some() || frame.ground_truth.is_some() {
        return Err(format!(
            "{} line {} embeds a label or ground truth; position captures must be unlabelled",
            path.display(),
            line_number
        ));
    }
    if frame.host_timestamp_unix_ns < metadata.started_at_unix_ns
        || frame.host_timestamp_unix_ns >= metadata.ended_at_unix_ns
    {
        return Err(format!(
            "{} line {} timestamp lies outside the capture",
            path.display(),
            line_number
        ));
    }
    if previous_timestamp.is_some_and(|previous| frame.host_timestamp_unix_ns < previous) {
        return Err(format!(
            "{} line {} timestamp moves backwards",
            path.display(),
            line_number
        ));
    }
    if !EXPECTED_RX_IDS.contains(&frame.rx_id) {
        return Err(format!(
            "{} line {} contains unexpected RX{}",
            path.display(),
            line_number,
            frame.rx_id
        ));
    }
    Ok(())
}

fn sidecar_path(raw_path: &Path, recording_id: &str) -> PathBuf {
    raw_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{recording_id}.raw-csi.v1.meta.json"))
}

fn frame_amplitudes(frame: &RawCsiFrame) -> Vec<f64> {
    frame
        .iq_pairs
        .iter()
        .map(|pair| {
            let i = f64::from(pair.i);
            let q = f64::from(pair.q);
            (i * i + q * q).sqrt()
        })
        .collect()
}

fn frame_rssi_dbm(frame: &RawCsiFrame) -> f64 {
    let rssi = if frame.rssi_dbm > 0 {
        frame.rssi_dbm.saturating_neg()
    } else {
        frame.rssi_dbm
    };
    f64::from(rssi)
}

fn frame_log_csi_rms(amplitudes: &[f64]) -> Option<f64> {
    if amplitudes.is_empty()
        || amplitudes
            .iter()
            .any(|amplitude| !amplitude.is_finite() || *amplitude < 0.0)
    {
        return None;
    }
    let rms = (amplitudes
        .iter()
        .map(|amplitude| amplitude * amplitude)
        .sum::<f64>()
        / amplitudes.len() as f64)
        .sqrt();
    (rms > f64::EPSILON).then(|| rms.ln())
}

fn duration_ns(duration: Duration) -> Result<u64, String> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| "duration does not fit into nanoseconds".to_string())
}

fn validate_stable_band_coverage(reference: &FingerprintProjectionReference) -> Result<(), String> {
    let stable_bins = reference.stable_bins();
    if stable_bins.len() < FREQUENCY_BANDS {
        return Err(format!(
            "position projection needs at least {FREQUENCY_BANDS} CSI bins"
        ));
    }
    for band in 0..FREQUENCY_BANDS {
        let start = band * stable_bins.len() / FREQUENCY_BANDS;
        let end = (band + 1) * stable_bins.len() / FREQUENCY_BANDS;
        if !stable_bins[start..end].iter().any(|stable| *stable) {
            return Err(format!(
                "position projection frequency band {band} has no stable CSI bin"
            ));
        }
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        ))
    }
}

fn median(values: &[f64]) -> f64 {
    debug_assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let midpoint = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[midpoint - 1] + sorted[midpoint]) / 2.0
    } else {
        sorted[midpoint]
    }
}

fn median_absolute_deviation(values: &[f64], center: f64) -> f64 {
    let deviations: Vec<f64> = values.iter().map(|value| (value - center).abs()).collect();
    median(&deviations)
}

#[cfg(test)]
mod tests {
    use super::super::raw_csi_recording::{
        GroundTruth, IqPair, SourceBinding, SOURCE_BINDING_REQUIRED_FLAGS,
        TX_SOURCE_BINDING_SCHEME, TX_SOURCE_BINDING_VERSION,
    };
    use super::*;

    const CAPTURE_START: u64 = 1_000_000_000;
    const FEATURE_START: u64 = CAPTURE_START + SETTLING_NS;

    fn valid_source_binding() -> SourceBinding {
        SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: SOURCE_BINDING_REQUIRED_FLAGS,
            scheme: TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: "f".repeat(64),
        }
    }

    struct SyntheticProjection;

    impl EmptyProjectionReference for SyntheticProjection {
        fn project(&self, frame: &RawCsiFrame) -> Result<EmptyProjection, String> {
            Ok(EmptyProjection {
                signed_bin_residuals: frame
                    .iq_pairs
                    .iter()
                    .map(|pair| f64::from(pair.i))
                    .collect(),
                stable_bins: vec![true; frame.iq_pairs.len()],
                rssi_delta_db: f64::from(frame.rssi_dbm) + 50.0,
                log_csi_rms_delta: f64::from(frame.iq_pairs[0].q) / 10.0,
            })
        }
    }

    fn residual_for_rx(rx_id: u8) -> i8 {
        match rx_id {
            1 => -2,
            2 => 3,
            3 => -4,
            4 => 5,
            _ => unreachable!("tests only create RX1..RX4"),
        }
    }

    fn synthetic_frame(
        recording_id: &str,
        rx_id: u8,
        timestamp_unix_ns: u64,
        sequence: u32,
        residual: i8,
        flags: u8,
    ) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: timestamp_unix_ns,
            host_monotonic_ns: Some(timestamp_unix_ns),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: Some(recording_id.to_string()),
            label: Some("position".to_string()),
            ground_truth: Some(GroundTruth {
                occupied: Some(true),
                person_count: Some(1),
                position_m: Some([2.0, 1.0, 1.5]),
                activity: Some("still".to_string()),
            }),
            rx_id,
            antenna_count: 1,
            subcarrier_count: 8,
            center_frequency_mhz: 2_437,
            sequence,
            rssi_dbm: -48,
            noise_floor_dbm: -92,
            ppdu_type: 0,
            flags,
            mesh_timestamp_us: None,
            source_binding: Some(valid_source_binding()),
            iq_pairs: (0..8).map(|_| IqPair { i: residual, q: 2 }).collect(),
        }
    }

    fn regular_frames(recording_id: &str, end_unix_ns: u64) -> Vec<RawCsiFrame> {
        let mut frames = Vec::new();
        let mut sequence = 0u32;
        let mut timestamp = FEATURE_START;
        while timestamp < end_unix_ns {
            for rx_id in EXPECTED_RX_IDS {
                frames.push(synthetic_frame(
                    recording_id,
                    rx_id,
                    timestamp,
                    sequence,
                    residual_for_rx(rx_id),
                    0,
                ));
            }
            sequence += 1;
            timestamp += 200_000_000;
        }
        frames
    }

    fn test_geometry() -> PositionCaptureGeometry {
        PositionCaptureGeometry {
            room_dimensions_m: [4.02, 2.59, 3.44],
            tx_position_m: [1.51, 1.19, 0.39],
            rx_positions_m: vec![
                [0.0, 0.50, 0.28],
                [4.02, 0.87, 0.97],
                [0.0, 0.74, 2.11],
                [4.02, 0.87, 2.46],
            ],
        }
    }

    fn regular_capture(recording_id: &str) -> PositionCapture {
        let ended_at_unix_ns = CAPTURE_START + 10_000_000_000;
        PositionCapture {
            recording_id: recording_id.to_string(),
            setup_id: "fixed-room-test".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns: CAPTURE_START,
            ended_at_unix_ns,
            frames: regular_frames(recording_id, ended_at_unix_ns),
        }
    }

    fn has_reason(
        window: &RejectedPositionWindow,
        predicate: impl Fn(&PositionWindowRejection) -> bool,
    ) -> bool {
        window.reasons.iter().any(predicate)
    }

    #[test]
    fn uses_exact_three_second_windows_after_settling_and_rejects_partial_tail() {
        let capture = regular_capture("window-boundaries");
        let result =
            extract_position_feature_blocks(&capture, &SyntheticProjection).expect("extract");
        let starts: Vec<u64> = result
            .accepted_blocks
            .iter()
            .map(|block| block.window_start_unix_ns)
            .collect();
        assert_eq!(
            starts,
            vec![
                FEATURE_START,
                FEATURE_START + WINDOW_STEP_NS,
                FEATURE_START + 2 * WINDOW_STEP_NS
            ]
        );
        assert!(result.accepted_blocks.iter().all(|block| {
            block.window_end_unix_ns - block.window_start_unix_ns == WINDOW_NS
                && block.receivers.len() == 4
                && block
                    .receivers
                    .iter()
                    .all(|receiver| receiver.frame_count == 15)
        }));
        let incomplete_starts: Vec<u64> = result
            .rejected_windows
            .iter()
            .filter(|window| {
                has_reason(window, |reason| {
                    matches!(reason, PositionWindowRejection::IncompleteWindow { .. })
                })
            })
            .map(|window| window.window_start_unix_ns)
            .collect();
        assert_eq!(
            incomplete_starts,
            vec![
                FEATURE_START + 3 * WINDOW_STEP_NS,
                FEATURE_START + 4 * WINDOW_STEP_NS
            ]
        );
    }

    #[test]
    fn explicit_live_window_is_feature_identical_to_offline_extraction() {
        let capture = regular_capture("live-offline-parity");
        let offline =
            extract_position_feature_blocks(&capture, &SyntheticProjection).expect("offline");
        let live = extract_position_feature_window(&capture, &SyntheticProjection, FEATURE_START)
            .expect("explicit live window");
        assert_eq!(live, offline.accepted_blocks[0]);
    }

    #[test]
    fn position_capture_rejects_missing_partial_or_inconsistent_tx_binding() {
        let mut missing = regular_capture("missing-binding");
        missing.frames[0].source_binding = None;
        assert!(
            extract_position_feature_blocks(&missing, &SyntheticProjection)
                .expect_err("missing proof must fail closed")
                .contains("has no TX-source binding")
        );

        let mut partial = regular_capture("partial-binding");
        partial.frames[0].source_binding = Some(SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: 0,
            scheme: TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: "0".repeat(64),
        });
        assert!(
            extract_position_feature_blocks(&partial, &SyntheticProjection)
                .expect_err("partial proof must fail closed")
                .contains("partial TX-source flags")
        );

        let mut inconsistent = regular_capture("inconsistent-binding");
        inconsistent.frames[0]
            .source_binding
            .as_mut()
            .unwrap()
            .tx_filter_sha256 = "e".repeat(64);
        assert!(
            extract_position_feature_blocks(&inconsistent, &SyntheticProjection)
                .expect_err("mixed TX identity must fail closed")
                .contains("inconsistent TX-source bindings")
        );
    }

    #[test]
    fn explicit_live_window_rejects_gap_missing_receiver_and_grid_mix() {
        let mut gap = regular_capture("live-gap");
        gap.frames.retain(|frame| {
            frame.rx_id != 1
                || frame.host_timestamp_unix_ns <= FEATURE_START + 400_000_000
                || frame.host_timestamp_unix_ns >= FEATURE_START + 1_400_000_000
        });
        assert!(
            extract_position_feature_window(&gap, &SyntheticProjection, FEATURE_START)
                .expect_err("one-second gap must fail")
                .contains("FrameGap")
        );

        let mut missing = regular_capture("live-missing");
        missing.frames.retain(|frame| frame.rx_id != 4);
        assert!(
            extract_position_feature_window(&missing, &SyntheticProjection, FEATURE_START)
                .expect_err("missing RX4 must fail")
                .contains("MissingReceiver")
        );

        let mut mixed = regular_capture("live-grid-mix");
        let changed = mixed
            .frames
            .iter_mut()
            .find(|frame| {
                frame.rx_id == 2 && frame.host_timestamp_unix_ns == FEATURE_START + 400_000_000
            })
            .expect("target mixed-grid frame");
        changed.flags = 1;
        assert!(
            extract_position_feature_window(&mixed, &SyntheticProjection, FEATURE_START)
                .expect_err("mixed grid must fail")
                .contains("MixedGrid")
        );
    }

    #[test]
    fn preserves_positive_and_negative_empty_room_residuals() {
        let capture = regular_capture("signed-residuals");
        let result =
            extract_position_feature_blocks(&capture, &SyntheticProjection).expect("extract");
        let first = &result.accepted_blocks[0];
        let rx1 = &first.receivers[0];
        let rx2 = &first.receivers[1];
        assert_eq!(rx1.features.len(), POSITION_FEATURE_COUNT);
        assert_eq!(rx1.features[0], -2.0);
        assert_eq!(rx1.features[1], 2.0);
        assert_eq!(rx1.features[2], 0.0);
        assert_eq!(rx2.features[0], 3.0);
        assert_eq!(rx2.features[1], 3.0);
        assert_eq!(rx2.features[2], 0.0);
        assert_eq!(rx1.features[24], 2.0);
        assert_eq!(rx1.features[25], 0.0);
        assert_eq!(rx1.features[26], 0.2);
        assert_eq!(rx1.features[27], 0.0);
    }

    #[test]
    fn concrete_empty_reference_reuses_d6_and_ignores_only_sync_flag() {
        let ended_at_unix_ns = CAPTURE_START + 65_000_000_000;
        let capture = PositionCapture {
            recording_id: "empty-reference".to_string(),
            setup_id: "fixed-room-test".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns: CAPTURE_START,
            ended_at_unix_ns,
            frames: regular_frames("empty-reference", ended_at_unix_ns),
        };
        let reference = build_position_empty_reference(&capture, &"a".repeat(64))
            .expect("build concrete D6 reference");
        reference.validate().expect("reference validates");

        let mut same_signal = synthetic_frame(
            "measurement",
            1,
            ended_at_unix_ns + 1,
            1,
            residual_for_rx(1),
            0x10,
        );
        same_signal.label = None;
        same_signal.ground_truth = None;
        let projection = reference
            .project(&same_signal)
            .expect("sync bit does not change the CSI layout");
        assert!(projection
            .signed_bin_residuals
            .iter()
            .all(|residual| residual.abs() < 1e-12));
        assert_eq!(projection.rssi_delta_db, 0.0);
        assert!(projection.log_csi_rms_delta.abs() < 1e-12);
        assert!(projection.stable_bins.iter().all(|stable| *stable));

        same_signal.flags = 0x01;
        assert!(reference.project(&same_signal).is_err());
    }

    #[test]
    fn empty_reference_validation_rejects_corrupted_private_state_and_context() {
        let ended_at_unix_ns = CAPTURE_START + 65_000_000_000;
        let capture = PositionCapture {
            recording_id: "validated-empty-reference".to_string(),
            setup_id: "fixed-room-test".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns: CAPTURE_START,
            ended_at_unix_ns,
            frames: regular_frames("validated-empty-reference", ended_at_unix_ns),
        };
        let reference = build_position_empty_reference(&capture, &"a".repeat(64))
            .expect("build valid reference");

        let mut wrong_order = reference.clone();
        wrong_order.receivers.swap(0, 1);
        assert!(wrong_order
            .validate()
            .expect_err("binary-search order must be protected")
            .contains("exactly [1, 2, 3, 4] in order"));

        let mut wrong_dimensions = reference.clone();
        wrong_dimensions.receivers[0].grid.subcarrier_count += 1;
        assert!(wrong_dimensions
            .validate()
            .expect_err("grid and D6 dimensions must match")
            .contains("grid requires"));

        let mut non_finite = reference.clone();
        non_finite.receivers[0].rssi_median_dbm = f64::NAN;
        assert!(non_finite
            .validate()
            .expect_err("non-finite baseline must fail closed")
            .contains("non-finite signal baseline"));

        let mut wrong_context = capture.clone();
        wrong_context.setup_sha256 = "b".repeat(64);
        assert!(reference
            .validate_capture_context(&wrong_context)
            .expect_err("capture setup must match the reference")
            .contains("setup identity differs"));

        let mut wrong_tx = capture;
        for frame in &mut wrong_tx.frames {
            frame.source_binding.as_mut().unwrap().tx_filter_sha256 = "e".repeat(64);
        }
        assert!(reference
            .validate_capture_context(&wrong_tx)
            .expect_err("capture TX identity must match the reference")
            .contains("TX-source binding differs"));
    }

    fn valid_position_sidecar_value() -> serde_json::Value {
        serde_json::json!({
            "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
            "recording_id": "blind-p05",
            "setup_id": "fixed-room-test",
            "setup_sha256": "a".repeat(64),
            "server_version": "test",
            "started_at_unix_seconds": 1,
            "started_at_unix_ns": 1_000_000_000_u64,
            "ended_at_unix_seconds": 36,
            "ended_at_unix_ns": 36_000_000_000_u64,
            "duration_secs": 35,
            "tx_position": [1.51, 1.19, 0.39],
            "rx_positions": test_geometry().rx_positions_m,
            "room_dimensions": [4.02, 2.59, 3.44],
            "capture_scope": EXPECTED_CAPTURE_SCOPE,
            "max_duration_seconds": 50,
            "status": "completed",
            "frames_written": 700,
            "rx_summaries": [],
            "dropped_frames": 0,
            "incomplete": false,
            "writer_error": null,
            "label": null,
            "ground_truth": null
        })
    }

    #[test]
    fn position_sidecar_accepts_recorder_watchdog_and_rx_summaries() {
        let sidecar = valid_position_sidecar_value();
        let parsed = serde_json::from_value::<PositionCaptureSidecar>(sidecar)
            .expect("current recorder sidecar fields must be accepted");
        assert_eq!(parsed.max_duration_seconds, Some(50));
        assert!(parsed.rx_summaries.is_empty());
    }

    #[test]
    fn position_sidecar_rejects_unknown_truth_like_fields() {
        let mut sidecar = valid_position_sidecar_value();
        sidecar["expected_point_id"] = serde_json::json!("P05");

        assert!(serde_json::from_value::<PositionCaptureSidecar>(sidecar).is_err());
    }

    #[test]
    fn empty_reference_rejects_a_bursty_or_gapped_calibration_block() {
        let ended_at_unix_ns = CAPTURE_START + 65_000_000_000;
        let mut capture = PositionCapture {
            recording_id: "gapped-empty-reference".to_string(),
            setup_id: "fixed-room-test".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns: CAPTURE_START,
            ended_at_unix_ns,
            frames: regular_frames("gapped-empty-reference", ended_at_unix_ns),
        };
        let gap_start = FEATURE_START + 12_000_000_000;
        let gap_end = FEATURE_START + 14_000_000_000;
        capture.frames.retain(|frame| {
            frame.rx_id != 1
                || frame.host_timestamp_unix_ns < gap_start
                || frame.host_timestamp_unix_ns >= gap_end
        });

        let error = build_position_empty_reference(&capture, &"a".repeat(64))
            .expect_err("a two-second calibration gap must fail closed");
        assert!(error.contains("failed timing quality"));
    }

    #[test]
    fn rejects_a_window_that_contains_an_exact_grid_change() {
        let mut capture = regular_capture("grid-change");
        let changed_at = FEATURE_START + 400_000_000;
        let frame = capture
            .frames
            .iter_mut()
            .find(|frame| frame.rx_id == 1 && frame.host_timestamp_unix_ns == changed_at)
            .expect("target frame");
        frame.flags = 1;

        let result =
            extract_position_feature_blocks(&capture, &SyntheticProjection).expect("extract");
        let first = result
            .rejected_windows
            .iter()
            .find(|window| window.window_start_unix_ns == FEATURE_START)
            .expect("first window rejected");
        assert!(has_reason(first, |reason| matches!(
            reason,
            PositionWindowRejection::MixedGrid { rx_id: 1, grids } if grids.len() == 2
        )));
        assert!(result
            .accepted_blocks
            .iter()
            .any(|block| block.window_start_unix_ns == FEATURE_START + WINDOW_STEP_NS));
    }

    #[test]
    fn diagnoses_a_missing_receiver_instead_of_emitting_a_block() {
        let mut capture = regular_capture("missing-rx");
        capture.frames.retain(|frame| frame.rx_id != 4);
        let result =
            extract_position_feature_blocks(&capture, &SyntheticProjection).expect("extract");
        assert!(result.accepted_blocks.is_empty());
        assert!(result.rejected_windows.iter().any(|window| {
            has_reason(window, |reason| {
                matches!(
                    reason,
                    PositionWindowRejection::MissingReceiver { rx_id: 4 }
                )
            })
        }));
    }

    #[test]
    fn diagnoses_low_rate_and_a_one_second_gap() {
        let ended_at_unix_ns = FEATURE_START + WINDOW_NS;
        let mut low_rate_frames = Vec::new();
        for rx_id in EXPECTED_RX_IDS {
            let offsets: Vec<u64> = if rx_id == 1 {
                (0..15).map(|index| index * 207_142_857).collect()
            } else {
                (0..15).map(|index| index * 200_000_000).collect()
            };
            for (sequence, offset) in offsets.into_iter().enumerate() {
                low_rate_frames.push(synthetic_frame(
                    "low-rate",
                    rx_id,
                    FEATURE_START + offset,
                    sequence as u32,
                    residual_for_rx(rx_id),
                    0,
                ));
            }
        }
        let low_rate_capture = PositionCapture {
            recording_id: "low-rate".to_string(),
            setup_id: "fixed-room-test".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns: CAPTURE_START,
            ended_at_unix_ns,
            frames: low_rate_frames,
        };
        let low_rate = extract_position_feature_blocks(&low_rate_capture, &SyntheticProjection)
            .expect("low-rate diagnostics");
        assert!(has_reason(&low_rate.rejected_windows[0], |reason| {
            matches!(
                reason,
                PositionWindowRejection::LowFrameRate { rx_id: 1, .. }
            )
        }));

        let mut gap_capture = PositionCapture {
            recording_id: "frame-gap".to_string(),
            setup_id: "fixed-room-test".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns: CAPTURE_START,
            ended_at_unix_ns,
            frames: regular_frames("frame-gap", ended_at_unix_ns),
        };
        gap_capture.frames.retain(|frame| {
            frame.rx_id != 1
                || frame.host_timestamp_unix_ns <= FEATURE_START + 400_000_000
                || frame.host_timestamp_unix_ns >= FEATURE_START + 1_400_000_000
        });
        let gap = extract_position_feature_blocks(&gap_capture, &SyntheticProjection)
            .expect("gap diagnostics");
        assert!(has_reason(&gap.rejected_windows[0], |reason| {
            matches!(
                reason,
                PositionWindowRejection::FrameGap {
                    rx_id: 1,
                    maximum_gap_ns: MAX_FRAME_GAP_NS,
                    ..
                }
            )
        }));
    }

    #[test]
    fn canonical_sorting_makes_output_independent_of_input_order() {
        let first_capture = regular_capture("deterministic");
        let mut reversed_capture = first_capture.clone();
        reversed_capture.frames.reverse();

        let first = extract_position_feature_blocks(&first_capture, &SyntheticProjection)
            .expect("first extraction");
        let second = extract_position_feature_blocks(&reversed_capture, &SyntheticProjection)
            .expect("second extraction");
        assert_eq!(
            serde_json::to_vec(&first).expect("first JSON"),
            serde_json::to_vec(&second).expect("second JSON")
        );
    }
}
