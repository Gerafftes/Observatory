//! Deterministic discrete position fingerprints for a sealed set of zones.
//!
//! This module deliberately contains no capture parsing, I/O, presence logic,
//! temporal smoothing, or continuous coordinate estimation. A later extractor
//! supplies exactly four receivers with 28 finite features each. The classifier
//! returns one configured position, `unknown`, or `ambiguous`.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

pub(crate) const MINIMUM_POSITION_COUNT: usize = 3;
/// Legacy P01-P09 benchmark cardinality. The fingerprint model itself is not
/// constrained by this value.
pub(crate) const POSITION_COUNT: usize = 9;
pub(crate) const RECEIVER_COUNT: usize = 4;
pub(crate) const FEATURES_PER_RECEIVER: usize = 28;

const MODEL_SCHEMA_VERSION: u16 = 2;
const LEGACY_MODEL_SCHEMA_VERSION: u16 = 1;
const ROBUST_SIGMA_FACTOR: f64 = 1.4826;
const GLOBAL_SCALE_FALLBACK_FRACTION: f64 = 0.05;
const NUMERICAL_SCALE_FLOOR: f64 = 1e-6;
const OOD_THRESHOLD_MULTIPLIER: f64 = 1.20;
/// A zero-radius class would reject every harmless deviation when its training
/// samples happen to be identical. This normalized RMS floor remains strict,
/// but gives such a class a finite, conservative acceptance neighbourhood.
const MINIMUM_OOD_THRESHOLD: f64 = 0.50;
const AMBIGUITY_MARGIN: f64 = 0.20;

