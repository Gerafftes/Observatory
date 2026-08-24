//! Sensor-independent benchmark contract for the Observatory Control Center.
//!
//! This module describes the comparison protocol and its evidence boundaries.
//! It deliberately does not manufacture scores: metrics become reportable only
//! after the existing WiFi capture, truth, and evaluation artifacts are bound.

use serde_json::{json, Value};

pub(crate) const SCHEMA_VERSION: u16 = 1;

pub(crate) fn catalog() -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "wifi_csi_benchmark_catalog",
        "status": "READY_FOR_WIFI_DATA",
        "source": "wifi_csi",
        "baseline": {
            "id": "prototype_d6",
            "label": "D6 fingerprint / prototype baseline",
            "status": "IMPLEMENTED",
            "truth": "separate_position_truth_artifact"
        },
        "comparators": [
            {"id": "knn", "label": "k-nearest neighbours", "status": "CONTRACT_READY"},
            {"id": "logistic_regression", "label": "Logistic regression", "status": "CONTRACT_READY"},
            {"id": "svm", "label": "Support vector machine", "status": "CONTRACT_READY"},
            {"id": "random_forest", "label": "Random forest", "status": "CONTRACT_READY"}
        ],
        "split": {
            "id": "sealed_wifi_train_blind_test_v1",
            "training": "P01-P09 labeled captures only",
            "test": "randomized blind captures with truth revealed after prediction",
            "grouping": "capture_session",
            "calibration_isolation": true
        },
        "metrics": [
            "accuracy",
            "coverage",
            "unknown_rate",
            "ambiguous_rate",
            "median_error_distance_m",
            "p95_error_distance_m",
            "confusion_matrix"
        ],
        "drift_scenarios": [
            {"id": "time_of_day", "label": "morning vs evening"},
            {"id": "door_state", "label": "door open vs closed"},
            {"id": "person_posture", "label": "sitting vs standing"},
            {"id": "operator_identity", "label": "different person"},
            {"id": "mac_shift", "label": "Mac moved slightly"},
            {"id": "repeated_day", "label": "24 hours later"},
            {"id": "repeated_week", "label": "7 days later"},
            {"id": "room_change", "label": "furniture changed"}
        ],
        "rx_ablation": [
            {"id": "rx1_rx2_rx3_rx4", "label": "RX1-RX4", "required_receivers": ["RX1", "RX2", "RX3", "RX4"]},
            {"id": "without_rx1", "label": "without RX1", "required_receivers": ["RX2", "RX3", "RX4"]},
            {"id": "without_rx2", "label": "without RX2", "required_receivers": ["RX1", "RX3", "RX4"]},
            {"id": "without_rx3", "label": "without RX3", "required_receivers": ["RX1", "RX2", "RX4"]},
            {"id": "without_rx4", "label": "without RX4", "required_receivers": ["RX1", "RX2", "RX3"]}
        ],
        "evidence_policy": {
            "scores_without_wifi_captures": false,
            "truth_must_be_separate_from_training": true,
            "live_position_requires_existing_acceptance_gates": true,
            "mmwave_is_not_a_model_input": true,
            "mmwave_reference_status": "INDEPENDENT_AND_UNVALIDATED"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_explicit_about_data_and_mmwave_boundaries() {
        let catalog = catalog();
        assert_eq!(catalog["status"], "READY_FOR_WIFI_DATA");
        assert_eq!(catalog["comparators"].as_array().expect("comparators").len(), 4);
        assert_eq!(catalog["rx_ablation"].as_array().expect("ablation").len(), 5);
        assert_eq!(catalog["evidence_policy"]["scores_without_wifi_captures"], false);
        assert_eq!(catalog["evidence_policy"]["mmwave_is_not_a_model_input"], true);
    }
}
