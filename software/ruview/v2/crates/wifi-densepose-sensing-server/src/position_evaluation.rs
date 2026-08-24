//! Deterministic, I/O-free evaluation for the fixed nine-point position model.
//!
//! Prediction and truth stay deliberately separate. The prediction artifact
//! contains no labels, while the truth manifest binds itself to the exact
//! prediction, index, setup, recording IDs, raw captures, metadata, and derived
//! signals. Evaluation fails closed on any structural or provenance mismatch.
//! Poor model accuracy, however, is a valid measurement and therefore produces
//! a normal report.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::position_fingerprint::{FingerprintPosition, POSITION_COUNT};

pub(crate) const PREDICTION_ARTIFACT_SCHEMA_VERSION: u16 = 1;
pub(crate) const TRUTH_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub(crate) const EVALUATION_REPORT_SCHEMA_VERSION: u16 = 2;

const PREDICTION_ARTIFACT_KIND: &str = "ruview.position-predictions";
const TRUTH_MANIFEST_KIND: &str = "ruview.position-truth";
pub(crate) const EVALUATION_REPORT_KIND: &str = "ruview.position-evaluation";
const SHA256_HEX_LENGTH: usize = 64;
const REQUIRED_BLIND_CAPTURES: u64 = 18;
const MINIMUM_MATCHED_CAPTURES: u64 = 16;
const MINIMUM_CORRECT_CAPTURES: u64 = 15;
const MINIMUM_DECIDED_ACCURACY: f64 = 0.90;
const MAXIMUM_ABSTENTIONS: u64 = 2;
const REQUIRED_MEDIAN_FLOOR_ERROR_M: f64 = 0.0;
const MAXIMUM_FLOOR_ERROR_M: f64 = 1.30;

/// Final per-capture decision emitted by the blind prediction step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum CapturePredictionStatus {
    Matched { point_id: String },
    Unknown,
    Ambiguous,
    InsufficientEvidence,
}

/// One blind capture and its prediction. No truth is stored here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CapturePrediction {
    recording_id: String,
    raw_sha256: String,
    metadata_sha256: String,
    signal_sha256: String,
    prediction: CapturePredictionStatus,
}

impl CapturePrediction {
    pub(crate) fn new(
        recording_id: impl Into<String>,
        raw_sha256: impl Into<String>,
        metadata_sha256: impl Into<String>,
        signal_sha256: impl Into<String>,
        prediction: CapturePredictionStatus,
    ) -> Self {
        Self {
            recording_id: recording_id.into(),
            raw_sha256: raw_sha256.into(),
            metadata_sha256: metadata_sha256.into(),
            signal_sha256: signal_sha256.into(),
            prediction,
        }
    }

    pub(crate) fn recording_id(&self) -> &str {
        &self.recording_id
    }

    pub(crate) fn raw_sha256(&self) -> &str {
        &self.raw_sha256
    }

    pub(crate) fn metadata_sha256(&self) -> &str {
        &self.metadata_sha256
    }

    pub(crate) fn signal_sha256(&self) -> &str {
        &self.signal_sha256
    }

    pub(crate) fn prediction(&self) -> &CapturePredictionStatus {
        &self.prediction
    }
}

/// Serializable output of the blind prediction step.
///
/// Construction canonicalizes reference points and captures so equivalent
/// inputs serialize identically regardless of caller order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionPredictionArtifact {
    schema_version: u16,
    kind: String,
    algorithm_id: String,
    index_sha256: String,
    setup_sha256: String,
    reference_points: Vec<FingerprintPosition>,
    captures: Vec<CapturePrediction>,
}

impl PositionPredictionArtifact {
    pub(crate) fn new(
        algorithm_id: impl Into<String>,
        index_sha256: impl Into<String>,
        setup_sha256: impl Into<String>,
        mut reference_points: Vec<FingerprintPosition>,
        mut captures: Vec<CapturePrediction>,
    ) -> Result<Self, PositionEvaluationError> {
        reference_points.sort_by(|left, right| left.id.cmp(&right.id));
        captures.sort_by(|left, right| {
            left.recording_id
                .cmp(&right.recording_id)
                .then_with(|| left.raw_sha256.cmp(&right.raw_sha256))
                .then_with(|| left.metadata_sha256.cmp(&right.metadata_sha256))
                .then_with(|| left.signal_sha256.cmp(&right.signal_sha256))
        });
        let artifact = Self {
            schema_version: PREDICTION_ARTIFACT_SCHEMA_VERSION,
            kind: PREDICTION_ARTIFACT_KIND.to_string(),
            algorithm_id: algorithm_id.into(),
            index_sha256: index_sha256.into(),
            setup_sha256: setup_sha256.into(),
            reference_points,
            captures,
        };
        validate_prediction_artifact(&artifact)?;
        Ok(artifact)
    }

    pub(crate) fn algorithm_id(&self) -> &str {
        &self.algorithm_id
    }

    pub(crate) fn index_sha256(&self) -> &str {
        &self.index_sha256
    }

    pub(crate) fn setup_sha256(&self) -> &str {
        &self.setup_sha256
    }

    pub(crate) fn reference_points(&self) -> &[FingerprintPosition] {
        &self.reference_points
    }

    pub(crate) fn captures(&self) -> &[CapturePrediction] {
        &self.captures
    }
}

/// Ground truth for one capture, supplied only to the evaluation step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PositionTruthItem {
    recording_id: String,
    raw_sha256: String,
    metadata_sha256: String,
    signal_sha256: String,
    expected_point_id: String,
}

impl PositionTruthItem {
    pub(crate) fn new(
        recording_id: impl Into<String>,
        raw_sha256: impl Into<String>,
        metadata_sha256: impl Into<String>,
        signal_sha256: impl Into<String>,
        expected_point_id: impl Into<String>,
    ) -> Self {
        Self {
            recording_id: recording_id.into(),
            raw_sha256: raw_sha256.into(),
            metadata_sha256: metadata_sha256.into(),
            signal_sha256: signal_sha256.into(),
            expected_point_id: expected_point_id.into(),
        }
    }

    pub(crate) fn recording_id(&self) -> &str {
        &self.recording_id
    }

    pub(crate) fn raw_sha256(&self) -> &str {
        &self.raw_sha256
    }

    pub(crate) fn metadata_sha256(&self) -> &str {
        &self.metadata_sha256
    }

    pub(crate) fn signal_sha256(&self) -> &str {
        &self.signal_sha256
    }

    pub(crate) fn expected_point_id(&self) -> &str {
        &self.expected_point_id
    }
}

/// Separately supplied truth, cryptographically bound to the evaluated inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PositionTruthManifest {
    schema_version: u16,
    kind: String,
    predictions_sha256: String,
    index_sha256: String,
    setup_sha256: String,
    items: Vec<PositionTruthItem>,
}

impl PositionTruthManifest {
    pub(crate) fn new(
        predictions_sha256: impl Into<String>,
        index_sha256: impl Into<String>,
        setup_sha256: impl Into<String>,
        mut items: Vec<PositionTruthItem>,
    ) -> Result<Self, PositionEvaluationError> {
        items.sort_by(|left, right| {
            left.recording_id
                .cmp(&right.recording_id)
                .then_with(|| left.raw_sha256.cmp(&right.raw_sha256))
                .then_with(|| left.metadata_sha256.cmp(&right.metadata_sha256))
                .then_with(|| left.signal_sha256.cmp(&right.signal_sha256))
        });
        let manifest = Self {
            schema_version: TRUTH_MANIFEST_SCHEMA_VERSION,
            kind: TRUTH_MANIFEST_KIND.to_string(),
            predictions_sha256: predictions_sha256.into(),
            index_sha256: index_sha256.into(),
            setup_sha256: setup_sha256.into(),
            items,
        };
        validate_truth_manifest(&manifest)?;
        Ok(manifest)
    }

    pub(crate) fn predictions_sha256(&self) -> &str {
        &self.predictions_sha256
    }

    pub(crate) fn index_sha256(&self) -> &str {
        &self.index_sha256
    }

    pub(crate) fn setup_sha256(&self) -> &str {
        &self.setup_sha256
    }

