//! Fail-closed evaluation for the fixed-room D6 classification experiment.
//!
//! The replay artifact is produced without labels. Truth is opened only after
//! prediction and is cryptographically bound to the exact replay bytes and to
//! every raw, metadata, and canonical signal identity.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::raw_csi_replay::{REPLAY_REPORT_KIND, REPLAY_REPORT_SCHEMA_VERSION};

const TRUTH_SCHEMA_VERSION: u16 = 1;
pub(crate) const REPORT_SCHEMA_VERSION: u16 = 1;
const TRUTH_KIND: &str = "ruview.classification-truth";
pub(crate) const REPORT_KIND: &str = "ruview.classification-evaluation";
const EXPECTED_EMPTY_CAPTURES: usize = 3;
const EXPECTED_OCCUPIED_CAPTURES: usize = 18;
const EXPECTED_POINT_IDS: [&str; 9] = [
    "P01", "P02", "P03", "P04", "P05", "P06", "P07", "P08", "P09",
];
const MAX_AGGREGATE_EMPTY_FALSE_PRESENCE_RATE: f64 = 0.05;
const MAX_SINGLE_EMPTY_FALSE_PRESENCE_RATE: f64 = 0.10;
const MIN_CONFIRMED_OCCUPIED_CAPTURES: u64 = 16;
const MIN_OCCUPIED_RECALL: f64 = 0.80;

#[derive(Debug, Deserialize)]
struct ReplayArtifact {
    schema_version: u16,
    kind: String,
    algorithm: String,
    evaluation_hz: u16,
    warmup_seconds: u64,
    calibration: ReplayCalibration,
    measurements: Vec<ReplayMeasurement>,
}

#[derive(Debug, Deserialize)]
struct ReplayCalibration {
    capture: ReplayCapture,
}

#[derive(Debug, Deserialize)]
struct ReplayMeasurement {
    capture: ReplayCapture,
    seconds: Vec<ReplaySecond>,
}