type FeatureMatrix = [[f64; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FingerprintPosition {
    pub(crate) id: String,
    /// Fixed labelled point in metres. Predictions always return this exact
    /// coordinate and never interpolate between configured points.
    pub(crate) coordinates_m: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionFingerprintSample {
    pub(crate) position: FingerprintPosition,
    /// Capture-extractor output. Training validates exactly `4 × 28` values.
    pub(crate) rx_features: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PositionFingerprintConfig {
    /// At least two independent time blocks are required. A later fold can set
    /// this to six for the planned six five-second blocks per recording.
    pub(crate) minimum_samples_per_position: usize,
}

impl Default for PositionFingerprintConfig {
    fn default() -> Self {
        Self {
            minimum_samples_per_position: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PositionPrototype {
    position: FingerprintPosition,
    prototype: FeatureMatrix,
    ood_threshold: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionFingerprintModel {
    schema_version: u16,
    config: PositionFingerprintConfig,
    prototypes: Vec<PositionPrototype>,
    shared_scale: FeatureMatrix,
    ood_threshold_multiplier: f64,
    minimum_ood_threshold: f64,
    ambiguity_margin: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PositionCandidate {
    pub(crate) position: FingerprintPosition,
    pub(crate) distance: f64,
    pub(crate) ood_threshold: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum PositionFingerprintPrediction {
    Position {
        position: FingerprintPosition,
        distance: f64,
        runner_up_distance: f64,
        margin: f64,
    },
    Unknown {
        nearest_position_id: String,
        distance: f64,
        ood_threshold: f64,
        margin: f64,
    },
    Ambiguous {
        candidates: Vec<PositionCandidate>,
        margin: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PositionFingerprintError {
    SchemaVersion {
        expected: u16,
        actual: u16,
    },
    AlgorithmParameterMismatch {
        name: &'static str,
    },
    InvalidMinimumSamples {
        minimum: usize,
    },
    EmptyTrainingSet,
    TooFewPositions {
        minimum: usize,
        actual: usize,
    },
    EmptyPositionId,
    NonFinitePosition {
        id: String,
        coordinate_index: usize,
    },
    InconsistentPositionCoordinates {
        id: String,
    },
    DuplicatePositionCoordinates {
        first_id: String,
        second_id: String,
    },
    NonIncreasingPositionId {
        previous_id: String,
        current_id: String,
    },
    NonFinitePrototype {
        position_id: String,
        receiver_index: usize,
        feature_index: usize,
    },
    InvalidSharedScale {
        receiver_index: usize,
        feature_index: usize,
    },
    InvalidOodThreshold {
        position_id: String,
    },
    ReceiverCount {
        expected: usize,
        actual: usize,
    },
    ReceiverIndex {
        actual: usize,
    },
    FeatureCount {
        receiver_index: usize,
        expected: usize,
        actual: usize,
    },
    NonFiniteFeature {
        receiver_index: usize,
        feature_index: usize,
    },
    InsufficientSamples {
        position_id: String,
        minimum: usize,
        actual: usize,
    },
}

impl fmt::Display for PositionFingerprintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaVersion { expected, actual } => write!(
                formatter,
                "position fingerprint schema version must be {expected}, got {actual}"
            ),
            Self::AlgorithmParameterMismatch { name } => {
                write!(formatter, "stored algorithm parameter {name:?} does not match")
            }
            Self::InvalidMinimumSamples { minimum } => write!(
                formatter,
                "minimum samples per position must be at least 2, got {minimum}"
            ),
            Self::EmptyTrainingSet => write!(formatter, "position training set is empty"),
            Self::TooFewPositions { minimum, actual } => write!(
                formatter,
                "position training requires at least {minimum} positions, got {actual}"
            ),
            Self::EmptyPositionId => write!(formatter, "position ID must not be empty"),
            Self::NonFinitePosition {
                id,
                coordinate_index,
            } => write!(
                formatter,
                "position {id:?} coordinate {coordinate_index} is not finite"
            ),
            Self::InconsistentPositionCoordinates { id } => write!(
                formatter,
                "position {id:?} has inconsistent coordinates across samples"
            ),
            Self::DuplicatePositionCoordinates {
                first_id,
                second_id,
            } => write!(
                formatter,
                "positions {first_id:?} and {second_id:?} use the same coordinates"
            ),
            Self::NonIncreasingPositionId {
                previous_id,
                current_id,
            } => write!(
                formatter,
                "position IDs must be unique and strictly sorted, got {previous_id:?} before {current_id:?}"
            ),
            Self::NonFinitePrototype {
                position_id,
                receiver_index,
                feature_index,
            } => write!(
                formatter,
                "position {position_id:?} prototype receiver {receiver_index} feature {feature_index} is not finite"
            ),
            Self::InvalidSharedScale {
                receiver_index,
                feature_index,
            } => write!(
                formatter,
                "shared scale receiver {receiver_index} feature {feature_index} must be finite and positive"
            ),
            Self::InvalidOodThreshold { position_id } => write!(
                formatter,
                "position {position_id:?} OOD threshold must be finite, positive, and not below the model floor"
            ),
            Self::ReceiverCount { expected, actual } => write!(
                formatter,
                "fingerprint requires exactly {expected} receivers, got {actual}"
            ),
            Self::ReceiverIndex { actual } => write!(
                formatter,
                "fingerprint receiver index must be in 0..{RECEIVER_COUNT}, got {actual}"
            ),
            Self::FeatureCount {
                receiver_index,
                expected,
                actual,
            } => write!(
                formatter,
                "receiver {receiver_index} requires exactly {expected} features, got {actual}"
            ),
            Self::NonFiniteFeature {
                receiver_index,
                feature_index,
            } => write!(
                formatter,
                "receiver {receiver_index} feature {feature_index} is not finite"
            ),
            Self::InsufficientSamples {
                position_id,
                minimum,
                actual,
            } => write!(
                formatter,
                "position {position_id:?} needs at least {minimum} samples, got {actual}"
            ),
        }
    }
}

impl Error for PositionFingerprintError {}

#[derive(Debug)]
struct PositionTrainingGroup {
    position: FingerprintPosition,
    samples: Vec<FeatureMatrix>,
}

#[derive(Debug)]
struct RankedPosition<'a> {
    prototype: &'a PositionPrototype,
    distance: f64,
}

impl PositionFingerprintModel {
    pub(crate) fn train(
        samples: &[PositionFingerprintSample],
        config: PositionFingerprintConfig,
    ) -> Result<Self, PositionFingerprintError> {
        if config.minimum_samples_per_position < 2 {
            return Err(PositionFingerprintError::InvalidMinimumSamples {
                minimum: config.minimum_samples_per_position,
            });
        }
        if samples.is_empty() {
            return Err(PositionFingerprintError::EmptyTrainingSet);
        }

        let mut groups: BTreeMap<String, PositionTrainingGroup> = BTreeMap::new();
        for sample in samples {
            validate_position(&sample.position)?;
            let features = validate_feature_matrix(&sample.rx_features)?;
            match groups.get_mut(&sample.position.id) {
                Some(group) if group.position.coordinates_m != sample.position.coordinates_m => {
                    return Err(PositionFingerprintError::InconsistentPositionCoordinates {
                        id: sample.position.id.clone(),
                    });
                }
                Some(group) => group.samples.push(features),
                None => {
                    groups.insert(
                        sample.position.id.clone(),
                        PositionTrainingGroup {
                            position: sample.position.clone(),
                            samples: vec![features],
                        },
                    );
                }
            }
        }

        if groups.len() < MINIMUM_POSITION_COUNT {
            return Err(PositionFingerprintError::TooFewPositions {
                minimum: MINIMUM_POSITION_COUNT,
                actual: groups.len(),
            });
        }

        let ordered_groups: Vec<PositionTrainingGroup> = groups.into_values().collect();
        reject_duplicate_coordinates(&ordered_groups)?;
        for group in &ordered_groups {
            if group.samples.len() < config.minimum_samples_per_position {
                return Err(PositionFingerprintError::InsufficientSamples {
                    position_id: group.position.id.clone(),
                    minimum: config.minimum_samples_per_position,
                    actual: group.samples.len(),
                });
            }
        }

        let initial_prototypes: Vec<FeatureMatrix> = ordered_groups
            .iter()
            .map(|group| median_matrix(&group.samples))
            .collect();
        let shared_scale = shared_robust_scale(&ordered_groups, &initial_prototypes);
        let prototypes = ordered_groups
            .iter()
            .zip(initial_prototypes)
            .map(|(group, prototype)| {
                let maximum_loo_distance = group
                    .samples
                    .iter()
                    .enumerate()
                    .map(|(held_out_index, held_out)| {
                        let remaining: Vec<FeatureMatrix> = group
                            .samples
                            .iter()
                            .enumerate()
                            .filter_map(|(index, sample)| {
                                (index != held_out_index).then_some(*sample)
                            })
                            .collect();
                        let leave_one_out_prototype = median_matrix(&remaining);
                        normalized_distance(held_out, &leave_one_out_prototype, &shared_scale)
                    })
                    .fold(0.0, f64::max);
                let ood_threshold =
                    (OOD_THRESHOLD_MULTIPLIER * maximum_loo_distance).max(MINIMUM_OOD_THRESHOLD);
                PositionPrototype {
                    position: group.position.clone(),
                    prototype,
                    ood_threshold,
                }
            })
            .collect();

        let model = Self {
            schema_version: MODEL_SCHEMA_VERSION,
            config,
            prototypes,
            shared_scale,
            ood_threshold_multiplier: OOD_THRESHOLD_MULTIPLIER,
            minimum_ood_threshold: MINIMUM_OOD_THRESHOLD,
            ambiguity_margin: AMBIGUITY_MARGIN,
        };
        model.validate()?;
        Ok(model)
    }

    /// Validate every invariant required before a deserialized model can be
    /// indexed or used for a decision.
    pub(crate) fn validate(&self) -> Result<(), PositionFingerprintError> {
        if !matches!(self.schema_version, LEGACY_MODEL_SCHEMA_VERSION | MODEL_SCHEMA_VERSION) {
            return Err(PositionFingerprintError::SchemaVersion {
                expected: MODEL_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        if self.config.minimum_samples_per_position < 2 {
            return Err(PositionFingerprintError::InvalidMinimumSamples {
                minimum: self.config.minimum_samples_per_position,
            });
        }
        validate_stored_parameter(
            "ood_threshold_multiplier",
            self.ood_threshold_multiplier,
            OOD_THRESHOLD_MULTIPLIER,
        )?;
        validate_stored_parameter(
            "minimum_ood_threshold",
            self.minimum_ood_threshold,
            MINIMUM_OOD_THRESHOLD,
        )?;
        validate_stored_parameter("ambiguity_margin", self.ambiguity_margin, AMBIGUITY_MARGIN)?;
        if self.schema_version == LEGACY_MODEL_SCHEMA_VERSION && self.prototypes.len() != POSITION_COUNT {
            return Err(PositionFingerprintError::TooFewPositions {
                minimum: POSITION_COUNT,
                actual: self.prototypes.len(),
            });
        }
        if self.schema_version == MODEL_SCHEMA_VERSION && self.prototypes.len() < MINIMUM_POSITION_COUNT {
            return Err(PositionFingerprintError::TooFewPositions {
                minimum: MINIMUM_POSITION_COUNT,
                actual: self.prototypes.len(),
            });
        }

        for receiver_index in 0..RECEIVER_COUNT {
            for feature_index in 0..FEATURES_PER_RECEIVER {
                let scale = self.shared_scale[receiver_index][feature_index];
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(PositionFingerprintError::InvalidSharedScale {
                        receiver_index,
                        feature_index,
                    });
                }
            }
        }

        for (index, prototype) in self.prototypes.iter().enumerate() {
            validate_position(&prototype.position)?;
            if let Some(previous) = index
                .checked_sub(1)
                .map(|previous| &self.prototypes[previous])
            {
                if previous.position.id >= prototype.position.id {
                    return Err(PositionFingerprintError::NonIncreasingPositionId {
                        previous_id: previous.position.id.clone(),
                        current_id: prototype.position.id.clone(),
                    });
                }
            }
            if !prototype.ood_threshold.is_finite()
                || prototype.ood_threshold < MINIMUM_OOD_THRESHOLD
            {
                return Err(PositionFingerprintError::InvalidOodThreshold {
                    position_id: prototype.position.id.clone(),
                });
            }
            for receiver_index in 0..RECEIVER_COUNT {
                for feature_index in 0..FEATURES_PER_RECEIVER {
                    if !prototype.prototype[receiver_index][feature_index].is_finite() {
                        return Err(PositionFingerprintError::NonFinitePrototype {
                            position_id: prototype.position.id.clone(),
                            receiver_index,
                            feature_index,
                        });
                    }
                }
            }
        }

        for (index, first) in self.prototypes.iter().enumerate() {
            for second in &self.prototypes[index + 1..] {
                if same_floor_coordinates(
                    first.position.coordinates_m,
                    second.position.coordinates_m,
                ) {
                    return Err(PositionFingerprintError::DuplicatePositionCoordinates {
                        first_id: first.position.id.clone(),
                        second_id: second.position.id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn predict(
        &self,
        rx_features: &[Vec<f64>],
    ) -> Result<PositionFingerprintPrediction, PositionFingerprintError> {
        self.validate()?;
        let features = validate_feature_matrix(rx_features)?;
        let mut ranked: Vec<RankedPosition<'_>> = self
            .prototypes
            .iter()
            .map(|prototype| RankedPosition {
                distance: normalized_distance(&features, &prototype.prototype, &self.shared_scale),
                prototype,
            })
            .collect();
        ranked.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then_with(|| left.prototype.position.id.cmp(&right.prototype.position.id))
        });

        let nearest = &ranked[0];
        let runner_up = &ranked[1];
        let margin = relative_margin(nearest.distance, runner_up.distance);
        if nearest.distance > nearest.prototype.ood_threshold {
            return Ok(PositionFingerprintPrediction::Unknown {
                nearest_position_id: nearest.prototype.position.id.clone(),
                distance: nearest.distance,
                ood_threshold: nearest.prototype.ood_threshold,
                margin,
            });
        }

        let candidates: Vec<PositionCandidate> = ranked
            .iter()
            .take_while(|candidate| {
                relative_margin(nearest.distance, candidate.distance) < AMBIGUITY_MARGIN
            })
            .map(|candidate| PositionCandidate {
                position: candidate.prototype.position.clone(),
                distance: candidate.distance,
                ood_threshold: candidate.prototype.ood_threshold,
            })
            .collect();
        if margin < AMBIGUITY_MARGIN || candidates.len() > 1 {
            return Ok(PositionFingerprintPrediction::Ambiguous { candidates, margin });
        }

        Ok(PositionFingerprintPrediction::Position {
            position: nearest.prototype.position.clone(),
            distance: nearest.distance,
            runner_up_distance: runner_up.distance,
            margin,
        })
    }

    /// Return the nearest stored position using one receiver only.
    ///
    /// This is a diagnostic ablation for RX1-RX4 blind-test reporting. It is
    /// deliberately not an OOD-gated public prediction: the deployed decision
    /// continues to use [`Self::predict`] and all four receivers equally.
    pub(crate) fn nearest_position_for_receiver(
        &self,
        receiver_index: usize,
        features: &[f64],
    ) -> Result<FingerprintPosition, PositionFingerprintError> {
        self.validate()?;
        if receiver_index >= RECEIVER_COUNT {
            return Err(PositionFingerprintError::ReceiverIndex {
                actual: receiver_index,
            });
        }
        if features.len() != FEATURES_PER_RECEIVER {
            return Err(PositionFingerprintError::FeatureCount {
                receiver_index,
                expected: FEATURES_PER_RECEIVER,
                actual: features.len(),
            });
        }
        for (feature_index, feature) in features.iter().enumerate() {
            if !feature.is_finite() {
                return Err(PositionFingerprintError::NonFiniteFeature {
                    receiver_index,
                    feature_index,
                });
            }
        }
        self.prototypes
            .iter()
            .map(|prototype| {
                (
                    normalized_receiver_distance(
                        features,
                        &prototype.prototype[receiver_index],
                        &self.shared_scale[receiver_index],
                    ),
                    &prototype.position,
                )
            })
            .min_by(|left, right| {
                left.0
                    .total_cmp(&right.0)
                    .then_with(|| left.1.id.cmp(&right.1.id))
            })
            .map(|(_, position)| position.clone())
            .ok_or(PositionFingerprintError::EmptyTrainingSet)
    }

    pub(crate) fn positions(&self) -> impl ExactSizeIterator<Item = &FingerprintPosition> {
        self.prototypes.iter().map(|prototype| &prototype.position)
    }
}

fn validate_stored_parameter(
    name: &'static str,
    actual: f64,
    expected: f64,
) -> Result<(), PositionFingerprintError> {
    if !actual.is_finite() || actual.to_bits() != expected.to_bits() {
        return Err(PositionFingerprintError::AlgorithmParameterMismatch { name });
    }
    Ok(())
}

fn validate_position(position: &FingerprintPosition) -> Result<(), PositionFingerprintError> {
    if position.id.trim().is_empty() {
        return Err(PositionFingerprintError::EmptyPositionId);
    }
    for (coordinate_index, coordinate) in position.coordinates_m.iter().enumerate() {
        if !coordinate.is_finite() {
            return Err(PositionFingerprintError::NonFinitePosition {
                id: position.id.clone(),
                coordinate_index,
            });
        }
    }
    Ok(())
}

fn validate_feature_matrix(
    rx_features: &[Vec<f64>],
) -> Result<FeatureMatrix, PositionFingerprintError> {
    if rx_features.len() != RECEIVER_COUNT {
        return Err(PositionFingerprintError::ReceiverCount {
            expected: RECEIVER_COUNT,
            actual: rx_features.len(),
        });
    }

    let mut matrix = [[0.0; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];
    for (receiver_index, features) in rx_features.iter().enumerate() {
        if features.len() != FEATURES_PER_RECEIVER {
            return Err(PositionFingerprintError::FeatureCount {
                receiver_index,
                expected: FEATURES_PER_RECEIVER,
                actual: features.len(),
            });
        }
        for (feature_index, feature) in features.iter().enumerate() {
            if !feature.is_finite() {
                return Err(PositionFingerprintError::NonFiniteFeature {
                    receiver_index,
                    feature_index,
                });
            }
            matrix[receiver_index][feature_index] = *feature;
        }
    }
    Ok(matrix)
}

fn reject_duplicate_coordinates(
    groups: &[PositionTrainingGroup],
) -> Result<(), PositionFingerprintError> {
    for (index, first) in groups.iter().enumerate() {
        for second in &groups[index + 1..] {
            if same_floor_coordinates(first.position.coordinates_m, second.position.coordinates_m) {
                return Err(PositionFingerprintError::DuplicatePositionCoordinates {
                    first_id: first.position.id.clone(),
                    second_id: second.position.id.clone(),
                });
            }
        }
    }
    Ok(())
}

fn same_floor_coordinates(left: [f64; 3], right: [f64; 3]) -> bool {
    left[0] == right[0] && left[2] == right[2]
}

fn median_matrix(samples: &[FeatureMatrix]) -> FeatureMatrix {
    let mut result = [[0.0; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];
    for receiver_index in 0..RECEIVER_COUNT {
        for feature_index in 0..FEATURES_PER_RECEIVER {
            let mut values: Vec<f64> = samples
                .iter()
                .map(|sample| sample[receiver_index][feature_index])
                .collect();
            result[receiver_index][feature_index] = median(&mut values);
        }
    }
    result
}

fn shared_robust_scale(
    groups: &[PositionTrainingGroup],
    prototypes: &[FeatureMatrix],
) -> FeatureMatrix {
    let mut scale = [[0.0; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];
    for receiver_index in 0..RECEIVER_COUNT {
        for feature_index in 0..FEATURES_PER_RECEIVER {
            let mut within_class_residuals = Vec::new();
            let mut all_values = Vec::new();
            for (group, prototype) in groups.iter().zip(prototypes) {
                for sample in &group.samples {
                    let value = sample[receiver_index][feature_index];
                    all_values.push(value);
                    within_class_residuals
                        .push((value - prototype[receiver_index][feature_index]).abs());
                }
            }
            let within_scale = ROBUST_SIGMA_FACTOR * median(&mut within_class_residuals);
            let mut values_for_global_center = all_values.clone();
            let global_center = median(&mut values_for_global_center);
            let mut global_residuals: Vec<f64> = all_values
                .iter()
                .map(|value| (value - global_center).abs())
                .collect();
            let global_scale = ROBUST_SIGMA_FACTOR * median(&mut global_residuals);
            scale[receiver_index][feature_index] = within_scale
                .max(GLOBAL_SCALE_FALLBACK_FRACTION * global_scale)
                .max(NUMERICAL_SCALE_FLOOR);
        }
    }
    scale
}

/// Equal receiver weighting: first average the squared standardized residuals
/// within each receiver, then average the four receiver distances.
fn normalized_distance(
    features: &FeatureMatrix,
    prototype: &FeatureMatrix,
    scale: &FeatureMatrix,
) -> f64 {
    let receiver_mean_squared_sum = (0..RECEIVER_COUNT)
        .map(|receiver_index| {
            (0..FEATURES_PER_RECEIVER)
                .map(|feature_index| {
                    let residual = (features[receiver_index][feature_index]
                        - prototype[receiver_index][feature_index])
                        / scale[receiver_index][feature_index];
                    residual * residual
                })
                .sum::<f64>()
                / FEATURES_PER_RECEIVER as f64
        })
        .sum::<f64>();
    (receiver_mean_squared_sum / RECEIVER_COUNT as f64).sqrt()
}

fn normalized_receiver_distance(
    features: &[f64],
    prototype: &[f64; FEATURES_PER_RECEIVER],
    scale: &[f64; FEATURES_PER_RECEIVER],
) -> f64 {
    let mean_squared = features
        .iter()
        .zip(prototype)
        .zip(scale)
        .map(|((feature, prototype), scale)| {
            let residual = (feature - prototype) / scale;
            residual * residual
        })
        .sum::<f64>()
        / FEATURES_PER_RECEIVER as f64;
    mean_squared.sqrt()
}

fn relative_margin(nearest_distance: f64, other_distance: f64) -> f64 {
    if other_distance <= f64::EPSILON {
        0.0
    } else {
        ((other_distance - nearest_distance) / other_distance).clamp(0.0, 1.0)
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(index: usize) -> FingerprintPosition {
        let columns = [0.75, 2.01, 3.27];
        let rows = [0.75, 1.72, 2.69];
        FingerprintPosition {
            id: format!("P{:02}", index + 1),
            coordinates_m: [columns[index % 3], 0.0, rows[index / 3]],
        }
    }

    fn feature_rows(value: f64) -> Vec<Vec<f64>> {
        vec![vec![value; FEATURES_PER_RECEIVER]; RECEIVER_COUNT]
    }

    fn training_samples(spacing: f64, variation: f64) -> Vec<PositionFingerprintSample> {
        (0..POSITION_COUNT)
            .flat_map(|index| {
                let center = index as f64 * spacing;
                [-variation, variation].map(|offset| PositionFingerprintSample {
                    position: position(index),
                    rx_features: feature_rows(center + offset),
                })
            })
            .collect()
    }

    fn zoned_training_samples(zone_count: usize) -> Vec<PositionFingerprintSample> {
        (0..zone_count)
            .flat_map(|index| {
                [-0.1, 0.1].map(move |offset| PositionFingerprintSample {
                    position: FingerprintPosition {
                        id: format!("Z{:03}", index + 1),
                        coordinates_m: [index as f64, 0.0, index as f64 / 2.0],
                    },
                    rx_features: feature_rows(index as f64 * 10.0 + offset),
                })
            })
            .collect()
    }

    #[test]
    fn current_model_accepts_variable_zone_counts() {
        for zone_count in [3, 9, 12] {
            let model = PositionFingerprintModel::train(
                &zoned_training_samples(zone_count),
                PositionFingerprintConfig::default(),
            )
            .unwrap();
            assert_eq!(model.positions().count(), zone_count);
        }
    }

    #[test]
    fn exact_fingerprint_returns_its_discrete_position() {
        let samples = training_samples(10.0, 1.0);
        let model = PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default())
            .unwrap();

        let prediction = model.predict(&feature_rows(40.0)).unwrap();

        match prediction {
            PositionFingerprintPrediction::Position { position, .. } => {
                assert_eq!(position.id, "P05");
                assert_eq!(position.coordinates_m, [2.01, 0.0, 1.72]);
            }
            other => panic!("expected exact position, got {other:?}"),
        }
    }

    #[test]
    fn positive_and_negative_features_are_treated_symmetrically() {
        let samples = training_samples(10.0, 2.0);
        let model = PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default())
            .unwrap();

        let below = model.predict(&feature_rows(39.5)).unwrap();
        let above = model.predict(&feature_rows(40.5)).unwrap();

        let extract = |prediction: PositionFingerprintPrediction| match prediction {
            PositionFingerprintPrediction::Position {
                position, distance, ..
            } => (position.id, distance),
            other => panic!("expected position, got {other:?}"),
        };
        let (below_id, below_distance) = extract(below);
        let (above_id, above_distance) = extract(above);
        assert_eq!(below_id, "P05");
        assert_eq!(above_id, "P05");
        assert!((below_distance - above_distance).abs() < 1e-12);
    }

    #[test]
    fn distant_fingerprint_is_unknown() {
        let samples = training_samples(10.0, 1.0);
        let model = PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default())
            .unwrap();

        let prediction = model.predict(&feature_rows(1_000.0)).unwrap();

        assert!(matches!(
            prediction,
            PositionFingerprintPrediction::Unknown { .. }
        ));
    }

    #[test]
    fn close_runner_up_is_ambiguous() {
        let samples = training_samples(10.0, 4.0);
        let model = PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default())
            .unwrap();

        let prediction = model.predict(&feature_rows(5.0)).unwrap();

        match prediction {
            PositionFingerprintPrediction::Ambiguous { candidates, margin } => {
                assert_eq!(margin, 0.0);
                assert_eq!(candidates.len(), 2);
                assert_eq!(candidates[0].position.id, "P01");
                assert_eq!(candidates[1].position.id, "P02");
            }
            other => panic!("expected ambiguity, got {other:?}"),
        }
    }

    #[test]
    fn receiver_feature_and_finite_validation_fail_closed() {
        let mut samples = training_samples(10.0, 1.0);
        samples[0].rx_features.pop();
        assert!(matches!(
            PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default()),
            Err(PositionFingerprintError::ReceiverCount {
                expected: RECEIVER_COUNT,
                actual: 3
            })
        ));

        let mut samples = training_samples(10.0, 1.0);
        samples[0].rx_features[2].pop();
        assert!(matches!(
            PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default()),
            Err(PositionFingerprintError::FeatureCount {
                receiver_index: 2,
                expected: FEATURES_PER_RECEIVER,
                actual: 27
            })
        ));

        let mut samples = training_samples(10.0, 1.0);
        samples[0].rx_features[1][7] = f64::NAN;
        assert!(matches!(
            PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default()),
            Err(PositionFingerprintError::NonFiniteFeature {
                receiver_index: 1,
                feature_index: 7
            })
        ));

        let model = PositionFingerprintModel::train(
            &training_samples(10.0, 1.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        assert!(matches!(
            model.predict(&feature_rows(0.0)[..3]),
            Err(PositionFingerprintError::ReceiverCount {
                expected: RECEIVER_COUNT,
                actual: 3
            })
        ));
        let mut wrong_prediction_width = feature_rows(0.0);
        wrong_prediction_width[3].push(0.0);
        assert!(matches!(
            model.predict(&wrong_prediction_width),
            Err(PositionFingerprintError::FeatureCount {
                receiver_index: 3,
                expected: FEATURES_PER_RECEIVER,
                actual: 29
            })
        ));
        assert!(matches!(
            model.predict(&feature_rows(f64::INFINITY)),
            Err(PositionFingerprintError::NonFiniteFeature { .. })
        ));
    }

    #[test]
    fn configured_minimum_samples_is_enforced() {
        let samples = training_samples(10.0, 1.0);
        assert!(matches!(
            PositionFingerprintModel::train(
                &samples,
                PositionFingerprintConfig {
                    minimum_samples_per_position: 1,
                }
            ),
            Err(PositionFingerprintError::InvalidMinimumSamples { minimum: 1 })
        ));

        let config = PositionFingerprintConfig {
            minimum_samples_per_position: 3,
        };

        assert!(matches!(
            PositionFingerprintModel::train(&samples, config),
            Err(PositionFingerprintError::InsufficientSamples {
                minimum: 3,
                actual: 2,
                ..
            })
        ));
    }

    #[test]
    fn receiver_distance_components_have_equal_weight() {
        let zero = [[0.0; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];
        let one = [[1.0; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];
        let mut one_receiver = zero;
        one_receiver[0] = [1.0; FEATURES_PER_RECEIVER];
        let all_receivers_half = [[0.5; FEATURES_PER_RECEIVER]; RECEIVER_COUNT];

        let one_receiver_distance = normalized_distance(&one_receiver, &zero, &one);
        let all_receiver_distance = normalized_distance(&all_receivers_half, &zero, &one);

        assert!((one_receiver_distance - 0.5).abs() < 1e-12);
        assert!((all_receiver_distance - 0.5).abs() < 1e-12);
    }

    #[test]
    fn training_order_and_model_serialization_are_deterministic() {
        let samples = training_samples(10.0, 1.0);
        let mut reversed = samples.clone();
        reversed.reverse();
        let first = PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default())
            .unwrap();
        let second =
            PositionFingerprintModel::train(&reversed, PositionFingerprintConfig::default())
                .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        assert_eq!(
            first.predict(&feature_rows(60.0)).unwrap(),
            second.predict(&feature_rows(60.0)).unwrap()
        );
        assert_eq!(first.positions().count(), POSITION_COUNT);
    }

    #[test]
    fn prediction_state_serializes_to_the_exact_public_name() {
        let samples = training_samples(10.0, 1.0);
        let model = PositionFingerprintModel::train(&samples, PositionFingerprintConfig::default())
            .unwrap();

        let position_json =
            serde_json::to_value(model.predict(&feature_rows(20.0)).unwrap()).unwrap();
        let unknown_json =
            serde_json::to_value(model.predict(&feature_rows(1_000.0)).unwrap()).unwrap();
        let ambiguous_model = PositionFingerprintModel::train(
            &training_samples(10.0, 4.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        let ambiguous_json =
            serde_json::to_value(ambiguous_model.predict(&feature_rows(5.0)).unwrap()).unwrap();

        assert_eq!(position_json["state"], "position");
        assert_eq!(unknown_json["state"], "unknown");
        assert_eq!(ambiguous_json["state"], "ambiguous");
    }

    #[test]
    fn deserialized_valid_model_validates_and_predicts() {
        let model = PositionFingerprintModel::train(
            &training_samples(10.0, 1.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        let encoded = serde_json::to_string(&model).unwrap();

        let decoded: PositionFingerprintModel = serde_json::from_str(&encoded).unwrap();

        decoded.validate().unwrap();
        assert!(matches!(
            decoded.predict(&feature_rows(30.0)).unwrap(),
            PositionFingerprintPrediction::Position { position, .. }
                if position.id == "P04"
        ));
    }

    #[test]
    fn corrupted_json_model_is_rejected_before_any_prediction_indexing() {
        let model = PositionFingerprintModel::train(
            &training_samples(10.0, 1.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        let pristine = serde_json::to_value(&model).unwrap();

        let mut no_prototypes = pristine.clone();
        no_prototypes["prototypes"] = serde_json::json!([]);
        let no_prototypes: PositionFingerprintModel =
            serde_json::from_value(no_prototypes).unwrap();
        let prediction_attempt =
            std::panic::catch_unwind(|| no_prototypes.predict(&feature_rows(0.0)));
        assert!(prediction_attempt.is_ok(), "corrupt model must not panic");
        assert!(matches!(
            prediction_attempt.unwrap(),
            Err(PositionFingerprintError::TooFewPositions {
                minimum: MINIMUM_POSITION_COUNT,
                actual: 0
            })
        ));

        let mut wrong_schema = pristine.clone();
        wrong_schema["schema_version"] = serde_json::json!(MODEL_SCHEMA_VERSION + 1);
        let wrong_schema: PositionFingerprintModel = serde_json::from_value(wrong_schema).unwrap();
        assert!(matches!(
            wrong_schema.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::SchemaVersion { .. })
        ));

        for parameter_name in [
            "ood_threshold_multiplier",
            "minimum_ood_threshold",
            "ambiguity_margin",
        ] {
            let mut wrong_parameter = pristine.clone();
            wrong_parameter[parameter_name] = serde_json::json!(0.25);
            let wrong_parameter: PositionFingerprintModel =
                serde_json::from_value(wrong_parameter).unwrap();
            assert!(matches!(
                wrong_parameter.predict(&feature_rows(0.0)),
                Err(PositionFingerprintError::AlgorithmParameterMismatch { name })
                    if name == parameter_name
            ));
        }

        let mut invalid_minimum = pristine.clone();
        invalid_minimum["config"]["minimum_samples_per_position"] = serde_json::json!(1);
        let invalid_minimum: PositionFingerprintModel =
            serde_json::from_value(invalid_minimum).unwrap();
        assert!(matches!(
            invalid_minimum.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::InvalidMinimumSamples { minimum: 1 })
        ));

        let mut unsorted = pristine.clone();
        unsorted["prototypes"].as_array_mut().unwrap().swap(0, 1);
        let unsorted: PositionFingerprintModel = serde_json::from_value(unsorted).unwrap();
        assert!(matches!(
            unsorted.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::NonIncreasingPositionId { .. })
        ));

        let mut duplicate_coordinates = pristine.clone();
        duplicate_coordinates["prototypes"][1]["position"]["coordinates_m"] =
            duplicate_coordinates["prototypes"][0]["position"]["coordinates_m"].clone();
        let duplicate_coordinates: PositionFingerprintModel =
            serde_json::from_value(duplicate_coordinates).unwrap();
        assert!(matches!(
            duplicate_coordinates.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::DuplicatePositionCoordinates { .. })
        ));

        let mut duplicate_floor_coordinates = pristine.clone();
        let floor_x =
            duplicate_floor_coordinates["prototypes"][0]["position"]["coordinates_m"][0].clone();
        let floor_z =
            duplicate_floor_coordinates["prototypes"][0]["position"]["coordinates_m"][2].clone();
        duplicate_floor_coordinates["prototypes"][1]["position"]["coordinates_m"][0] = floor_x;
        duplicate_floor_coordinates["prototypes"][1]["position"]["coordinates_m"][1] =
            serde_json::json!(1.75);
        duplicate_floor_coordinates["prototypes"][1]["position"]["coordinates_m"][2] = floor_z;
        let duplicate_floor_coordinates: PositionFingerprintModel =
            serde_json::from_value(duplicate_floor_coordinates).unwrap();
        assert!(matches!(
            duplicate_floor_coordinates.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::DuplicatePositionCoordinates { .. })
        ));

        let mut invalid_threshold = pristine.clone();
        invalid_threshold["prototypes"][0]["ood_threshold"] = serde_json::json!(0.0);
        let invalid_threshold: PositionFingerprintModel =
            serde_json::from_value(invalid_threshold).unwrap();
        assert!(matches!(
            invalid_threshold.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::InvalidOodThreshold { .. })
        ));

        let mut invalid_scale = pristine;
        invalid_scale["shared_scale"][0][0] = serde_json::json!(0.0);
        let invalid_scale: PositionFingerprintModel =
            serde_json::from_value(invalid_scale).unwrap();
        assert!(matches!(
            invalid_scale.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::InvalidSharedScale {
                receiver_index: 0,
                feature_index: 0
            })
        ));
    }

    #[test]
    fn malformed_json_matrix_dimensions_fail_during_deserialization() {
        let model = PositionFingerprintModel::train(
            &training_samples(10.0, 1.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        let pristine = serde_json::to_value(model).unwrap();

        let mut missing_receiver = pristine.clone();
        missing_receiver["prototypes"][0]["prototype"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(serde_json::from_value::<PositionFingerprintModel>(missing_receiver).is_err());

        let mut wrong_feature_count = pristine.clone();
        wrong_feature_count["prototypes"][0]["prototype"][0]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(serde_json::from_value::<PositionFingerprintModel>(wrong_feature_count).is_err());

        let mut wrong_scale_shape = pristine;
        wrong_scale_shape["shared_scale"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(serde_json::from_value::<PositionFingerprintModel>(wrong_scale_shape).is_err());
    }

    #[test]
    fn non_finite_in_memory_artifact_is_rejected() {
        let mut model = PositionFingerprintModel::train(
            &training_samples(10.0, 1.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        model.prototypes[0].prototype[2][7] = f64::NAN;
        assert!(matches!(
            model.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::NonFinitePrototype {
                receiver_index: 2,
                feature_index: 7,
                ..
            })
        ));

        let mut model = PositionFingerprintModel::train(
            &training_samples(10.0, 1.0),
            PositionFingerprintConfig::default(),
        )
        .unwrap();
        model.shared_scale[3][27] = f64::INFINITY;
        assert!(matches!(
            model.predict(&feature_rows(0.0)),
            Err(PositionFingerprintError::InvalidSharedScale {
                receiver_index: 3,
                feature_index: 27
            })
        ));
    }
}
