//! Versioned, auditable CSI training samples with separately derived radar labels.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::position_artifact::{sha256_bytes, sha256_file, write_pretty_json_no_clobber};
use super::position_capture::{PositionFeatureBlock, WINDOW_NS, WINDOW_STEP_NS};

pub(crate) const DATASET_KIND: &str = "ruview.calibration-dataset.v1";
pub(crate) const ALIGNMENT_LIMIT_MS: u64 = 150;
pub(crate) const ALIGNMENT_LIMIT_NS: u64 = ALIGNMENT_LIMIT_MS * 1_000_000;

/// Stable identity for one CSI window in one sealed calibration session.
///
/// The session identifier is intentionally part of the domain: identical
/// windows from different runs must never silently collide in a dataset.
pub(crate) fn deterministic_sample_id(
    setup_sha256: &str,
    session_id: &str,
    zone_id: &str,
    midpoint_monotonic_ns: u64,
) -> String {
    let seed = format!(
        "ruview.calibration-sample.v1:{setup_sha256}:{session_id}:{zone_id}:{midpoint_monotonic_ns}"
    );
    sha256_bytes(seed.as_bytes())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RadarObservation {
    pub(crate) host_unix_ns: u64,
    pub(crate) host_monotonic_ns: u64,
    pub(crate) clock_epoch_id: String,
    pub(crate) boot_id: u32,
    pub(crate) sequence: u32,
    pub(crate) transform_sha256: String,
    pub(crate) position_mm: [i32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReceiverSample {
    pub(crate) rx_id: u8,
    pub(crate) frame_count: usize,
    pub(crate) observed_rate_millihz: u64,
    pub(crate) maximum_gap_ns: u64,
    pub(crate) midpoint_delta_ms: u64,
    pub(crate) features: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum CalibrationDatasetRecord {
    Accepted {
        sample_id: String,
        zone_id: String,
        window_start_unix_ns: u64,
        window_end_unix_ns: u64,
        window_midpoint_monotonic_ns: u64,
        radar_position_m: [f64; 3],
        radar_before_delta_ms: u64,
        radar_after_delta_ms: u64,
        max_abs_delta_ms: u64,
        receivers: Vec<ReceiverSample>,
        quality_flags: Vec<String>,
    },
    Rejected {
        sample_id: String,
        zone_id: String,
        window_start_unix_ns: u64,
        window_end_unix_ns: u64,
        reasons: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationDatasetManifest {
    pub(crate) schema_version: u16,
    pub(crate) kind: String,
    pub(crate) validation_status: String,
    pub(crate) run_id: String,
    pub(crate) session_id: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) clock_epoch_id: String,
    pub(crate) raw_csi_sha256: String,
    pub(crate) raw_radar_sha256: String,
    pub(crate) samples_sha256: String,
    pub(crate) feature_extractor: String,
    pub(crate) window_ns: u64,
    pub(crate) window_step_ns: u64,
    pub(crate) alignment_limit_ms: u64,
    pub(crate) zone_count: usize,
    pub(crate) accepted_samples: usize,
    pub(crate) rejected_samples: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct DatasetIdentity {
    pub(crate) manifest_path: PathBuf,
    pub(crate) manifest_sha256: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_dataset(
    directory: &Path,
    run_id: &str,
    session_id: &str,
    setup_id: &str,
    setup_sha256: &str,
    clock_epoch_id: &str,
    raw_csi_sha256: String,
    raw_radar_sha256: String,
    zone_count: usize,
    records: &[CalibrationDatasetRecord],
) -> Result<DatasetIdentity, String> {
    let samples_path = directory.join(format!("{session_id}.calibration-samples.v1.jsonl"));
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
    }
    super::position_artifact::atomic_write_no_clobber(&samples_path, &bytes)
        .map_err(|error| error.to_string())?;
    let samples_sha256 = sha256_file(&samples_path).map_err(|error| error.to_string())?;
    let accepted_samples = records
        .iter()
        .filter(|record| matches!(record, CalibrationDatasetRecord::Accepted { .. }))
        .count();
    let manifest = CalibrationDatasetManifest {
        schema_version: 1,
        kind: DATASET_KIND.to_string(),
        validation_status: "UNVALIDATED".to_string(),
        run_id: run_id.to_string(),
        session_id: session_id.to_string(),
        setup_id: setup_id.to_string(),
        setup_sha256: setup_sha256.to_string(),
        clock_epoch_id: clock_epoch_id.to_string(),
        raw_csi_sha256,
        raw_radar_sha256,
        samples_sha256,
        feature_extractor: "d6-position-features-4x28-v1".to_string(),
        window_ns: WINDOW_NS,
        window_step_ns: WINDOW_STEP_NS,
        alignment_limit_ms: ALIGNMENT_LIMIT_MS,
        zone_count,
        accepted_samples,
        rejected_samples: records.len().saturating_sub(accepted_samples),
    };
    let manifest_path = directory.join(format!("{session_id}.calibration-dataset.v1.json"));
    write_pretty_json_no_clobber(&manifest_path, &manifest).map_err(|error| error.to_string())?;
    let manifest_sha256 = sha256_file(&manifest_path).map_err(|error| error.to_string())?;
    Ok(DatasetIdentity {
        manifest_path,
        manifest_sha256,
    })
}

pub(crate) fn align_record(
    sample_id: String,
    zone_id: String,
    block: &PositionFeatureBlock,
    midpoint_monotonic_ns: u64,
    receiver_midpoints_ns: &[(u8, u64)],
    radar: &[RadarObservation],
) -> CalibrationDatasetRecord {
    let reject = |reason: String| CalibrationDatasetRecord::Rejected {
        sample_id: sample_id.clone(),
        zone_id: zone_id.clone(),
        window_start_unix_ns: block.window_start_unix_ns,
        window_end_unix_ns: block.window_end_unix_ns,
        reasons: vec![reason],
    };
    let Some(before) = radar
        .iter()
        .filter(|item| item.host_monotonic_ns <= midpoint_monotonic_ns)
        .max_by_key(|item| item.host_monotonic_ns)
    else {
        return reject("missing_radar_before_midpoint".to_string());
    };
    let Some(after) = radar
        .iter()
        .filter(|item| item.host_monotonic_ns >= midpoint_monotonic_ns)
        .min_by_key(|item| item.host_monotonic_ns)
    else {
        return reject("missing_radar_after_midpoint".to_string());
    };
    if before.clock_epoch_id != after.clock_epoch_id
        || before.boot_id != after.boot_id
        || before.transform_sha256 != after.transform_sha256
    {
        return reject("radar_epoch_boot_or_transform_changed".to_string());
    }
    let before_delta = midpoint_monotonic_ns.abs_diff(before.host_monotonic_ns);
    let after_delta = midpoint_monotonic_ns.abs_diff(after.host_monotonic_ns);
    if before_delta > ALIGNMENT_LIMIT_NS || after_delta > ALIGNMENT_LIMIT_NS {
        return reject("radar_alignment_limit_exceeded".to_string());
    }
    let span = after.host_monotonic_ns.saturating_sub(before.host_monotonic_ns);
    let numerator = midpoint_monotonic_ns.saturating_sub(before.host_monotonic_ns) as f64;
    let ratio = if span == 0 { 0.0 } else { numerator / span as f64 };
    let interpolate = |axis: usize| {
        f64::from(before.position_mm[axis])
            + ratio * f64::from(after.position_mm[axis] - before.position_mm[axis])
    };
    let mut max_delta = before_delta.max(after_delta);
    let mut receivers = Vec::with_capacity(block.receivers.len());
    for receiver in &block.receivers {
        let Some((_, midpoint)) = receiver_midpoints_ns
            .iter()
            .find(|(rx_id, _)| *rx_id == receiver.rx_id)
        else {
            return reject(format!("missing_rx{}_midpoint", receiver.rx_id));
        };
        let delta = midpoint_monotonic_ns.abs_diff(*midpoint);
        max_delta = max_delta.max(delta);
        if delta > ALIGNMENT_LIMIT_NS {
            return reject(format!("rx{}_alignment_limit_exceeded", receiver.rx_id));
        }
        receivers.push(ReceiverSample {
            rx_id: receiver.rx_id,
            frame_count: receiver.frame_count,
            observed_rate_millihz: receiver.observed_rate_millihz,
            maximum_gap_ns: receiver.maximum_gap_ns,
            midpoint_delta_ms: delta / 1_000_000,
            features: receiver.features.to_vec(),
        });
    }
    CalibrationDatasetRecord::Accepted {
        sample_id,
        zone_id,
        window_start_unix_ns: block.window_start_unix_ns,
        window_end_unix_ns: block.window_end_unix_ns,
        window_midpoint_monotonic_ns: midpoint_monotonic_ns,
        radar_position_m: [interpolate(0) / 1000.0, 0.0, interpolate(1) / 1000.0],
        radar_before_delta_ms: before_delta / 1_000_000,
        radar_after_delta_ms: after_delta / 1_000_000,
        max_abs_delta_ms: max_delta / 1_000_000,
        receivers,
        quality_flags: vec!["single_target".to_string(), "aligned".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position_capture::{PositionGridIdentity, RxPositionFeatures, POSITION_FEATURE_COUNT};

    fn block() -> PositionFeatureBlock {
        PositionFeatureBlock {
            window_start_unix_ns: 10,
            window_end_unix_ns: 3_000_000_010,
            common_coverage_ns: 2_900_000_000,
            receivers: (1..=4)
                .map(|rx_id| RxPositionFeatures {
                    rx_id,
                    grid: PositionGridIdentity {
                        center_frequency_mhz: 2437,
                        antenna_count: 1,
                        subcarrier_count: 64,
                        ppdu_type: 0,
                        layout_flags: 0,
                    },
                    frame_count: 20,
                    observed_rate_millihz: 6_000,
                    coverage_ns: 2_900_000_000,
                    maximum_gap_ns: 100_000_000,
                    features: [1.0; POSITION_FEATURE_COUNT],
                })
                .collect(),
        }
    }

    fn radar(at: u64, x: i32) -> RadarObservation {
        RadarObservation {
            host_unix_ns: at,
            host_monotonic_ns: at,
            clock_epoch_id: "epoch".to_string(),
            boot_id: 1,
            sequence: at as u32,
            transform_sha256: "a".repeat(64),
            position_mm: [x, 500],
        }
    }

    #[test]
    fn accepts_exact_alignment_boundary_and_interpolates() {
        let record = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[radar(850_000_000, 1000), radar(1_150_000_000, 2000)],
        );
        match record {
            CalibrationDatasetRecord::Accepted {
                radar_position_m,
                max_abs_delta_ms,
                ..
            } => {
                assert_eq!(radar_position_m, [1.5, 0.0, 0.5]);
                assert_eq!(max_abs_delta_ms, ALIGNMENT_LIMIT_MS);
            }
            CalibrationDatasetRecord::Rejected { reasons, .. } => {
                panic!("exact alignment boundary was rejected: {reasons:?}");
            }
        }
    }

    #[test]
    fn rejects_radar_beyond_alignment_boundary() {
        let record = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[radar(849_999_999, 1000), radar(1_150_000_000, 2000)],
        );
        assert!(matches!(record, CalibrationDatasetRecord::Rejected { .. }));
    }

    #[test]
    fn wall_clock_rollback_does_not_change_monotonic_alignment() {
        let mut before = radar(900_000_000, 1000);
        let mut after = radar(1_100_000_000, 2000);
        before.host_unix_ns = 5_000_000_000;
        after.host_unix_ns = 4_000_000_000;
        let record = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[before, after],
        );
        assert!(matches!(record, CalibrationDatasetRecord::Accepted { .. }));
    }

    #[test]
    fn rejects_interpolation_across_radar_reboot_or_transform_change() {
        let mut rebooted = radar(1_150_000_000, 2000);
        rebooted.boot_id = 2;
        let reboot_record = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[radar(850_000_000, 1000), rebooted],
        );
        assert!(matches!(reboot_record, CalibrationDatasetRecord::Rejected { reasons, .. } if reasons == vec!["radar_epoch_boot_or_transform_changed"]));

        let mut transformed = radar(1_150_000_000, 2000);
        transformed.transform_sha256 = "b".repeat(64);
        let transform_record = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[radar(850_000_000, 1000), transformed],
        );
        assert!(matches!(transform_record, CalibrationDatasetRecord::Rejected { reasons, .. } if reasons == vec!["radar_epoch_boot_or_transform_changed"]));
    }

    #[test]
    fn rejects_missing_receiver_or_receiver_alignment_over_limit() {
        let missing = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000)],
            &[radar(850_000_000, 1000), radar(1_150_000_000, 2000)],
        );
        assert!(matches!(missing, CalibrationDatasetRecord::Rejected { reasons, .. } if reasons == vec!["missing_rx4_midpoint"]));

        let late = align_record(
            "sample".into(),
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_150_000_001), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[radar(850_000_000, 1000), radar(1_150_000_000, 2000)],
        );
        assert!(matches!(late, CalibrationDatasetRecord::Rejected { reasons, .. } if reasons == vec!["rx2_alignment_limit_exceeded"]));
    }

    #[test]
    fn sample_ids_and_materialized_dataset_hashes_are_deterministic() {
        let first = deterministic_sample_id("a", "session", "Z001", 1_000);
        let second = deterministic_sample_id("a", "session", "Z001", 1_000);
        let changed = deterministic_sample_id("a", "session", "Z002", 1_000);
        assert_eq!(first, second);
        assert_ne!(first, changed);

        let record = align_record(
            first,
            "Z001".into(),
            &block(),
            1_000_000_000,
            &[(1, 1_000_000_000), (2, 1_000_000_000), (3, 1_000_000_000), (4, 1_000_000_000)],
            &[radar(850_000_000, 1000), radar(1_150_000_000, 2000)],
        );
        let first_dir = tempfile::tempdir().expect("first dataset directory");
        let second_dir = tempfile::tempdir().expect("second dataset directory");
        let first_identity = write_dataset(
            first_dir.path(),
            "run-01",
            "session-01",
            "setup-01",
            &"a".repeat(64),
            "epoch-01",
            "b".repeat(64),
            "c".repeat(64),
            9,
            std::slice::from_ref(&record),
        )
        .expect("first dataset");
        let second_identity = write_dataset(
            second_dir.path(),
            "run-01",
            "session-01",
            "setup-01",
            &"a".repeat(64),
            "epoch-01",
            "b".repeat(64),
            "c".repeat(64),
            9,
            std::slice::from_ref(&record),
        )
        .expect("second dataset");
        assert_eq!(
            first_identity.manifest_sha256,
            second_identity.manifest_sha256
        );
        assert_eq!(
            std::fs::read(
                first_dir.path().join("session-01.calibration-samples.v1.jsonl")
            )
            .expect("first samples"),
            std::fs::read(
                second_dir.path().join("session-01.calibration-samples.v1.jsonl")
            )
            .expect("second samples")
        );
    }
}
