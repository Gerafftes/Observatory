//! Final composition of the independently frozen classification and position
//! verdicts. The combined artifact never reinterprets metrics; it binds the
//! exact two reports and passes only when both component verdicts pass for the
//! same sealed setup.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REPORT_SCHEMA_VERSION: u16 = 1;
const REPORT_KIND: &str = "ruview.fixed-room-experiment-evaluation";

#[derive(Debug, Deserialize)]
struct ClassificationReportSummary {
    schema_version: u16,
    kind: String,
    passed: bool,
    setup_id: String,
    setup_sha256: String,
}

#[derive(Debug, Deserialize)]
struct PositionReportSummary {
    schema_version: u16,
    kind: String,
    position_verdict: String,
    index_sha256: String,
    setup_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ExperimentVerdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExperimentEvaluationReport {
    pub(crate) schema_version: u16,
    pub(crate) kind: String,
    pub(crate) classification_report_sha256: String,
    pub(crate) position_report_sha256: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) index_sha256: String,
    pub(crate) classification_passed: bool,
    pub(crate) position_passed: bool,
    pub(crate) experiment_verdict: ExperimentVerdict,
    pub(crate) experiment_verdict_reasons: Vec<String>,
}

pub(crate) fn evaluate_files(
    classification_path: &Path,
    position_path: &Path,
) -> Result<ExperimentEvaluationReport, String> {
    let classification_bytes = std::fs::read(classification_path).map_err(|error| {
        format!(
            "could not read classification report {}: {error}",
            classification_path.display()
        )
    })?;
    let position_bytes = std::fs::read(position_path).map_err(|error| {
        format!(
            "could not read position report {}: {error}",
            position_path.display()
        )
    })?;
    evaluate_bytes(&classification_bytes, &position_bytes)
}

fn evaluate_bytes(
    classification_bytes: &[u8],
    position_bytes: &[u8],
) -> Result<ExperimentEvaluationReport, String> {
    let classification: ClassificationReportSummary = serde_json::from_slice(classification_bytes)
        .map_err(|error| format!("invalid classification evaluation report: {error}"))?;
    let position: PositionReportSummary = serde_json::from_slice(position_bytes)
        .map_err(|error| format!("invalid position evaluation report: {error}"))?;
    if classification.schema_version != super::classification_evaluation::REPORT_SCHEMA_VERSION
        || classification.kind != super::classification_evaluation::REPORT_KIND
    {
        return Err("classification evaluation report schema or kind is unsupported".to_string());
    }
    if position.schema_version != super::position_evaluation::EVALUATION_REPORT_SCHEMA_VERSION
        || position.kind != super::position_evaluation::EVALUATION_REPORT_KIND
    {
        return Err("position evaluation report schema or kind is unsupported".to_string());
    }
    validate_sha256("classification.setup_sha256", &classification.setup_sha256)?;
    validate_sha256("position.setup_sha256", &position.setup_sha256)?;
    validate_sha256("position.index_sha256", &position.index_sha256)?;
    if classification.setup_id.trim().is_empty() {
        return Err("classification setup_id is empty".to_string());
    }
    if classification.setup_sha256 != position.setup_sha256 {
        return Err("classification and position reports belong to different setups".to_string());
    }
    let position_passed = match position.position_verdict.as_str() {
        "PASS" => true,
        "FAIL" => false,
        value => return Err(format!("unsupported position verdict {value:?}")),
    };
    let mut reasons = Vec::new();
    if !classification.passed {
        reasons.push("classification_failed".to_string());
    }
    if !position_passed {
        reasons.push("position_failed".to_string());
    }
    Ok(ExperimentEvaluationReport {
        schema_version: REPORT_SCHEMA_VERSION,
        kind: REPORT_KIND.to_string(),
        classification_report_sha256: sha256_bytes(classification_bytes),
        position_report_sha256: sha256_bytes(position_bytes),
        setup_id: classification.setup_id,
        setup_sha256: classification.setup_sha256,
        index_sha256: position.index_sha256,
        classification_passed: classification.passed,
        position_passed,
        experiment_verdict: if reasons.is_empty() {
            ExperimentVerdict::Pass
        } else {
            ExperimentVerdict::Fail
        },
        experiment_verdict_reasons: reasons,
    })
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

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reports(
        classification_passed: bool,
        position_verdict: &str,
        position_setup: char,
    ) -> (Vec<u8>, Vec<u8>) {
        let classification = serde_json::to_vec(&serde_json::json!({
            "schema_version": super::super::classification_evaluation::REPORT_SCHEMA_VERSION,
            "kind": super::super::classification_evaluation::REPORT_KIND,
            "passed": classification_passed,
            "setup_id": "setup-0123456789abcdef",
            "setup_sha256": "a".repeat(64)
        }))
        .unwrap();
        let position = serde_json::to_vec(&serde_json::json!({
            "schema_version": super::super::position_evaluation::EVALUATION_REPORT_SCHEMA_VERSION,
            "kind": super::super::position_evaluation::EVALUATION_REPORT_KIND,
            "position_verdict": position_verdict,
            "index_sha256": "b".repeat(64),
            "setup_sha256": position_setup.to_string().repeat(64)
        }))
        .unwrap();
        (classification, position)
    }

    #[test]
    fn both_component_verdicts_are_required() {
        let (classification, position) = reports(true, "PASS", 'a');
        let report = evaluate_bytes(&classification, &position).unwrap();
        assert_eq!(report.experiment_verdict, ExperimentVerdict::Pass);
        assert!(report.experiment_verdict_reasons.is_empty());

        let (classification, position) = reports(false, "PASS", 'a');
        let report = evaluate_bytes(&classification, &position).unwrap();
        assert_eq!(report.experiment_verdict, ExperimentVerdict::Fail);
        assert_eq!(
            report.experiment_verdict_reasons,
            vec!["classification_failed"]
        );
    }

    #[test]
    fn setup_mismatch_fails_closed() {
        let (classification, position) = reports(true, "PASS", 'c');
        let error = evaluate_bytes(&classification, &position).unwrap_err();
        assert!(error.contains("different setups"), "{error}");
    }
}
