//! Position index produced from mmWave-gated CSI blocks.
//!
//! Radar provides labels only while this artifact is built. Prediction reads
//! only the stored WiFi fingerprint model and empty-room projection.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::position_artifact::{
    deterministic_pretty_json, sha256_bytes, sha256_file, write_pretty_json_no_clobber,
};
use super::position_capture::{
    PositionCaptureGeometry, PositionEmptyReference, PositionFeatureBlock, PositionGridIdentity,
};
use super::position_fingerprint::{
    FingerprintPosition, PositionFingerprintModel, PositionFingerprintPrediction,
    MINIMUM_POSITION_COUNT, RECEIVER_COUNT,
};

pub(crate) const MMWAVE_INDEX_ALGORITHM: &str = "d6-mmwave-zoned-fingerprint-v2";
const LEGACY_MMWAVE_INDEX_ALGORITHM: &str = "d6-mmwave-nine-zone-fingerprint-v1";
const INDEX_KIND: &str = "ruview.mmwave-position-index";
const INDEX_SCHEMA_VERSION: u16 = 2;
const LEGACY_INDEX_SCHEMA_VERSION: u16 = 1;
pub(crate) const DEFAULT_ZONE_COUNT: usize = 9;
pub(crate) const MIN_ZONE_COUNT: usize = 3;
pub(crate) const MAX_ZONE_COUNT: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrainingBlockProvenance {
    pub(crate) zone_id: String,
    pub(crate) started_at_unix_ns: u64,
    pub(crate) ended_at_unix_ns: u64,
    pub(crate) csi_signal_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MmwavePositionIndexArtifact {
    schema_version: u16,
    kind: String,
    algorithm_id: String,
    setup_id: String,
    setup_sha256: String,
    server_version: String,
    geometry: PositionCaptureGeometry,
    radar_recording_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dataset_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zone_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alignment_limit_ms: Option<u64>,
    alignment_sha256: String,
    receiver_grids: Vec<PositionGridIdentity>,
    points: Vec<FingerprintPosition>,
    training_blocks: Vec<TrainingBlockProvenance>,
    empty_reference: PositionEmptyReference,
    model: PositionFingerprintModel,
}

impl MmwavePositionIndexArtifact {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        setup_id: String,
        setup_sha256: String,
        server_version: String,
        geometry: PositionCaptureGeometry,
        radar_recording_sha256: String,
        dataset_sha256: String,
        zone_count: usize,
        alignment_limit_ms: u64,
        receiver_grids: Vec<PositionGridIdentity>,
        mut points: Vec<FingerprintPosition>,
        mut training_blocks: Vec<TrainingBlockProvenance>,
        empty_reference: PositionEmptyReference,
        model: PositionFingerprintModel,
    ) -> Result<Self, String> {
        points.sort_by(|left, right| left.id.cmp(&right.id));
        training_blocks.sort_by(|left, right| {
            left.zone_id
                .cmp(&right.zone_id)
                .then_with(|| left.started_at_unix_ns.cmp(&right.started_at_unix_ns))
        });
        let alignment_bytes = deterministic_pretty_json(&training_blocks)
            .map_err(|error| format!("could not seal mmWave alignment: {error}"))?;
        let artifact = Self {
            schema_version: INDEX_SCHEMA_VERSION,
            kind: INDEX_KIND.to_string(),
            algorithm_id: MMWAVE_INDEX_ALGORITHM.to_string(),
            setup_id,
            setup_sha256,
            server_version,
            geometry,
            radar_recording_sha256,
            dataset_sha256: Some(dataset_sha256),
            zone_count: Some(zone_count),
            alignment_limit_ms: Some(alignment_limit_ms),
            alignment_sha256: sha256_bytes(&alignment_bytes),
            receiver_grids,
            points,
            training_blocks,
            empty_reference,
            model,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let legacy = self.schema_version == LEGACY_INDEX_SCHEMA_VERSION
            && self.algorithm_id == LEGACY_MMWAVE_INDEX_ALGORITHM;
        let current = self.schema_version == INDEX_SCHEMA_VERSION
            && self.algorithm_id == MMWAVE_INDEX_ALGORITHM;
        if self.kind != INDEX_KIND || (!legacy && !current) {
            return Err("unsupported mmWave position-index header".to_string());
        }
        validate_sha256("setup_sha256", &self.setup_sha256)?;
        validate_sha256("radar_recording_sha256", &self.radar_recording_sha256)?;
        validate_sha256("alignment_sha256", &self.alignment_sha256)?;
        if self.setup_id.trim().is_empty() || self.server_version.trim().is_empty() {
            return Err("mmWave index setup and server identity must not be empty".to_string());
        }
        let zone_count = if legacy {
            DEFAULT_ZONE_COUNT
        } else {
            let zone_count = self
                .zone_count
                .ok_or_else(|| "mmWave index v2 requires zone_count".to_string())?;
            if !(MIN_ZONE_COUNT..=MAX_ZONE_COUNT).contains(&zone_count) {
                return Err(format!("zone_count must be in {MIN_ZONE_COUNT}..={MAX_ZONE_COUNT}"));
            }
            validate_sha256(
                "dataset_sha256",
                self.dataset_sha256
                    .as_deref()
                    .ok_or_else(|| "mmWave index v2 requires dataset_sha256".to_string())?,
            )?;
            if self.alignment_limit_ms != Some(150) {
                return Err("mmWave index v2 requires the sealed 150 ms alignment policy".to_string());
            }
            zone_count
        };
        if self.points.len() != zone_count || self.points.len() < MINIMUM_POSITION_COUNT {
            return Err(format!("mmWave index requires exactly {zone_count} zones"));
        }
        for (index, point) in self.points.iter().enumerate() {
            let expected = if legacy {
                format!("P{:02}", index + 1)
            } else {
                format!("Z{:03}", index + 1)
            };
            if point.id != expected || point.coordinates_m.iter().any(|value| !value.is_finite()) {
                return Err("mmWave index zones are not finite or canonically ordered".to_string());
            }
        }
        if self.receiver_grids.len() != RECEIVER_COUNT {
            return Err(format!(
                "mmWave index requires {RECEIVER_COUNT} receiver grids"
            ));
        }
        if self.training_blocks.len() != zone_count * 6 {
            return Err("mmWave index requires exactly six blocks per zone".to_string());
        }
        for (point_index, point) in self.points.iter().enumerate() {
            let blocks = &self.training_blocks[point_index * 6..point_index * 6 + 6];
            if blocks.iter().any(|block| {
                block.zone_id != point.id
                    || block
                        .ended_at_unix_ns
                        .saturating_sub(block.started_at_unix_ns)
                        < 5_000_000_000
                    || validate_sha256("csi_signal_sha256", &block.csi_signal_sha256).is_err()
            }) {
                return Err(format!("invalid training provenance for {}", point.id));
            }
        }
        let alignment_bytes = deterministic_pretty_json(&self.training_blocks)
            .map_err(|error| format!("could not verify mmWave alignment: {error}"))?;
        if sha256_bytes(&alignment_bytes) != self.alignment_sha256 {
            return Err("mmWave alignment hash does not match its blocks".to_string());
        }
        self.empty_reference.validate()?;
        self.model.validate().map_err(|error| error.to_string())?;
        let model_points: Vec<_> = self.model.positions().cloned().collect();
        if model_points != self.points {
            return Err("mmWave model points do not match artifact points".to_string());
        }
        deterministic_pretty_json(self)
            .map_err(|error| format!("could not canonicalize mmWave index: {error}"))?;
        Ok(())
    }

    pub(crate) fn write(&self, path: &Path) -> Result<String, String> {
        write_pretty_json_no_clobber(path, self).map_err(|error| error.to_string())?;
        sha256_file(path).map_err(|error| error.to_string())
    }

    pub(crate) fn setup_id(&self) -> &str {
        &self.setup_id
    }

    pub(crate) fn setup_sha256(&self) -> &str {
        &self.setup_sha256
    }

    pub(crate) fn server_version(&self) -> &str {
        &self.server_version
    }

    pub(crate) fn geometry(&self) -> &PositionCaptureGeometry {
        &self.geometry
    }

    pub(crate) fn empty_reference(&self) -> &PositionEmptyReference {
        &self.empty_reference
    }

    pub(crate) fn predict_feature_block(
        &self,
        block: &PositionFeatureBlock,
    ) -> Result<PositionFingerprintPrediction, String> {
        if block.receivers.len() != RECEIVER_COUNT {
            return Err("mmWave feature block does not contain RX1-RX4".to_string());
        }
        let mut features = Vec::with_capacity(RECEIVER_COUNT);
        for (index, receiver) in block.receivers.iter().enumerate() {
            if receiver.rx_id != index as u8 + 1 || receiver.grid != self.receiver_grids[index] {
                return Err("mmWave feature block CSI grid does not match the index".to_string());
            }
            features.push(receiver.features.to_vec());
        }
        self.model
            .predict(&features)
            .map_err(|error| error.to_string())
    }

    /// Diagnostic single-receiver ablation for a complete, grid-validated
    /// blind feature block. This never replaces the four-RX live decision.
    pub(crate) fn predict_receiver_feature_block(
        &self,
        block: &PositionFeatureBlock,
        rx_id: u8,
    ) -> Result<FingerprintPosition, String> {
        let receiver_index = usize::from(
            rx_id
                .checked_sub(1)
                .ok_or_else(|| "receiver ablation rx_id must be in 1..=4".to_string())?,
        );
        if receiver_index >= RECEIVER_COUNT || block.receivers.len() != RECEIVER_COUNT {
            return Err("receiver ablation requires a complete RX1-RX4 block".to_string());
        }
        for (index, receiver) in block.receivers.iter().enumerate() {
            if receiver.rx_id != index as u8 + 1 || receiver.grid != self.receiver_grids[index] {
                return Err("mmWave feature block CSI grid does not match the index".to_string());
            }
        }
        self.model
            .nearest_position_for_receiver(
                receiver_index,
                &block.receivers[receiver_index].features,
            )
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn load_mmwave_position_index(
    path: &Path,
) -> Result<(MmwavePositionIndexArtifact, String), String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "could not read mmWave position index {}: {error}",
            path.display()
        )
    })?;
    let artifact: MmwavePositionIndexArtifact = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid mmWave position index {}: {error}", path.display()))?;
    artifact.validate()?;
    let sha256 = sha256_file(path).map_err(|error| error.to_string())?;
    Ok((artifact, sha256))
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{field} must be 64 lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position_capture::{
        build_position_empty_reference, PositionCapture, RxPositionFeatures, POSITION_FEATURE_COUNT,
    };
    use crate::position_fingerprint::{
        PositionFingerprintConfig, PositionFingerprintPrediction, PositionFingerprintSample,
        POSITION_COUNT,
    };
    use crate::raw_csi_recording::{
        IqPair, RawCsiFrame, SourceBinding, RAW_CSI_SCHEMA_VERSION, SOURCE_BINDING_REQUIRED_FLAGS,
        TX_SOURCE_BINDING_SCHEME, TX_SOURCE_BINDING_VERSION,
    };

    fn geometry() -> PositionCaptureGeometry {
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

    fn source_binding() -> SourceBinding {
        SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: SOURCE_BINDING_REQUIRED_FLAGS,
            scheme: TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: "f".repeat(64),
        }
    }

    fn empty_reference() -> PositionEmptyReference {
        let start = 1_000_000_000;
        let end = start + 65_000_000_000;
        let mut frames = Vec::new();
        let mut sequence = 0;
        let mut timestamp = start;
        while timestamp < end {
            for rx_id in 1..=4 {
                frames.push(RawCsiFrame {
                    schema_version: RAW_CSI_SCHEMA_VERSION,
                    host_timestamp_unix_ns: timestamp,
                    host_monotonic_ns: Some(timestamp),
                    clock_epoch_id: Some("test-clock".to_string()),
                    session_id: Some("synthetic-empty".to_string()),
                    label: Some("empty".to_string()),
                    ground_truth: None,
                    rx_id,
                    antenna_count: 1,
                    subcarrier_count: 8,
                    center_frequency_mhz: 2_437,
                    sequence,
                    rssi_dbm: -48,
                    noise_floor_dbm: -92,
                    ppdu_type: 0,
                    flags: 0,
                    mesh_timestamp_us: None,
                    source_binding: Some(source_binding()),
                    iq_pairs: (0..8)
                        .map(|_| IqPair {
                            i: rx_id as i8,
                            q: 2,
                        })
                        .collect(),
                });
            }
            sequence += 1;
            timestamp += 200_000_000;
        }
        build_position_empty_reference(
            &PositionCapture {
                recording_id: "synthetic-empty".to_string(),
                setup_id: "sealed-d6".to_string(),
                setup_sha256: "a".repeat(64),
                server_version: "test".to_string(),
                geometry: geometry(),
                started_at_unix_ns: start,
                ended_at_unix_ns: end,
                frames,
            },
            &"a".repeat(64),
        )
        .expect("synthetic empty reference")
    }

    #[test]
    fn synthetic_guided_training_round_trips_and_predicts_without_radar() {
        let points: Vec<_> = (0..POSITION_COUNT)
            .map(|index| FingerprintPosition {
                id: format!("Z{:03}", index + 1),
                coordinates_m: [index as f64 * 0.8, 0.0, index as f64 * 0.4],
            })
            .collect();
        let samples: Vec<_> = points
            .iter()
            .enumerate()
            .flat_map(|(index, point)| {
                (0..6).map(move |repeat| PositionFingerprintSample {
                    position: point.clone(),
                    rx_features: (0..RECEIVER_COUNT)
                        .map(|rx| {
                            (0..POSITION_FEATURE_COUNT)
                                .map(|feature| {
                                    index as f64 * 10.0
                                        + rx as f64
                                        + feature as f64 / 100.0
                                        + repeat as f64 / 1000.0
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
                minimum_samples_per_position: 6,
            },
        )
        .expect("train synthetic WiFi-only model");
        let grid = PositionGridIdentity {
            center_frequency_mhz: 2_437,
            antenna_count: 1,
            subcarrier_count: 8,
            ppdu_type: 0,
            layout_flags: 0,
        };
        let blocks: Vec<_> = points
            .iter()
            .flat_map(|point| {
                (0_u64..6).map(move |repeat| TrainingBlockProvenance {
                    zone_id: point.id.clone(),
                    started_at_unix_ns: repeat * 6_000_000_000,
                    ended_at_unix_ns: repeat * 6_000_000_000 + 5_000_000_000,
                    csi_signal_sha256: format!("{:064x}", repeat + 1),
                })
            })
            .collect();
        let artifact = MmwavePositionIndexArtifact::new(
            "sealed-d6".to_string(),
            "a".repeat(64),
            "test".to_string(),
            geometry(),
            "b".repeat(64),
            "c".repeat(64),
            POSITION_COUNT,
            150,
            vec![grid; RECEIVER_COUNT],
            points,
            blocks,
            empty_reference(),
            model,
        )
        .expect("build synthetic index");

        let target_index = 4usize;
        let feature_block = PositionFeatureBlock {
            window_start_unix_ns: 1,
            window_end_unix_ns: 2,
            common_coverage_ns: 1,
            receivers: (0..RECEIVER_COUNT)
                .map(|rx| RxPositionFeatures {
                    rx_id: rx as u8 + 1,
                    grid,
                    frame_count: 25,
                    observed_rate_millihz: 5_000,
                    coverage_ns: 5_000_000_000,
                    maximum_gap_ns: 200_000_000,
                    features: std::array::from_fn(|feature| {
                        target_index as f64 * 10.0 + rx as f64 + feature as f64 / 100.0
                    }),
                })
                .collect(),
        };
        let prediction = artifact
            .predict_feature_block(&feature_block)
            .expect("predict from CSI features only");
        assert!(matches!(
            prediction,
            PositionFingerprintPrediction::Position { position, .. } if position.id == "Z005"
        ));
        for rx_id in 1..=4 {
            assert_eq!(
                artifact
                    .predict_receiver_feature_block(&feature_block, rx_id)
                    .expect("receiver ablation uses WiFi features only")
                    .id,
                "Z005"
            );
        }

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("guided-index.json");
        let written_sha = artifact.write(&path).expect("write index once");
        let (loaded, loaded_sha) = load_mmwave_position_index(&path).expect("load sealed index");
        assert_eq!(written_sha, loaded_sha);
        assert_eq!(loaded.setup_id(), artifact.setup_id());
        assert_eq!(loaded.setup_sha256(), artifact.setup_sha256());
        assert!(matches!(
            loaded
                .predict_feature_block(&feature_block)
                .expect("loaded index predicts from CSI features"),
                PositionFingerprintPrediction::Position { position, .. } if position.id == "Z005"
        ));
        assert!(
            artifact.write(&path).is_err(),
            "index write must not clobber"
        );
    }
}