    pub(crate) fn items(&self) -> &[PositionTruthItem] {
        &self.items
    }
}

/// One expected-position row in the deterministic confusion matrix.
///
/// `matched_by_point` uses the same order as
/// [`PositionConfusionMatrix::predicted_point_ids`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PositionConfusionRow {
    pub(crate) expected_point_id: String,
    pub(crate) matched_by_point: Vec<u64>,
    pub(crate) unknown: u64,
    pub(crate) ambiguous: u64,
    pub(crate) insufficient_evidence: u64,
}

/// A fixed 9×9 matched-position matrix plus the three abstention columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PositionConfusionMatrix {
    pub(crate) predicted_point_ids: Vec<String>,
    pub(crate) rows: Vec<PositionConfusionRow>,
}

/// Overall decision for the frozen 18-capture blind-position protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PositionAcceptanceVerdict {
    Pass,
    Fail,
}

/// Machine-readable results for every predeclared position acceptance gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PositionAcceptanceGates {
    pub(crate) exactly_eighteen_blind_captures: bool,
    pub(crate) coverage_at_least_sixteen_of_eighteen: bool,
    pub(crate) accuracy_all_at_least_fifteen_of_eighteen: bool,
    pub(crate) accuracy_decided_at_least_ninety_percent: bool,
    pub(crate) every_point_has_correct_repetition: bool,
    pub(crate) points_without_correct_repetition: Vec<String>,
    pub(crate) abstentions_at_most_two: bool,
    pub(crate) median_floor_error_is_zero: bool,
    pub(crate) p95_floor_error_at_most_1_30_m: bool,
    pub(crate) maximum_wrong_floor_error_at_most_1_30_m: bool,
}

/// Deterministic evaluation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionEvaluationReport {
    pub(crate) schema_version: u16,
    pub(crate) kind: String,
    pub(crate) algorithm_id: String,
    pub(crate) predictions_sha256: String,
    pub(crate) index_sha256: String,
    pub(crate) setup_sha256: String,
    pub(crate) total: u64,
    pub(crate) matched: u64,
    pub(crate) correct: u64,
    pub(crate) unknown: u64,
    pub(crate) ambiguous: u64,
    pub(crate) insufficient_evidence: u64,
    pub(crate) abstentions: u64,
    pub(crate) coverage: f64,
    pub(crate) accuracy_all: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) accuracy_decided: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) median_floor_error_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) p95_floor_error_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_floor_error_m: Option<f64>,
    pub(crate) position_verdict: PositionAcceptanceVerdict,
    pub(crate) position_verdict_reasons: Vec<String>,
    pub(crate) acceptance_gates: PositionAcceptanceGates,
    pub(crate) confusion: PositionConfusionMatrix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PositionEvaluationError {
    UnsupportedPredictionSchema {
        expected: u16,
        actual: u16,
    },
    UnexpectedPredictionKind {
        actual: String,
    },
    UnsupportedTruthSchema {
        expected: u16,
        actual: u16,
    },
    UnexpectedTruthKind {
        actual: String,
    },
    EmptyAlgorithmId,
    InvalidSha256 {
        field: String,
    },
    PredictionHashMismatch,
    IndexHashMismatch,
    SetupHashMismatch,
    ReferencePointCount {
        expected: usize,
        actual: usize,
    },
    EmptyPointId,
    DuplicatePointId {
        point_id: String,
    },
    DuplicatePointCoordinates {
        first_id: String,
        second_id: String,
    },
    NonFinitePointCoordinate {
        point_id: String,
        coordinate_index: usize,
    },
    EmptyPredictions,
    EmptyTruth,
    EmptyRecordingId {
        source: &'static str,
    },
    DuplicatePredictionRecordingId {
        recording_id: String,
    },
    DuplicatePredictionRawSha256 {
        raw_sha256: String,
    },
    DuplicatePredictionMetadataSha256 {
        metadata_sha256: String,
    },
    DuplicatePredictionSignalSha256 {
        signal_sha256: String,
    },
    DuplicateTruthRecordingId {
        recording_id: String,
    },
    DuplicateTruthRawSha256 {
        raw_sha256: String,
    },
    DuplicateTruthMetadataSha256 {
        metadata_sha256: String,
    },
    DuplicateTruthSignalSha256 {
        signal_sha256: String,
    },
    EmptyExpectedPointId {
        recording_id: String,
    },
    EmptyPredictedPointId {
        recording_id: String,
    },
    UnknownExpectedPointId {
        recording_id: String,
        point_id: String,
    },
    UnknownPredictedPointId {
        recording_id: String,
        point_id: String,
    },
    MissingTruthForPrediction {
        recording_id: String,
    },
    MissingPredictionForTruth {
        recording_id: String,
    },
    CaptureRawSha256Mismatch {
        recording_id: String,
    },
    CaptureMetadataSha256Mismatch {
        recording_id: String,
    },
    CaptureSignalSha256Mismatch {
        recording_id: String,
    },
}

impl fmt::Display for PositionEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPredictionSchema { expected, actual } => write!(
                formatter,
                "unsupported prediction schema {actual}; expected {expected}"
            ),
            Self::UnexpectedPredictionKind { actual } => write!(
                formatter,
                "prediction kind is {actual:?}; expected {PREDICTION_ARTIFACT_KIND:?}"
            ),
            Self::UnsupportedTruthSchema { expected, actual } => write!(
                formatter,
                "unsupported truth schema {actual}; expected {expected}"
            ),
            Self::UnexpectedTruthKind { actual } => write!(
                formatter,
                "truth kind is {actual:?}; expected {TRUTH_MANIFEST_KIND:?}"
            ),
            Self::EmptyAlgorithmId => {
                write!(formatter, "prediction algorithm_id must not be empty")
            }
            Self::InvalidSha256 { field } => {
                write!(
                    formatter,
                    "{field} must be 64 lowercase hexadecimal characters"
                )
            }
            Self::PredictionHashMismatch => {
                write!(
                    formatter,
                    "truth predictions_sha256 does not bind this prediction file"
                )
            }
            Self::IndexHashMismatch => {
                write!(
                    formatter,
                    "truth and predictions use different index_sha256 values"
                )
            }
            Self::SetupHashMismatch => {
                write!(
                    formatter,
                    "truth and predictions use different setup_sha256 values"
                )
            }
            Self::ReferencePointCount { expected, actual } => write!(
                formatter,
                "prediction artifact requires exactly {expected} reference points, got {actual}"
            ),
            Self::EmptyPointId => write!(formatter, "reference point ID must not be empty"),
            Self::DuplicatePointId { point_id } => {
                write!(formatter, "duplicate reference point ID {point_id:?}")
            }
            Self::DuplicatePointCoordinates {
                first_id,
                second_id,
            } => write!(
                formatter,
                "reference points {first_id:?} and {second_id:?} share floor coordinates (x, z)"
            ),
            Self::NonFinitePointCoordinate {
                point_id,
                coordinate_index,
            } => write!(
                formatter,
                "reference point {point_id:?} coordinate {coordinate_index} is not finite"
            ),
            Self::EmptyPredictions => write!(formatter, "prediction artifact contains no captures"),
            Self::EmptyTruth => write!(formatter, "truth manifest contains no captures"),
            Self::EmptyRecordingId { source } => {
                write!(formatter, "{source} recording_id must not be empty")
            }
            Self::DuplicatePredictionRecordingId { recording_id } => write!(
                formatter,
                "prediction artifact repeats recording_id {recording_id:?}"
            ),
            Self::DuplicatePredictionRawSha256 { raw_sha256 } => write!(
                formatter,
                "prediction artifact repeats raw_sha256 {raw_sha256}"
            ),
            Self::DuplicatePredictionMetadataSha256 { metadata_sha256 } => write!(
                formatter,
                "prediction artifact repeats metadata_sha256 {metadata_sha256}"
            ),
            Self::DuplicatePredictionSignalSha256 { signal_sha256 } => write!(
                formatter,
                "prediction artifact repeats signal_sha256 {signal_sha256}"
            ),
            Self::DuplicateTruthRecordingId { recording_id } => {
                write!(
                    formatter,
                    "truth manifest repeats recording_id {recording_id:?}"
                )
            }
            Self::DuplicateTruthRawSha256 { raw_sha256 } => {
                write!(formatter, "truth manifest repeats raw_sha256 {raw_sha256}")
            }
            Self::DuplicateTruthMetadataSha256 { metadata_sha256 } => write!(
                formatter,
                "truth manifest repeats metadata_sha256 {metadata_sha256}"
            ),
            Self::DuplicateTruthSignalSha256 { signal_sha256 } => write!(
                formatter,
                "truth manifest repeats signal_sha256 {signal_sha256}"
            ),
            Self::EmptyExpectedPointId { recording_id } => write!(
                formatter,
                "truth for recording {recording_id:?} has an empty expected_point_id"
            ),
            Self::EmptyPredictedPointId { recording_id } => write!(
                formatter,
                "prediction for recording {recording_id:?} has an empty point_id"
            ),
            Self::UnknownExpectedPointId {
                recording_id,
                point_id,
            } => write!(
                formatter,
                "truth for recording {recording_id:?} references unknown point {point_id:?}"
            ),
            Self::UnknownPredictedPointId {
                recording_id,
                point_id,
            } => write!(
                formatter,
                "prediction for recording {recording_id:?} references unknown point {point_id:?}"
            ),
            Self::MissingTruthForPrediction { recording_id } => write!(
                formatter,
                "prediction recording {recording_id:?} has no matching truth item"
            ),
            Self::MissingPredictionForTruth { recording_id } => write!(
                formatter,
                "truth recording {recording_id:?} has no matching prediction"
            ),
            Self::CaptureRawSha256Mismatch { recording_id } => write!(
                formatter,
                "prediction and truth raw_sha256 differ for recording {recording_id:?}"
            ),
            Self::CaptureMetadataSha256Mismatch { recording_id } => write!(
                formatter,
                "prediction and truth metadata_sha256 differ for recording {recording_id:?}"
            ),
            Self::CaptureSignalSha256Mismatch { recording_id } => write!(
                formatter,
                "prediction and truth signal_sha256 differ for recording {recording_id:?}"
            ),
        }
    }
}