#[derive(Debug, Deserialize)]
struct ReplayCapture {
    recording_id: String,
    label: Option<String>,
    ground_truth: Option<serde_json::Value>,
    raw_sha256: String,
    metadata_sha256: String,
    signal_sha256: String,
    setup_id: Option<String>,
    setup_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReplaySecond {
    warmup: bool,
    gap: bool,
    classification: Option<ReplayClassification>,
}

#[derive(Debug, Deserialize)]
struct ReplayClassification {
    presence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClassificationTruthItem {
    recording_id: String,
    raw_sha256: String,
    metadata_sha256: String,
    signal_sha256: String,
    expected_occupied: bool,
    expected_point_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClassificationTruthManifest {
    schema_version: u16,
    kind: String,
    predictions_sha256: String,
    setup_id: String,
    setup_sha256: String,
    calibration: ClassificationTruthItem,
    measurements: Vec<ClassificationTruthItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ClassificationCaptureEvaluation {
    pub(crate) recording_id: String,
    pub(crate) raw_sha256: String,
    pub(crate) metadata_sha256: String,
    pub(crate) signal_sha256: String,
    pub(crate) expected_occupied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected_point_id: Option<String>,
    pub(crate) evaluated_seconds: u64,
    pub(crate) presence_seconds: u64,
    pub(crate) presence_rate: f64,
    pub(crate) confirmed_presence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClassificationPointEvaluation {
    pub(crate) point_id: String,
    pub(crate) captures: u64,
    pub(crate) confirmed_presence_captures: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ClassificationEvaluationReport {
    pub(crate) schema_version: u16,
    pub(crate) kind: String,
    pub(crate) algorithm: String,
    pub(crate) predictions_sha256: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) passed: bool,
    pub(crate) failures: Vec<String>,
    pub(crate) empty_capture_count: u64,
    pub(crate) empty_evaluated_seconds: u64,
    pub(crate) empty_false_presence_seconds: u64,
    pub(crate) aggregate_empty_false_presence_rate: f64,
    pub(crate) occupied_capture_count: u64,
    pub(crate) confirmed_occupied_captures: u64,
    pub(crate) occupied_evaluated_seconds: u64,
    pub(crate) occupied_presence_seconds: u64,
    pub(crate) occupied_recall: f64,
    pub(crate) points: Vec<ClassificationPointEvaluation>,
    pub(crate) captures: Vec<ClassificationCaptureEvaluation>,
}

pub(crate) fn evaluate_files(
    predictions_path: &Path,
    truth_path: &Path,
) -> Result<ClassificationEvaluationReport, String> {
    let prediction_bytes = std::fs::read(predictions_path).map_err(|error| {
        format!(
            "could not read classification predictions {}: {error}",
            predictions_path.display()
        )
    })?;
    let truth_bytes = std::fs::read(truth_path).map_err(|error| {
        format!(
            "could not read classification truth {}: {error}",
            truth_path.display()
        )
    })?;
    evaluate_bytes(&prediction_bytes, &truth_bytes)
}

fn evaluate_bytes(
    prediction_bytes: &[u8],
    truth_bytes: &[u8],
) -> Result<ClassificationEvaluationReport, String> {
    let predictions: ReplayArtifact = serde_json::from_slice(prediction_bytes)
        .map_err(|error| format!("invalid classification prediction artifact: {error}"))?;
    let truth: ClassificationTruthManifest = serde_json::from_slice(truth_bytes)
        .map_err(|error| format!("invalid classification truth manifest: {error}"))?;
    validate_artifacts(&predictions, &truth, prediction_bytes)?;

    let truth_by_id: BTreeMap<&str, &ClassificationTruthItem> = truth
        .measurements
        .iter()
        .map(|item| (item.recording_id.as_str(), item))
        .collect();
    let mut captures = Vec::with_capacity(predictions.measurements.len());
    for measurement in &predictions.measurements {
        let truth_item = truth_by_id
            .get(measurement.capture.recording_id.as_str())
            .copied()
            .ok_or_else(|| {
                format!(
                    "missing truth for prediction {}",
                    measurement.capture.recording_id
                )
            })?;
        let evaluated: Vec<&ReplaySecond> = measurement
            .seconds
            .iter()
            .filter(|second| !second.warmup && !second.gap)
            .collect();
        if evaluated.is_empty() {
            return Err(format!(
                "{} has no post-warmup evaluated seconds",
                measurement.capture.recording_id
            ));
        }
        if evaluated
            .iter()
            .any(|second| second.classification.is_none())
        {
            return Err(format!(
                "{} has a non-gap post-warmup second without classification",
                measurement.capture.recording_id
            ));
        }
        let presence_seconds = evaluated
            .iter()
            .filter(|second| {
                second
                    .classification
                    .as_ref()
                    .is_some_and(|classification| classification.presence)
            })
            .count() as u64;
        let evaluated_seconds = evaluated.len() as u64;
        captures.push(ClassificationCaptureEvaluation {
            recording_id: truth_item.recording_id.clone(),
            raw_sha256: truth_item.raw_sha256.clone(),
            metadata_sha256: truth_item.metadata_sha256.clone(),
            signal_sha256: truth_item.signal_sha256.clone(),
            expected_occupied: truth_item.expected_occupied,
            expected_point_id: truth_item.expected_point_id.clone(),
            evaluated_seconds,
            presence_seconds,
            presence_rate: ratio(presence_seconds, evaluated_seconds),
            // A positive D6 second is already temporally confirmed by the
            // classifier. The aggregate recall gate separately requires that
            // detection is sustained across the occupied protocol.
            confirmed_presence: presence_seconds > 0,
        });
    }
    captures.sort_by(|left, right| left.recording_id.cmp(&right.recording_id));

    let empty: Vec<&ClassificationCaptureEvaluation> = captures
        .iter()
        .filter(|capture| !capture.expected_occupied)
        .collect();
    let occupied: Vec<&ClassificationCaptureEvaluation> = captures
        .iter()
        .filter(|capture| capture.expected_occupied)
        .collect();
    let empty_evaluated_seconds = empty.iter().map(|capture| capture.evaluated_seconds).sum();
    let empty_false_presence_seconds = empty.iter().map(|capture| capture.presence_seconds).sum();
    let aggregate_empty_false_presence_rate =
        ratio(empty_false_presence_seconds, empty_evaluated_seconds);
    let occupied_evaluated_seconds = occupied
        .iter()
        .map(|capture| capture.evaluated_seconds)
        .sum();
    let occupied_presence_seconds = occupied
        .iter()
        .map(|capture| capture.presence_seconds)
        .sum();
    let occupied_recall = ratio(occupied_presence_seconds, occupied_evaluated_seconds);
    let confirmed_occupied_captures = occupied
        .iter()
        .filter(|capture| capture.confirmed_presence)
        .count() as u64;

    let mut points = Vec::with_capacity(EXPECTED_POINT_IDS.len());
    for point_id in EXPECTED_POINT_IDS {
        let matching: Vec<_> = occupied
            .iter()
            .filter(|capture| capture.expected_point_id.as_deref() == Some(point_id))
            .collect();
        points.push(ClassificationPointEvaluation {
            point_id: point_id.to_string(),
            captures: matching.len() as u64,
            confirmed_presence_captures: matching
                .iter()
                .filter(|capture| capture.confirmed_presence)
                .count() as u64,
        });
    }

    let mut failures = Vec::new();
    if aggregate_empty_false_presence_rate > MAX_AGGREGATE_EMPTY_FALSE_PRESENCE_RATE {
        failures.push("aggregate_empty_false_presence_rate_above_0.05".to_string());
    }
    for capture in &empty {
        if capture.presence_rate > MAX_SINGLE_EMPTY_FALSE_PRESENCE_RATE {
            failures.push(format!(
                "empty_capture_false_presence_rate_above_0.10:{}",
                capture.recording_id
            ));
        }
    }
    if confirmed_occupied_captures < MIN_CONFIRMED_OCCUPIED_CAPTURES {
        failures.push("confirmed_occupied_captures_below_16".to_string());
    }
    if occupied_recall < MIN_OCCUPIED_RECALL {
        failures.push("occupied_recall_below_0.80".to_string());
    }
    for point in &points {
        if point.confirmed_presence_captures == 0 {
            failures.push(format!(
                "point_missed_in_both_repetitions:{}",
                point.point_id
            ));
        }
    }

    Ok(ClassificationEvaluationReport {
        schema_version: REPORT_SCHEMA_VERSION,
        kind: REPORT_KIND.to_string(),
        algorithm: predictions.algorithm,
        predictions_sha256: truth.predictions_sha256,
        setup_id: truth.setup_id,
        setup_sha256: truth.setup_sha256,
        passed: failures.is_empty(),
        failures,
        empty_capture_count: empty.len() as u64,
        empty_evaluated_seconds,
        empty_false_presence_seconds,
        aggregate_empty_false_presence_rate,
        occupied_capture_count: occupied.len() as u64,
        confirmed_occupied_captures,
        occupied_evaluated_seconds,
        occupied_presence_seconds,
        occupied_recall,
        points,
        captures,
    })
}

fn validate_artifacts(
    predictions: &ReplayArtifact,
    truth: &ClassificationTruthManifest,
    prediction_bytes: &[u8],
) -> Result<(), String> {
    if predictions.schema_version != REPLAY_REPORT_SCHEMA_VERSION {
        return Err(format!(
            "classification prediction schema {} is unsupported; expected {}",
            predictions.schema_version, REPLAY_REPORT_SCHEMA_VERSION
        ));
    }
    if predictions.kind != REPLAY_REPORT_KIND {
        return Err(format!(
            "classification prediction kind {:?} is unsupported",
            predictions.kind
        ));
    }
    if predictions.algorithm.trim().is_empty() {
        return Err("classification prediction algorithm is empty".to_string());
    }
    if predictions.evaluation_hz != 1 || predictions.warmup_seconds != 5 {
        return Err("classification prediction evaluation protocol changed".to_string());
    }
    if truth.schema_version != TRUTH_SCHEMA_VERSION || truth.kind != TRUTH_KIND {
        return Err("classification truth schema or kind is unsupported".to_string());
    }
    validate_sha256("truth.predictions_sha256", &truth.predictions_sha256)?;
    if truth.setup_id.trim().is_empty() {
        return Err("classification truth setup_id is empty".to_string());
    }
    validate_sha256("truth.setup_sha256", &truth.setup_sha256)?;
    if truth.predictions_sha256 != sha256_bytes(prediction_bytes) {
        return Err("classification truth does not bind the exact prediction bytes".to_string());
    }
    if predictions.calibration.capture.label.is_some()
        || predictions.calibration.capture.ground_truth.is_some()
    {
        return Err(
            "classification calibration prediction contains embedded label or truth".to_string(),
        );
    }
    if truth.calibration.expected_occupied || truth.calibration.expected_point_id.is_some() {
        return Err("classification calibration truth must describe an empty room".to_string());
    }
    validate_identity("truth.calibration", &truth.calibration)?;
    ensure_identity_matches(
        &predictions.calibration.capture,
        &truth.calibration,
        "calibration",
    )?;
    ensure_setup_matches(
        &predictions.calibration.capture,
        &truth.setup_id,
        &truth.setup_sha256,
        "calibration",
    )?;

    if truth.measurements.len() != EXPECTED_EMPTY_CAPTURES + EXPECTED_OCCUPIED_CAPTURES {
        return Err(format!(
            "classification truth must contain exactly {} measurements",
            EXPECTED_EMPTY_CAPTURES + EXPECTED_OCCUPIED_CAPTURES
        ));
    }
    if predictions.measurements.len() != truth.measurements.len() {
        return Err("classification prediction/truth measurement counts differ".to_string());
    }
    let empty_count = truth
        .measurements
        .iter()
        .filter(|item| !item.expected_occupied)
        .count();
    let occupied_count = truth.measurements.len() - empty_count;
    if empty_count != EXPECTED_EMPTY_CAPTURES || occupied_count != EXPECTED_OCCUPIED_CAPTURES {
        return Err(
            "classification truth must contain exactly 3 empty and 18 occupied captures"
                .to_string(),
        );
    }

    let valid_points: BTreeSet<&str> = EXPECTED_POINT_IDS.into_iter().collect();
    let mut point_counts = BTreeMap::<&str, usize>::new();
    let mut recording_ids = BTreeSet::new();
    let mut raw_hashes = BTreeSet::new();
    let mut metadata_hashes = BTreeSet::new();
    let mut signal_hashes = BTreeSet::new();
    insert_unique_identities(
        &truth.calibration,
        &mut recording_ids,
        &mut raw_hashes,
        &mut metadata_hashes,
        &mut signal_hashes,
    )?;
    let truth_by_id: BTreeMap<&str, &ClassificationTruthItem> = truth
        .measurements
        .iter()
        .map(|item| (item.recording_id.as_str(), item))
        .collect();
    if truth_by_id.len() != truth.measurements.len() {
        return Err("classification truth contains duplicate recording IDs".to_string());
    }
    for item in &truth.measurements {
        validate_identity("truth.measurements", item)?;
        insert_unique_identities(
            item,
            &mut recording_ids,
            &mut raw_hashes,
            &mut metadata_hashes,
            &mut signal_hashes,
        )?;
        match (item.expected_occupied, item.expected_point_id.as_deref()) {
            (false, None) => {}
            (false, Some(_)) => {
                return Err("empty classification truth item contains a point ID".to_string())
            }
            (true, Some(point_id)) if valid_points.contains(point_id) => {
                *point_counts.entry(point_id).or_default() += 1;
            }
            (true, Some(point_id)) => {
                return Err(format!("unknown occupied point ID {point_id:?}"))
            }
            (true, None) => {
                return Err("occupied classification truth item has no point ID".to_string())
            }
        }
    }
    for point_id in EXPECTED_POINT_IDS {
        if point_counts.get(point_id).copied() != Some(2) {
            return Err(format!(
                "classification truth must contain exactly two captures for {point_id}"
            ));
        }
    }

    let mut prediction_ids = BTreeSet::new();
    for measurement in &predictions.measurements {
        if measurement.capture.label.is_some() || measurement.capture.ground_truth.is_some() {
            return Err(format!(
                "classification prediction {} contains embedded label or truth",
                measurement.capture.recording_id
            ));
        }
        if !prediction_ids.insert(measurement.capture.recording_id.as_str()) {
            return Err("classification predictions contain duplicate recording IDs".to_string());
        }
        let item = truth_by_id
            .get(measurement.capture.recording_id.as_str())
            .copied()
            .ok_or_else(|| {
                format!(
                    "classification prediction {} has no truth item",
                    measurement.capture.recording_id
                )
            })?;
        ensure_identity_matches(&measurement.capture, item, "measurement")?;
        ensure_setup_matches(
            &measurement.capture,
            &truth.setup_id,
            &truth.setup_sha256,
            "measurement",
        )?;
    }
    if prediction_ids.len() != truth_by_id.len() {
        return Err("classification truth contains a capture without prediction".to_string());
    }
    Ok(())
}

fn validate_identity(field: &str, item: &ClassificationTruthItem) -> Result<(), String> {
    if item.recording_id.trim().is_empty() {
        return Err(format!("{field}.recording_id is empty"));
    }
    validate_sha256(&format!("{field}.raw_sha256"), &item.raw_sha256)?;
    validate_sha256(&format!("{field}.metadata_sha256"), &item.metadata_sha256)?;
    validate_sha256(&format!("{field}.signal_sha256"), &item.signal_sha256)
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

fn ensure_identity_matches(
    capture: &ReplayCapture,
    truth: &ClassificationTruthItem,
    role: &str,
) -> Result<(), String> {
    if capture.recording_id != truth.recording_id
        || capture.raw_sha256 != truth.raw_sha256
        || capture.metadata_sha256 != truth.metadata_sha256
        || capture.signal_sha256 != truth.signal_sha256
    {
        Err(format!(
            "classification {role} identity does not match truth for {}",
            capture.recording_id
        ))
    } else {
        Ok(())
    }
}

fn ensure_setup_matches(
    capture: &ReplayCapture,
    expected_id: &str,
    expected_sha256: &str,
    role: &str,
) -> Result<(), String> {
    if capture.setup_id.as_deref() != Some(expected_id)
        || capture.setup_sha256.as_deref() != Some(expected_sha256)
    {
        Err(format!(
            "classification {role} {} is not bound to the truth setup",
            capture.recording_id
        ))
    } else {
        Ok(())
    }
}

fn insert_unique_identities<'a>(
    item: &'a ClassificationTruthItem,
    recording_ids: &mut BTreeSet<&'a str>,
    raw_hashes: &mut BTreeSet<&'a str>,
    metadata_hashes: &mut BTreeSet<&'a str>,
    signal_hashes: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    if !recording_ids.insert(&item.recording_id)
        || !raw_hashes.insert(&item.raw_sha256)
        || !metadata_hashes.insert(&item.metadata_sha256)
        || !signal_hashes.insert(&item.signal_sha256)
    {
        Err("classification truth reuses a capture identity".to_string())
    } else {
        Ok(())
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(seed: usize) -> String {
        format!("{seed:064x}")
    }

    fn capture(recording_id: &str, seed: usize) -> serde_json::Value {
        serde_json::json!({
            "recording_id": recording_id,
            "label": null,
            "ground_truth": null,
            "server_version": "test",
            "started_at_unix_ns": 1,
            "ended_at_unix_ns": 2,
            "raw_sha256": hash(seed),
            "metadata_sha256": hash(seed + 100),
            "signal_sha256": hash(seed + 200),
            "setup_id": "setup-0123456789abcdef",
            "setup_sha256": hash(500),
            "frames_total": 1,
            "frames_accepted": 1,
            "frames_grid_rejected": 0
        })
    }

    fn truth_item(
        recording_id: &str,
        seed: usize,
        expected_occupied: bool,
        expected_point_id: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "recording_id": recording_id,
            "raw_sha256": hash(seed),
            "metadata_sha256": hash(seed + 100),
            "signal_sha256": hash(seed + 200),
            "expected_occupied": expected_occupied,
            "expected_point_id": expected_point_id
        })
    }

    fn seconds(presence_seconds: usize) -> Vec<serde_json::Value> {
        (0..10)
            .map(|index| {
                serde_json::json!({
                    "second_index": index,
                    "interval_start_unix_ns": index,
                    "interval_end_unix_ns": index + 1,
                    "warmup": false,
                    "frames_total": 1,
                    "frames_accepted": 1,
                    "frames_grid_rejected": 0,
                    "gap": false,
                    "sample_timestamp_unix_ns": index,
                    "classification": {
                        "motion_level": "present_still",
                        "presence": index < presence_seconds,
                        "confidence": 1.0
                    },
                    "nodes": []
                })
            })
            .collect()
    }

    fn artifacts(occupied_presence_seconds: usize) -> (Vec<u8>, Vec<u8>) {
        let mut measurements = Vec::new();
        let mut truth_items = Vec::new();
        for empty_index in 0..3 {
            let seed = empty_index + 2;
            let id = format!("empty-{empty_index}");
            measurements.push(serde_json::json!({
                "capture": capture(&id, seed),
                "seconds": seconds(0),
                "summary": {}
            }));
            truth_items.push(truth_item(&id, seed, false, None));
        }
        for point_index in 0..9 {
            for repetition in 0..2 {
                let seed = 10 + point_index * 2 + repetition;
                let id = format!("blind-{point_index}-{repetition}");
                measurements.push(serde_json::json!({
                    "capture": capture(&id, seed),
                    "seconds": seconds(occupied_presence_seconds),
                    "summary": {}
                }));
                truth_items.push(truth_item(
                    &id,
                    seed,
                    true,
                    Some(EXPECTED_POINT_IDS[point_index]),
                ));
            }
        }
        let predictions = serde_json::to_vec(&serde_json::json!({
            "schema_version": REPLAY_REPORT_SCHEMA_VERSION,
            "kind": REPLAY_REPORT_KIND,
            "algorithm": "test-algorithm",
            "evaluation_hz": 1,
            "warmup_seconds": 5,
            "geometry": {},
            "calibration": { "capture": capture("calibration", 1), "nodes": [] },
            "measurements": measurements
        }))
        .unwrap();
        let truth = serde_json::to_vec(&serde_json::json!({
            "schema_version": TRUTH_SCHEMA_VERSION,
            "kind": TRUTH_KIND,
            "predictions_sha256": sha256_bytes(&predictions),
            "setup_id": "setup-0123456789abcdef",
            "setup_sha256": hash(500),
            "calibration": truth_item("calibration", 1, false, None),
            "measurements": truth_items
        }))
        .unwrap();
        (predictions, truth)
    }

    #[test]
    fn perfect_fixed_protocol_passes() {
        let (predictions, truth) = artifacts(10);
        let report = evaluate_bytes(&predictions, &truth).unwrap();
        assert!(report.passed);
        assert!(report.failures.is_empty());
        assert_eq!(report.empty_capture_count, 3);
        assert_eq!(report.occupied_capture_count, 18);
        assert_eq!(report.confirmed_occupied_captures, 18);
        assert_eq!(report.occupied_recall, 1.0);
    }

    #[test]
    fn sustained_occupied_misses_fail_recall_gate() {
        let (predictions, truth) = artifacts(1);
        let report = evaluate_bytes(&predictions, &truth).unwrap();
        assert!(!report.passed);
        assert!(report
            .failures
            .contains(&"occupied_recall_below_0.80".to_string()));
    }

    #[test]
    fn truth_hash_mismatch_fails_closed() {
        let (predictions, truth) = artifacts(10);
        let mut value: serde_json::Value = serde_json::from_slice(&truth).unwrap();
        value["predictions_sha256"] = serde_json::Value::String(hash(999));
        let error = evaluate_bytes(&predictions, &serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("exact prediction bytes"), "{error}");
    }

    #[test]
    fn embedded_truth_fails_closed() {
        let (predictions, truth) = artifacts(10);
        let mut value: serde_json::Value = serde_json::from_slice(&predictions).unwrap();
        value["measurements"][0]["capture"]["ground_truth"] =
            serde_json::json!({"occupied": false});
        let changed = serde_json::to_vec(&value).unwrap();
        let mut truth_value: serde_json::Value = serde_json::from_slice(&truth).unwrap();
        truth_value["predictions_sha256"] = serde_json::Value::String(sha256_bytes(&changed));
        let error =
            evaluate_bytes(&changed, &serde_json::to_vec(&truth_value).unwrap()).unwrap_err();
        assert!(error.contains("embedded label or truth"), "{error}");
    }
}