impl Error for PositionEvaluationError {}

/// Evaluate blind predictions against a separately supplied truth manifest.
///
/// `prediction_file_sha256` is calculated by the I/O layer over the exact
/// serialized prediction artifact bytes. Passing it explicitly avoids a
/// circular self-hash field while keeping this module pure.
pub(crate) fn evaluate(
    predictions: &PositionPredictionArtifact,
    prediction_file_sha256: &str,
    truth: &PositionTruthManifest,
) -> Result<PositionEvaluationReport, PositionEvaluationError> {
    validate_prediction_artifact(predictions)?;
    validate_truth_manifest(truth)?;
    validate_sha256("prediction_file_sha256", prediction_file_sha256)?;

    if truth.predictions_sha256 != prediction_file_sha256 {
        return Err(PositionEvaluationError::PredictionHashMismatch);
    }
    if truth.index_sha256 != predictions.index_sha256 {
        return Err(PositionEvaluationError::IndexHashMismatch);
    }
    if truth.setup_sha256 != predictions.setup_sha256 {
        return Err(PositionEvaluationError::SetupHashMismatch);
    }

    let reference_points = canonical_reference_points(&predictions.reference_points)?;
    let point_index: BTreeMap<String, usize> = reference_points
        .iter()
        .enumerate()
        .map(|(index, point)| (point.id.clone(), index))
        .collect();
    let point_by_id: BTreeMap<String, &FingerprintPosition> = reference_points
        .iter()
        .map(|point| (point.id.clone(), point))
        .collect();

    let predictions_by_recording: BTreeMap<&str, &CapturePrediction> = predictions
        .captures
        .iter()
        .map(|capture| (capture.recording_id.as_str(), capture))
        .collect();
    let truth_by_recording: BTreeMap<&str, &PositionTruthItem> = truth
        .items
        .iter()
        .map(|item| (item.recording_id.as_str(), item))
        .collect();

    for recording_id in predictions_by_recording.keys() {
        if !truth_by_recording.contains_key(recording_id) {
            return Err(PositionEvaluationError::MissingTruthForPrediction {
                recording_id: (*recording_id).to_string(),
            });
        }
    }
    for recording_id in truth_by_recording.keys() {
        if !predictions_by_recording.contains_key(recording_id) {
            return Err(PositionEvaluationError::MissingPredictionForTruth {
                recording_id: (*recording_id).to_string(),
            });
        }
    }

    let point_ids: Vec<String> = reference_points
        .iter()
        .map(|point| point.id.clone())
        .collect();
    let mut confusion_rows: Vec<PositionConfusionRow> = point_ids
        .iter()
        .map(|point_id| PositionConfusionRow {
            expected_point_id: point_id.clone(),
            matched_by_point: vec![0; POSITION_COUNT],
            unknown: 0,
            ambiguous: 0,
            insufficient_evidence: 0,
        })
        .collect();

    let mut matched = 0u64;
    let mut correct = 0u64;
    let mut unknown = 0u64;
    let mut ambiguous = 0u64;
    let mut insufficient_evidence = 0u64;
    let mut floor_errors = Vec::new();

    for (recording_id, truth_item) in &truth_by_recording {
        let prediction = predictions_by_recording
            .get(recording_id)
            .expect("capture-set equality was checked");
        if prediction.raw_sha256 != truth_item.raw_sha256 {
            return Err(PositionEvaluationError::CaptureRawSha256Mismatch {
                recording_id: (*recording_id).to_string(),
            });
        }
        if prediction.metadata_sha256 != truth_item.metadata_sha256 {
            return Err(PositionEvaluationError::CaptureMetadataSha256Mismatch {
                recording_id: (*recording_id).to_string(),
            });
        }
        if prediction.signal_sha256 != truth_item.signal_sha256 {
            return Err(PositionEvaluationError::CaptureSignalSha256Mismatch {
                recording_id: (*recording_id).to_string(),
            });
        }
        let expected_index = *point_index
            .get(&truth_item.expected_point_id)
            .ok_or_else(|| PositionEvaluationError::UnknownExpectedPointId {
                recording_id: (*recording_id).to_string(),
                point_id: truth_item.expected_point_id.clone(),
            })?;
        let row = &mut confusion_rows[expected_index];

        match &prediction.prediction {
            CapturePredictionStatus::Matched { point_id } => {
                let predicted_index = *point_index.get(point_id).ok_or_else(|| {
                    PositionEvaluationError::UnknownPredictedPointId {
                        recording_id: (*recording_id).to_string(),
                        point_id: point_id.clone(),
                    }
                })?;
                matched += 1;
                row.matched_by_point[predicted_index] += 1;
                if point_id == &truth_item.expected_point_id {
                    correct += 1;
                }
                let expected = point_by_id
                    .get(&truth_item.expected_point_id)
                    .expect("expected point was validated");
                let predicted = point_by_id
                    .get(point_id)
                    .expect("predicted point was validated");
                floor_errors.push(floor_distance_m(
                    expected.coordinates_m,
                    predicted.coordinates_m,
                ));
            }
            CapturePredictionStatus::Unknown => {
                unknown += 1;
                row.unknown += 1;
            }
            CapturePredictionStatus::Ambiguous => {
                ambiguous += 1;
                row.ambiguous += 1;
            }
            CapturePredictionStatus::InsufficientEvidence => {
                insufficient_evidence += 1;
                row.insufficient_evidence += 1;
            }
        }
    }

    floor_errors.sort_by(f64::total_cmp);
    let total = truth.items.len() as u64;
    let abstentions = unknown + ambiguous + insufficient_evidence;
    let coverage = ratio(matched, total);
    let accuracy_all = ratio(correct, total);
    let accuracy_decided = (matched > 0).then(|| ratio(correct, matched));
    let median_floor_error_m = median(&floor_errors);
    let p95_floor_error_m = nearest_rank_percentile(&floor_errors, 0.95);
    let max_floor_error_m = floor_errors.last().copied();
    let points_without_correct_repetition: Vec<String> = confusion_rows
        .iter()
        .enumerate()
        .filter(|(index, row)| row.matched_by_point[*index] == 0)
        .map(|(_, row)| row.expected_point_id.clone())
        .collect();
    let maximum_wrong_floor_error_m = floor_errors
        .iter()
        .copied()
        .filter(|error| *error > 0.0)
        .max_by(f64::total_cmp);
    let acceptance_gates = PositionAcceptanceGates {
        exactly_eighteen_blind_captures: total == REQUIRED_BLIND_CAPTURES,
        coverage_at_least_sixteen_of_eighteen: matched >= MINIMUM_MATCHED_CAPTURES
            && coverage >= ratio(MINIMUM_MATCHED_CAPTURES, REQUIRED_BLIND_CAPTURES),
        accuracy_all_at_least_fifteen_of_eighteen: correct >= MINIMUM_CORRECT_CAPTURES
            && accuracy_all >= ratio(MINIMUM_CORRECT_CAPTURES, REQUIRED_BLIND_CAPTURES),
        accuracy_decided_at_least_ninety_percent: accuracy_decided
            .is_some_and(|accuracy| accuracy >= MINIMUM_DECIDED_ACCURACY),
        every_point_has_correct_repetition: points_without_correct_repetition.is_empty(),
        points_without_correct_repetition,
        abstentions_at_most_two: abstentions <= MAXIMUM_ABSTENTIONS,
        median_floor_error_is_zero: median_floor_error_m == Some(REQUIRED_MEDIAN_FLOOR_ERROR_M),
        p95_floor_error_at_most_1_30_m: p95_floor_error_m
            .is_some_and(|error| error <= MAXIMUM_FLOOR_ERROR_M),
        maximum_wrong_floor_error_at_most_1_30_m: maximum_wrong_floor_error_m
            .is_none_or(|error| error <= MAXIMUM_FLOOR_ERROR_M),
    };
    let position_verdict_reasons = failed_acceptance_reasons(
        &acceptance_gates,
        total,
        matched,
        correct,
        accuracy_decided,
        abstentions,
        median_floor_error_m,
        p95_floor_error_m,
        maximum_wrong_floor_error_m,
    );
    let position_verdict = if position_verdict_reasons.is_empty() {
        PositionAcceptanceVerdict::Pass
    } else {
        PositionAcceptanceVerdict::Fail
    };
    Ok(PositionEvaluationReport {
        schema_version: EVALUATION_REPORT_SCHEMA_VERSION,
        kind: EVALUATION_REPORT_KIND.to_string(),
        algorithm_id: predictions.algorithm_id.clone(),
        predictions_sha256: prediction_file_sha256.to_string(),
        index_sha256: predictions.index_sha256.clone(),
        setup_sha256: predictions.setup_sha256.clone(),
        total,
        matched,
        correct,
        unknown,
        ambiguous,
        insufficient_evidence,
        abstentions,
        coverage,
        accuracy_all,
        accuracy_decided,
        median_floor_error_m,
        p95_floor_error_m,
        max_floor_error_m,
        position_verdict,
        position_verdict_reasons,
        acceptance_gates,
        confusion: PositionConfusionMatrix {
            predicted_point_ids: point_ids,
            rows: confusion_rows,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn failed_acceptance_reasons(
    gates: &PositionAcceptanceGates,
    total: u64,
    matched: u64,
    correct: u64,
    accuracy_decided: Option<f64>,
    abstentions: u64,
    median_floor_error_m: Option<f64>,
    p95_floor_error_m: Option<f64>,
    maximum_wrong_floor_error_m: Option<f64>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if !gates.exactly_eighteen_blind_captures {
        reasons.push(format!(
            "expected exactly {REQUIRED_BLIND_CAPTURES} blind captures, got {total}"
        ));
    }
    if !gates.coverage_at_least_sixteen_of_eighteen {
        reasons.push(format!(
            "coverage requires at least {MINIMUM_MATCHED_CAPTURES}/{REQUIRED_BLIND_CAPTURES} matched captures, got {matched}/{total}"
        ));
    }
    if !gates.accuracy_all_at_least_fifteen_of_eighteen {
        reasons.push(format!(
            "accuracy_all requires at least {MINIMUM_CORRECT_CAPTURES}/{REQUIRED_BLIND_CAPTURES} correct captures, got {correct}/{total}"
        ));
    }
    if !gates.accuracy_decided_at_least_ninety_percent {
        reasons.push(match accuracy_decided {
            Some(accuracy) => format!(
                "accuracy_decided requires at least {MINIMUM_DECIDED_ACCURACY:.2}, got {accuracy:.6}"
            ),
            None => "accuracy_decided is unavailable because no capture was matched".to_string(),
        });
    }
    if !gates.every_point_has_correct_repetition {
        reasons.push(format!(
            "no correct repetition for {}",
            gates.points_without_correct_repetition.join(",")
        ));
    }
    if !gates.abstentions_at_most_two {
        reasons.push(format!(
            "at most {MAXIMUM_ABSTENTIONS} abstentions are allowed, got {abstentions}"
        ));
    }
    if !gates.median_floor_error_is_zero {
        reasons.push(format!(
            "median floor error must be {REQUIRED_MEDIAN_FLOOR_ERROR_M:.1} m, got {}",
            optional_metric(median_floor_error_m)
        ));
    }
    if !gates.p95_floor_error_at_most_1_30_m {
        reasons.push(format!(
            "p95 floor error must be at most {MAXIMUM_FLOOR_ERROR_M:.2} m, got {}",
            optional_metric(p95_floor_error_m)
        ));
    }
    if !gates.maximum_wrong_floor_error_at_most_1_30_m {
        reasons.push(format!(
            "maximum individual wrong-point floor error must be at most {MAXIMUM_FLOOR_ERROR_M:.2} m, got {}",
            optional_metric(maximum_wrong_floor_error_m)
        ));
    }
    reasons
}

fn optional_metric(value: Option<f64>) -> String {
    value.map_or_else(
        || "unavailable".to_string(),
        |value| format!("{value:.6} m"),
    )
}

fn validate_prediction_artifact(
    artifact: &PositionPredictionArtifact,
) -> Result<(), PositionEvaluationError> {
    if artifact.schema_version != PREDICTION_ARTIFACT_SCHEMA_VERSION {
        return Err(PositionEvaluationError::UnsupportedPredictionSchema {
            expected: PREDICTION_ARTIFACT_SCHEMA_VERSION,
            actual: artifact.schema_version,
        });
    }
    if artifact.kind != PREDICTION_ARTIFACT_KIND {
        return Err(PositionEvaluationError::UnexpectedPredictionKind {
            actual: artifact.kind.clone(),
        });
    }
    if artifact.algorithm_id.trim().is_empty() {
        return Err(PositionEvaluationError::EmptyAlgorithmId);
    }
    validate_sha256("predictions.index_sha256", &artifact.index_sha256)?;
    validate_sha256("predictions.setup_sha256", &artifact.setup_sha256)?;
    let reference_points = canonical_reference_points(&artifact.reference_points)?;
    let valid_point_ids: BTreeSet<&str> = reference_points
        .iter()
        .map(|point| point.id.as_str())
        .collect();
    if artifact.captures.is_empty() {
        return Err(PositionEvaluationError::EmptyPredictions);
    }

    let mut recording_ids = BTreeSet::new();
    let mut raw_hashes = BTreeSet::new();
    let mut metadata_hashes = BTreeSet::new();
    let mut signal_hashes = BTreeSet::new();
    for capture in &artifact.captures {
        if capture.recording_id.trim().is_empty() {
            return Err(PositionEvaluationError::EmptyRecordingId {
                source: "prediction",
            });
        }
        if !recording_ids.insert(capture.recording_id.as_str()) {
            return Err(PositionEvaluationError::DuplicatePredictionRecordingId {
                recording_id: capture.recording_id.clone(),
            });
        }
        validate_sha256("predictions.captures[].raw_sha256", &capture.raw_sha256)?;
        if !raw_hashes.insert(capture.raw_sha256.as_str()) {
            return Err(PositionEvaluationError::DuplicatePredictionRawSha256 {
                raw_sha256: capture.raw_sha256.clone(),
            });
        }
        validate_sha256(
            "predictions.captures[].metadata_sha256",
            &capture.metadata_sha256,
        )?;
        if !metadata_hashes.insert(capture.metadata_sha256.as_str()) {
            return Err(PositionEvaluationError::DuplicatePredictionMetadataSha256 {
                metadata_sha256: capture.metadata_sha256.clone(),
            });
        }
        validate_sha256(
            "predictions.captures[].signal_sha256",
            &capture.signal_sha256,
        )?;
        if !signal_hashes.insert(capture.signal_sha256.as_str()) {
            return Err(PositionEvaluationError::DuplicatePredictionSignalSha256 {
                signal_sha256: capture.signal_sha256.clone(),
            });
        }
        if let CapturePredictionStatus::Matched { point_id } = &capture.prediction {
            if point_id.trim().is_empty() {
                return Err(PositionEvaluationError::EmptyPredictedPointId {
                    recording_id: capture.recording_id.clone(),
                });
            }
            if !valid_point_ids.contains(point_id.as_str()) {
                return Err(PositionEvaluationError::UnknownPredictedPointId {
                    recording_id: capture.recording_id.clone(),
                    point_id: point_id.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_truth_manifest(
    manifest: &PositionTruthManifest,
) -> Result<(), PositionEvaluationError> {
    if manifest.schema_version != TRUTH_MANIFEST_SCHEMA_VERSION {
        return Err(PositionEvaluationError::UnsupportedTruthSchema {
            expected: TRUTH_MANIFEST_SCHEMA_VERSION,
            actual: manifest.schema_version,
        });
    }
    if manifest.kind != TRUTH_MANIFEST_KIND {
        return Err(PositionEvaluationError::UnexpectedTruthKind {
            actual: manifest.kind.clone(),
        });
    }
    validate_sha256("truth.predictions_sha256", &manifest.predictions_sha256)?;
    validate_sha256("truth.index_sha256", &manifest.index_sha256)?;
    validate_sha256("truth.setup_sha256", &manifest.setup_sha256)?;
    if manifest.items.is_empty() {
        return Err(PositionEvaluationError::EmptyTruth);
    }

    let mut recording_ids = BTreeSet::new();
    let mut raw_hashes = BTreeSet::new();
    let mut metadata_hashes = BTreeSet::new();
    let mut signal_hashes = BTreeSet::new();
    for item in &manifest.items {
        if item.recording_id.trim().is_empty() {
            return Err(PositionEvaluationError::EmptyRecordingId { source: "truth" });
        }
        if !recording_ids.insert(item.recording_id.as_str()) {
            return Err(PositionEvaluationError::DuplicateTruthRecordingId {
                recording_id: item.recording_id.clone(),
            });
        }
        validate_sha256("truth.items[].raw_sha256", &item.raw_sha256)?;
        if !raw_hashes.insert(item.raw_sha256.as_str()) {
            return Err(PositionEvaluationError::DuplicateTruthRawSha256 {
                raw_sha256: item.raw_sha256.clone(),
            });
        }
        validate_sha256("truth.items[].metadata_sha256", &item.metadata_sha256)?;
        if !metadata_hashes.insert(item.metadata_sha256.as_str()) {
            return Err(PositionEvaluationError::DuplicateTruthMetadataSha256 {
                metadata_sha256: item.metadata_sha256.clone(),
            });
        }
        validate_sha256("truth.items[].signal_sha256", &item.signal_sha256)?;
        if !signal_hashes.insert(item.signal_sha256.as_str()) {
            return Err(PositionEvaluationError::DuplicateTruthSignalSha256 {
                signal_sha256: item.signal_sha256.clone(),
            });
        }
        if item.expected_point_id.trim().is_empty() {
            return Err(PositionEvaluationError::EmptyExpectedPointId {
                recording_id: item.recording_id.clone(),
            });
        }
    }
    Ok(())
}

fn canonical_reference_points(
    points: &[FingerprintPosition],
) -> Result<Vec<FingerprintPosition>, PositionEvaluationError> {
    if points.len() != POSITION_COUNT {
        return Err(PositionEvaluationError::ReferencePointCount {
            expected: POSITION_COUNT,
            actual: points.len(),
        });
    }
    let mut points = points.to_vec();
    points.sort_by(|left, right| left.id.cmp(&right.id));
    for (index, point) in points.iter().enumerate() {
        if point.id.trim().is_empty() {
            return Err(PositionEvaluationError::EmptyPointId);
        }
        if index > 0 && points[index - 1].id == point.id {
            return Err(PositionEvaluationError::DuplicatePointId {
                point_id: point.id.clone(),
            });
        }
        for (coordinate_index, coordinate) in point.coordinates_m.iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(PositionEvaluationError::NonFinitePointCoordinate {
                    point_id: point.id.clone(),
                    coordinate_index,
                });
            }
        }
    }
    for (index, first) in points.iter().enumerate() {
        for second in &points[index + 1..] {
            if first.coordinates_m[0] == second.coordinates_m[0]
                && first.coordinates_m[2] == second.coordinates_m[2]
            {
                return Err(PositionEvaluationError::DuplicatePointCoordinates {
                    first_id: first.id.clone(),
                    second_id: second.id.clone(),
                });
            }
        }
    }
    Ok(points)
}

fn validate_sha256(field: &str, value: &str) -> Result<(), PositionEvaluationError> {
    let valid = value.len() == SHA256_HEX_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(PositionEvaluationError::InvalidSha256 {
            field: field.to_string(),
        })
    }
}

fn floor_distance_m(left: [f64; 3], right: [f64; 3]) -> f64 {
    let x = left[0] - right[0];
    let z = left[2] - right[2];
    (x * x + z * z).sqrt()
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn median(sorted_values: &[f64]) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let middle = sorted_values.len() / 2;
    Some(if sorted_values.len() % 2 == 0 {
        (sorted_values[middle - 1] + sorted_values[middle]) / 2.0
    } else {
        sorted_values[middle]
    })
}

fn nearest_rank_percentile(sorted_values: &[f64], quantile: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let rank = (quantile * sorted_values.len() as f64).ceil() as usize;
    Some(sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALGORITHM_ID: &str = "position_fingerprint_exact_v1";

    fn sha256(character: char) -> String {
        std::iter::repeat_n(character, SHA256_HEX_LENGTH).collect()
    }

    fn provenance_hashes(key: char) -> (String, String, String) {
        let value = key.to_digit(16).expect("test provenance key must be hex") as u64;
        (
            format!("{value:064x}"),
            format!("{:064x}", value + 16),
            format!("{:064x}", value + 32),
        )
    }

    fn reference_points() -> Vec<FingerprintPosition> {
        let columns = [0.75, 2.01, 3.27];
        let rows = [0.75, 1.72, 2.69];
        (0..POSITION_COUNT)
            .map(|index| FingerprintPosition {
                id: format!("P{:02}", index + 1),
                coordinates_m: [columns[index % 3], 0.0, rows[index / 3]],
            })
            .collect()
    }

    fn prediction(
        recording_id: &str,
        hash_character: char,
        status: CapturePredictionStatus,
    ) -> CapturePrediction {
        let (raw_sha256, metadata_sha256, signal_sha256) = provenance_hashes(hash_character);
        CapturePrediction::new(
            recording_id,
            raw_sha256,
            metadata_sha256,
            signal_sha256,
            status,
        )
    }

    fn truth_item(
        recording_id: &str,
        hash_character: char,
        expected_point_id: &str,
    ) -> PositionTruthItem {
        let (raw_sha256, metadata_sha256, signal_sha256) = provenance_hashes(hash_character);
        PositionTruthItem::new(
            recording_id,
            raw_sha256,
            metadata_sha256,
            signal_sha256,
            expected_point_id,
        )
    }

    fn prediction_artifact(captures: Vec<CapturePrediction>) -> PositionPredictionArtifact {
        PositionPredictionArtifact::new(
            ALGORITHM_ID,
            sha256('a'),
            sha256('b'),
            reference_points(),
            captures,
        )
        .unwrap()
    }

    fn truth_manifest(items: Vec<PositionTruthItem>) -> PositionTruthManifest {
        PositionTruthManifest::new(sha256('c'), sha256('a'), sha256('b'), items).unwrap()
    }

    fn blind_provenance_hashes(run: usize) -> (String, String, String) {
        let run = run as u64;
        (
            format!("{:064x}", 1_000 + run),
            format!("{:064x}", 2_000 + run),
            format!("{:064x}", 3_000 + run),
        )
    }

    fn passing_blind_inputs() -> (PositionPredictionArtifact, PositionTruthManifest) {
        let mut captures = Vec::new();
        let mut truth_items = Vec::new();
        for point_index in 0..POSITION_COUNT {
            let point_id = format!("P{:02}", point_index + 1);
            for repetition in 0..2 {
                let run = point_index * 2 + repetition + 1;
                let recording_id = format!("blind-{run:02}");
                let (raw_sha256, metadata_sha256, signal_sha256) = blind_provenance_hashes(run);
                captures.push(CapturePrediction::new(
                    &recording_id,
                    &raw_sha256,
                    &metadata_sha256,
                    &signal_sha256,
                    CapturePredictionStatus::Matched {
                        point_id: point_id.clone(),
                    },
                ));
                truth_items.push(PositionTruthItem::new(
                    recording_id,
                    raw_sha256,
                    metadata_sha256,
                    signal_sha256,
                    &point_id,
                ));
            }
        }
        (prediction_artifact(captures), truth_manifest(truth_items))
    }

    fn set_prediction(
        predictions: &mut PositionPredictionArtifact,
        recording_id: &str,
        status: CapturePredictionStatus,
    ) {
        predictions
            .captures
            .iter_mut()
            .find(|capture| capture.recording_id == recording_id)
            .expect("blind capture exists")
            .prediction = status;
    }

    fn adjacent_wrong_point(point_index: usize) -> String {
        let adjacent_index = if point_index % 3 < 2 {
            point_index + 1
        } else {
            point_index - 1
        };
        format!("P{:02}", adjacent_index + 1)
    }

    #[test]
    fn frozen_blind_position_gates_produce_an_explicit_pass() {
        let (predictions, truth) = passing_blind_inputs();

        let report = evaluate(&predictions, &sha256('c'), &truth).unwrap();

        assert_eq!(report.schema_version, EVALUATION_REPORT_SCHEMA_VERSION);
        assert_eq!(report.total, 18);
        assert_eq!(report.matched, 18);
        assert_eq!(report.correct, 18);
        assert_eq!(report.abstentions, 0);
        assert_eq!(report.max_floor_error_m, Some(0.0));
        assert_eq!(report.position_verdict, PositionAcceptanceVerdict::Pass);
        assert!(report.position_verdict_reasons.is_empty());
        assert_eq!(
            report.acceptance_gates,
            PositionAcceptanceGates {
                exactly_eighteen_blind_captures: true,
                coverage_at_least_sixteen_of_eighteen: true,
                accuracy_all_at_least_fifteen_of_eighteen: true,
                accuracy_decided_at_least_ninety_percent: true,
                every_point_has_correct_repetition: true,
                points_without_correct_repetition: Vec::new(),
                abstentions_at_most_two: true,
                median_floor_error_is_zero: true,
                p95_floor_error_at_most_1_30_m: true,
                maximum_wrong_floor_error_at_most_1_30_m: true,
            }
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["position_verdict"], "PASS");
    }

    #[test]
    fn capture_count_coverage_and_abstention_gates_fail_closed() {
        let (mut predictions, mut truth) = passing_blind_inputs();
        predictions.captures.pop();
        truth.items.pop();
        let seventeen = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert_eq!(seventeen.position_verdict, PositionAcceptanceVerdict::Fail);
        assert!(!seventeen.acceptance_gates.exactly_eighteen_blind_captures);
        assert!(
            seventeen
                .acceptance_gates
                .every_point_has_correct_repetition
        );

        let (mut predictions, truth) = passing_blind_inputs();
        for recording_id in ["blind-01", "blind-03", "blind-05"] {
            set_prediction(
                &mut predictions,
                recording_id,
                CapturePredictionStatus::Unknown,
            );
        }
        let abstaining = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert_eq!(abstaining.abstentions, 3);
        assert!(
            !abstaining
                .acceptance_gates
                .coverage_at_least_sixteen_of_eighteen
        );
        assert!(!abstaining.acceptance_gates.abstentions_at_most_two);
        assert!(
            abstaining
                .acceptance_gates
                .accuracy_all_at_least_fifteen_of_eighteen
        );
    }

    #[test]
    fn both_accuracy_gates_and_each_point_repetition_gate_are_enforced() {
        let (mut predictions, truth) = passing_blind_inputs();
        for (recording_id, expected_point_index) in [
            ("blind-01", 0),
            ("blind-03", 1),
            ("blind-05", 2),
            ("blind-07", 3),
        ] {
            set_prediction(
                &mut predictions,
                recording_id,
                CapturePredictionStatus::Matched {
                    point_id: adjacent_wrong_point(expected_point_index),
                },
            );
        }
        let inaccurate = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert!(
            !inaccurate
                .acceptance_gates
                .accuracy_all_at_least_fifteen_of_eighteen
        );
        assert!(
            !inaccurate
                .acceptance_gates
                .accuracy_decided_at_least_ninety_percent
        );

        let (mut predictions, truth) = passing_blind_inputs();
        for (recording_id, point_id) in [("blind-01", "P02"), ("blind-03", "P01")] {
            set_prediction(
                &mut predictions,
                recording_id,
                CapturePredictionStatus::Matched {
                    point_id: point_id.to_string(),
                },
            );
        }
        set_prediction(
            &mut predictions,
            "blind-05",
            CapturePredictionStatus::Unknown,
        );
        let decided_accuracy = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert!(
            decided_accuracy
                .acceptance_gates
                .accuracy_all_at_least_fifteen_of_eighteen
        );
        assert!(
            !decided_accuracy
                .acceptance_gates
                .accuracy_decided_at_least_ninety_percent
        );

        let (mut predictions, truth) = passing_blind_inputs();
        for recording_id in ["blind-01", "blind-02"] {
            set_prediction(
                &mut predictions,
                recording_id,
                CapturePredictionStatus::Matched {
                    point_id: "P02".to_string(),
                },
            );
        }
        let missing_point = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert!(
            !missing_point
                .acceptance_gates
                .every_point_has_correct_repetition
        );
        assert_eq!(
            missing_point
                .acceptance_gates
                .points_without_correct_repetition,
            ["P01"]
        );
        assert!(missing_point
            .position_verdict_reasons
            .iter()
            .any(|reason| reason.contains("P01")));
    }

    #[test]
    fn median_p95_and_maximum_wrong_error_gates_are_enforced_and_reported() {
        let (mut predictions, truth) = passing_blind_inputs();
        for run in 1..=10 {
            let expected_point_index = (run - 1) / 2;
            set_prediction(
                &mut predictions,
                &format!("blind-{run:02}"),
                CapturePredictionStatus::Matched {
                    point_id: adjacent_wrong_point(expected_point_index),
                },
            );
        }
        let nonzero_median = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert!(!nonzero_median.acceptance_gates.median_floor_error_is_zero);
        assert!(
            nonzero_median
                .acceptance_gates
                .p95_floor_error_at_most_1_30_m
        );
        assert!(
            nonzero_median
                .acceptance_gates
                .maximum_wrong_floor_error_at_most_1_30_m
        );

        let (mut predictions, truth) = passing_blind_inputs();
        set_prediction(
            &mut predictions,
            "blind-01",
            CapturePredictionStatus::Matched {
                point_id: "P09".to_string(),
            },
        );
        let excessive_error = evaluate(&predictions, &sha256('c'), &truth).unwrap();
        assert!(excessive_error
            .max_floor_error_m
            .is_some_and(|error| error > 1.30));
        assert!(
            !excessive_error
                .acceptance_gates
                .p95_floor_error_at_most_1_30_m
        );
        assert!(
            !excessive_error
                .acceptance_gates
                .maximum_wrong_floor_error_at_most_1_30_m
        );
        assert_eq!(
            excessive_error.position_verdict,
            PositionAcceptanceVerdict::Fail
        );
        assert!(excessive_error
            .position_verdict_reasons
            .iter()
            .any(|reason| reason.contains("maximum individual wrong-point")));
    }

    #[test]
    fn correct_predictions_produce_a_complete_report() {
        let predictions = prediction_artifact(vec![
            prediction(
                "capture-01",
                '1',
                CapturePredictionStatus::Matched {
                    point_id: "P01".to_string(),
                },
            ),
            prediction(
                "capture-05",
                '5',
                CapturePredictionStatus::Matched {
                    point_id: "P05".to_string(),
                },
            ),
            prediction(
                "capture-09",
                '9',
                CapturePredictionStatus::Matched {
                    point_id: "P09".to_string(),
                },
            ),
        ]);
        let truth = truth_manifest(vec![
            truth_item("capture-01", '1', "P01"),
            truth_item("capture-05", '5', "P05"),
            truth_item("capture-09", '9', "P09"),
        ]);

        let report = evaluate(&predictions, &sha256('c'), &truth).unwrap();

        assert_eq!(report.total, 3);
        assert_eq!(report.matched, 3);
        assert_eq!(report.correct, 3);
        assert_eq!(report.coverage, 1.0);
        assert_eq!(report.accuracy_all, 1.0);
        assert_eq!(report.accuracy_decided, Some(1.0));
        assert_eq!(report.median_floor_error_m, Some(0.0));
        assert_eq!(report.p95_floor_error_m, Some(0.0));
        assert_eq!(report.confusion.rows[0].matched_by_point[0], 1);
        assert_eq!(report.confusion.rows[4].matched_by_point[4], 1);
        assert_eq!(report.confusion.rows[8].matched_by_point[8], 1);
    }

    #[test]
    fn missing_and_extra_truth_items_are_rejected() {
        let predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);

        let missing_truth = truth_manifest(vec![truth_item("capture-02", '2', "P02")]);
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &missing_truth),
            Err(PositionEvaluationError::MissingTruthForPrediction { recording_id })
                if recording_id == "capture-01"
        ));

        let predictions_without_second = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        let extra_truth = truth_manifest(vec![
            truth_item("capture-01", '1', "P01"),
            truth_item("capture-02", '2', "P02"),
        ]);
        assert!(matches!(
            evaluate(&predictions_without_second, &sha256('c'), &extra_truth),
            Err(PositionEvaluationError::MissingPredictionForTruth { recording_id })
                if recording_id == "capture-02"
        ));
    }

    #[test]
    fn duplicate_recording_ids_and_raw_hashes_are_rejected() {
        let mut duplicate_predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        duplicate_predictions.captures.push(prediction(
            "capture-01",
            '2',
            CapturePredictionStatus::Unknown,
        ));
        let truth = truth_manifest(vec![truth_item("capture-01", '1', "P01")]);
        assert!(matches!(
            evaluate(&duplicate_predictions, &sha256('c'), &truth),
            Err(PositionEvaluationError::DuplicatePredictionRecordingId { .. })
        ));

        let predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        let mut duplicate_truth = truth_manifest(vec![truth_item("capture-01", '1', "P01")]);
        duplicate_truth
            .items
            .push(truth_item("capture-02", '1', "P02"));
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &duplicate_truth),
            Err(PositionEvaluationError::DuplicateTruthRawSha256 { .. })
        ));
    }

    #[test]
    fn duplicate_metadata_and_signal_hashes_are_rejected() {
        let predictions = prediction_artifact(vec![
            prediction("capture-01", '1', CapturePredictionStatus::Unknown),
            prediction("capture-02", '2', CapturePredictionStatus::Unknown),
        ]);
        let truth = truth_manifest(vec![
            truth_item("capture-01", '1', "P01"),
            truth_item("capture-02", '2', "P02"),
        ]);

        let mut duplicate_prediction_metadata = predictions.clone();
        duplicate_prediction_metadata.captures[1].metadata_sha256 = duplicate_prediction_metadata
            .captures[0]
            .metadata_sha256
            .clone();
        assert!(matches!(
            evaluate(&duplicate_prediction_metadata, &sha256('c'), &truth),
            Err(PositionEvaluationError::DuplicatePredictionMetadataSha256 { .. })
        ));

        let mut duplicate_prediction_signal = predictions.clone();
        duplicate_prediction_signal.captures[1].signal_sha256 = duplicate_prediction_signal
            .captures[0]
            .signal_sha256
            .clone();
        assert!(matches!(
            evaluate(&duplicate_prediction_signal, &sha256('c'), &truth),
            Err(PositionEvaluationError::DuplicatePredictionSignalSha256 { .. })
        ));

        let mut duplicate_truth_metadata = truth.clone();
        duplicate_truth_metadata.items[1].metadata_sha256 =
            duplicate_truth_metadata.items[0].metadata_sha256.clone();
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &duplicate_truth_metadata),
            Err(PositionEvaluationError::DuplicateTruthMetadataSha256 { .. })
        ));

        let mut duplicate_truth_signal = truth.clone();
        duplicate_truth_signal.items[1].signal_sha256 =
            duplicate_truth_signal.items[0].signal_sha256.clone();
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &duplicate_truth_signal),
            Err(PositionEvaluationError::DuplicateTruthSignalSha256 { .. })
        ));
    }

    #[test]
    fn all_hash_bindings_are_fail_closed() {
        let predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        let truth = truth_manifest(vec![truth_item("capture-01", '1', "P01")]);

        assert_eq!(
            evaluate(&predictions, &sha256('d'), &truth),
            Err(PositionEvaluationError::PredictionHashMismatch)
        );

        let mut wrong_index = truth.clone();
        wrong_index.index_sha256 = sha256('d');
        assert_eq!(
            evaluate(&predictions, &sha256('c'), &wrong_index),
            Err(PositionEvaluationError::IndexHashMismatch)
        );

        let mut wrong_setup = truth.clone();
        wrong_setup.setup_sha256 = sha256('d');
        assert_eq!(
            evaluate(&predictions, &sha256('c'), &wrong_setup),
            Err(PositionEvaluationError::SetupHashMismatch)
        );

        let mut wrong_capture = truth.clone();
        wrong_capture.items[0].raw_sha256 = sha256('d');
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &wrong_capture),
            Err(PositionEvaluationError::CaptureRawSha256Mismatch { recording_id })
                if recording_id == "capture-01"
        ));

        let mut wrong_metadata = truth.clone();
        wrong_metadata.items[0].metadata_sha256 = sha256('d');
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &wrong_metadata),
            Err(PositionEvaluationError::CaptureMetadataSha256Mismatch { recording_id })
                if recording_id == "capture-01"
        ));

        let mut wrong_signal = truth.clone();
        wrong_signal.items[0].signal_sha256 = sha256('d');
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &wrong_signal),
            Err(PositionEvaluationError::CaptureSignalSha256Mismatch { recording_id })
                if recording_id == "capture-01"
        ));
    }

    #[test]
    fn all_capture_provenance_hashes_require_exact_lowercase_sha256() {
        let predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        let truth = truth_manifest(vec![truth_item("capture-01", '1', "P01")]);

        let mut invalid_prediction_raw = predictions.clone();
        invalid_prediction_raw.captures[0].raw_sha256 = "A".repeat(SHA256_HEX_LENGTH);
        assert_eq!(
            evaluate(&invalid_prediction_raw, &sha256('c'), &truth),
            Err(PositionEvaluationError::InvalidSha256 {
                field: "predictions.captures[].raw_sha256".to_string()
            })
        );

        let mut invalid_prediction_metadata = predictions.clone();
        invalid_prediction_metadata.captures[0].metadata_sha256 = sha256('0')[..63].to_string();
        assert_eq!(
            evaluate(&invalid_prediction_metadata, &sha256('c'), &truth),
            Err(PositionEvaluationError::InvalidSha256 {
                field: "predictions.captures[].metadata_sha256".to_string()
            })
        );

        let mut invalid_prediction_signal = predictions.clone();
        invalid_prediction_signal.captures[0].signal_sha256 = "g".repeat(SHA256_HEX_LENGTH);
        assert_eq!(
            evaluate(&invalid_prediction_signal, &sha256('c'), &truth),
            Err(PositionEvaluationError::InvalidSha256 {
                field: "predictions.captures[].signal_sha256".to_string()
            })
        );

        let mut invalid_truth_raw = truth.clone();
        invalid_truth_raw.items[0].raw_sha256 = "A".repeat(SHA256_HEX_LENGTH);
        assert_eq!(
            evaluate(&predictions, &sha256('c'), &invalid_truth_raw),
            Err(PositionEvaluationError::InvalidSha256 {
                field: "truth.items[].raw_sha256".to_string()
            })
        );

        let mut invalid_truth_metadata = truth.clone();
        invalid_truth_metadata.items[0].metadata_sha256 = sha256('0')[..63].to_string();
        assert_eq!(
            evaluate(&predictions, &sha256('c'), &invalid_truth_metadata),
            Err(PositionEvaluationError::InvalidSha256 {
                field: "truth.items[].metadata_sha256".to_string()
            })
        );

        let mut invalid_truth_signal = truth.clone();
        invalid_truth_signal.items[0].signal_sha256 = "g".repeat(SHA256_HEX_LENGTH);
        assert_eq!(
            evaluate(&predictions, &sha256('c'), &invalid_truth_signal),
            Err(PositionEvaluationError::InvalidSha256 {
                field: "truth.items[].signal_sha256".to_string()
            })
        );
    }

    #[test]
    fn missing_capture_provenance_hashes_fail_deserialization() {
        let capture = prediction("capture-01", '1', CapturePredictionStatus::Unknown);
        for field in ["raw_sha256", "metadata_sha256", "signal_sha256"] {
            let mut value = serde_json::to_value(&capture).unwrap();
            value.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<CapturePrediction>(value).is_err(),
                "prediction without {field} must be rejected"
            );
        }

        let truth = truth_item("capture-01", '1', "P01");
        for field in ["raw_sha256", "metadata_sha256", "signal_sha256"] {
            let mut value = serde_json::to_value(&truth).unwrap();
            value.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PositionTruthItem>(value).is_err(),
                "truth without {field} must be rejected"
            );
        }
    }

    #[test]
    fn unknown_expected_and_predicted_point_ids_are_rejected() {
        let predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        let unknown_truth = truth_manifest(vec![truth_item("capture-01", '1', "P99")]);
        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &unknown_truth),
            Err(PositionEvaluationError::UnknownExpectedPointId { point_id, .. })
                if point_id == "P99"
        ));

        let mut unknown_prediction = predictions.clone();
        unknown_prediction.captures[0].prediction = CapturePredictionStatus::Matched {
            point_id: "P99".to_string(),
        };
        let truth = truth_manifest(vec![truth_item("capture-01", '1', "P01")]);
        assert!(matches!(
            evaluate(&unknown_prediction, &sha256('c'), &truth),
            Err(PositionEvaluationError::UnknownPredictedPointId { point_id, .. })
                if point_id == "P99"
        ));
    }

    #[test]
    fn abstentions_reduce_all_accuracy_and_are_separate_confusion_columns() {
        let predictions = prediction_artifact(vec![
            prediction(
                "capture-01",
                '1',
                CapturePredictionStatus::Matched {
                    point_id: "P01".to_string(),
                },
            ),
            prediction("capture-02", '2', CapturePredictionStatus::Unknown),
            prediction("capture-03", '3', CapturePredictionStatus::Ambiguous),
            prediction(
                "capture-04",
                '4',
                CapturePredictionStatus::InsufficientEvidence,
            ),
        ]);
        let truth = truth_manifest(vec![
            truth_item("capture-01", '1', "P01"),
            truth_item("capture-02", '2', "P02"),
            truth_item("capture-03", '3', "P03"),
            truth_item("capture-04", '4', "P04"),
        ]);

        let report = evaluate(&predictions, &sha256('c'), &truth).unwrap();

        assert_eq!(report.total, 4);
        assert_eq!(report.matched, 1);
        assert_eq!(report.correct, 1);
        assert_eq!(report.unknown, 1);
        assert_eq!(report.ambiguous, 1);
        assert_eq!(report.insufficient_evidence, 1);
        assert_eq!(report.coverage, 0.25);
        assert_eq!(report.accuracy_all, 0.25);
        assert_eq!(report.accuracy_decided, Some(1.0));
        assert_eq!(report.confusion.rows[1].unknown, 1);
        assert_eq!(report.confusion.rows[2].ambiguous, 1);
        assert_eq!(report.confusion.rows[3].insufficient_evidence, 1);
    }

    #[test]
    fn non_finite_reference_coordinates_are_rejected() {
        let mut predictions = prediction_artifact(vec![prediction(
            "capture-01",
            '1',
            CapturePredictionStatus::Unknown,
        )]);
        predictions.reference_points[0].coordinates_m[0] = f64::NAN;
        let truth = truth_manifest(vec![truth_item("capture-01", '1', "P01")]);

        assert!(matches!(
            evaluate(&predictions, &sha256('c'), &truth),
            Err(PositionEvaluationError::NonFinitePointCoordinate {
                point_id,
                coordinate_index: 0
            }) if point_id == "P01"
        ));
    }

    #[test]
    fn duplicate_floor_coordinates_are_rejected_even_when_height_differs() {
        let mut points = reference_points();
        let first_coordinates = points[0].coordinates_m;
        points[1].coordinates_m = [
            first_coordinates[0],
            first_coordinates[1] + 1.0,
            first_coordinates[2],
        ];

        let result = PositionPredictionArtifact::new(
            ALGORITHM_ID,
            sha256('a'),
            sha256('b'),
            points,
            vec![prediction(
                "capture-01",
                '1',
                CapturePredictionStatus::Unknown,
            )],
        );

        assert!(matches!(
            result,
            Err(PositionEvaluationError::DuplicatePointCoordinates {
                first_id,
                second_id
            }) if first_id == "P01" && second_id == "P02"
        ));
    }

    #[test]
    fn equivalent_input_orders_produce_identical_reports_and_json() {
        let ordered_predictions = vec![
            prediction(
                "capture-01",
                '1',
                CapturePredictionStatus::Matched {
                    point_id: "P02".to_string(),
                },
            ),
            prediction("capture-02", '2', CapturePredictionStatus::Unknown),
        ];
        let ordered_truth = vec![
            truth_item("capture-01", '1', "P01"),
            truth_item("capture-02", '2', "P02"),
        ];
        let first = PositionPredictionArtifact::new(
            ALGORITHM_ID,
            sha256('a'),
            sha256('b'),
            reference_points(),
            ordered_predictions.clone(),
        )
        .unwrap();
        let first_truth = PositionTruthManifest::new(
            sha256('c'),
            sha256('a'),
            sha256('b'),
            ordered_truth.clone(),
        )
        .unwrap();

        let mut reversed_points = reference_points();
        reversed_points.reverse();
        let mut reversed_predictions = ordered_predictions;
        reversed_predictions.reverse();
        let mut reversed_truth = ordered_truth;
        reversed_truth.reverse();
        let second = PositionPredictionArtifact::new(
            ALGORITHM_ID,
            sha256('a'),
            sha256('b'),
            reversed_points,
            reversed_predictions,
        )
        .unwrap();
        let second_truth =
            PositionTruthManifest::new(sha256('c'), sha256('a'), sha256('b'), reversed_truth)
                .unwrap();

        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&first_truth).unwrap(),
            serde_json::to_vec(&second_truth).unwrap()
        );

        let first_report = evaluate(&first, &sha256('c'), &first_truth).unwrap();
        let second_report = evaluate(&second, &sha256('c'), &second_truth).unwrap();
        assert_eq!(first_report, second_report);
        assert_eq!(
            serde_json::to_vec(&first_report).unwrap(),
            serde_json::to_vec(&second_report).unwrap()
        );
    }
}
