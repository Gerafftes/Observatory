//! Honest, geometry-only coarse localization for the fixed 1-TX / multi-RX setup.
//!
//! This module turns per-receiver D6 anomaly strengths into a relative 2D
//! likelihood map.  For a candidate floor point, each TX/RX link contributes a
//! Fresnel-like weight derived from its excess path:
//!
//! `|TX - point| + |point - RX| - |TX - RX|`.
//!
//! The observed link-strength signature is compared with the geometric
//! signature at every grid point.  This is useful as a coarse link-likelihood
//! visualization, but it is not calibrated ranging, angle-of-arrival, or
//! validated metric positioning.  Indoor multipath and the small number of
//! links can leave several positions equally plausible.

use serde::{Deserialize, Serialize};

pub(crate) const MIN_USABLE_LINKS: usize = 3;
pub(crate) const MIN_ACTIVE_LINKS: usize = 2;

const MIN_LINK_LENGTH_M: f64 = 1e-6;
const MIN_SIGNATURE_NORM: f64 = 1e-12;
const UNCERTAINTY_MASS: f64 = 0.68;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct FloorPoint {
    pub(crate) x: f64,
    pub(crate) z: f64,
}

impl FloorPoint {
    fn is_finite(self) -> bool {
        self.x.is_finite() && self.z.is_finite()
    }

    fn distance_to(self, other: Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.z - other.z).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct FloorBounds {
    pub(crate) min_x: f64,
    pub(crate) max_x: f64,
    pub(crate) min_z: f64,
    pub(crate) max_z: f64,
}

impl FloorBounds {
    fn is_valid(self) -> bool {
        self.min_x.is_finite()
            && self.max_x.is_finite()
            && self.min_z.is_finite()
            && self.max_z.is_finite()
            && self.max_x > self.min_x
            && self.max_z > self.min_z
    }

    fn contains(self, point: FloorPoint) -> bool {
        point.x >= self.min_x
            && point.x <= self.max_x
            && point.z >= self.min_z
            && point.z <= self.max_z
    }
}

/// One D6 observation and its fixed receiver geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseLinkObservation {
    pub(crate) node_id: String,
    pub(crate) receiver: FloorPoint,
    /// D6 anomaly strength, expected in `[0, 1]`.
    pub(crate) anomaly_strength: f64,
    /// Whether this link has a completed empty-room D6 reference.
    pub(crate) reference_ready: bool,
    /// Whether the reference, live window, frame rate, and freshness checks pass.
    pub(crate) evidence_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseLocalizationConfig {
    pub(crate) grid_columns: usize,
    pub(crate) grid_rows: usize,
    /// Width of the excess-path kernel.  This is a visualization parameter,
    /// not a measured ranging resolution.
    pub(crate) excess_path_scale_m: f64,
    pub(crate) active_strength_threshold: f64,
    /// Candidate cells this close to a radio are excluded from person
    /// occupancy.  This prevents radio endpoints from becoming trivial modes.
    pub(crate) device_exclusion_radius_m: f64,
    /// Controls how sharply geometric signatures are compared.
    pub(crate) signature_sigma: f64,
    /// Minimum relative map concentration required before emitting a position.
    pub(crate) minimum_confidence: f64,
    /// Maximum accepted radius containing 68% of the relative map mass.
    pub(crate) maximum_radius_68_m: f64,
    /// Minimum distance from the primary peak when looking for a competing peak.
    pub(crate) secondary_peak_min_distance_m: f64,
    /// Maximum accepted likelihood ratio between the competing and primary peak.
    pub(crate) maximum_secondary_peak_ratio: f64,
}

impl Default for CoarseLocalizationConfig {
    fn default() -> Self {
        Self {
            grid_columns: 25,
            grid_rows: 21,
            excess_path_scale_m: 0.35,
            active_strength_threshold: 0.05,
            device_exclusion_radius_m: 0.25,
            signature_sigma: 0.18,
            minimum_confidence: 0.01,
            maximum_radius_68_m: 0.9,
            secondary_peak_min_distance_m: 0.75,
            maximum_secondary_peak_ratio: 0.5,
        }
    }
}

impl CoarseLocalizationConfig {
    fn is_valid(self) -> bool {
        self.grid_columns >= 2
            && self.grid_rows >= 2
            && self.excess_path_scale_m.is_finite()
            && self.excess_path_scale_m > 0.0
            && self.active_strength_threshold.is_finite()
            && (0.0..=1.0).contains(&self.active_strength_threshold)
            && self.device_exclusion_radius_m.is_finite()
            && self.device_exclusion_radius_m >= 0.0
            && self.signature_sigma.is_finite()
            && self.signature_sigma > 0.0
            && self.minimum_confidence.is_finite()
            && (0.0..=1.0).contains(&self.minimum_confidence)
            && self.maximum_radius_68_m.is_finite()
            && self.maximum_radius_68_m > 0.0
            && self.secondary_peak_min_distance_m.is_finite()
            && self.secondary_peak_min_distance_m > 0.0
            && self.maximum_secondary_peak_ratio.is_finite()
            && (0.0..=1.0).contains(&self.maximum_secondary_peak_ratio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CoarseLocalizationStatus {
    Unavailable,
    Uncalibrated,
    InsufficientEvidence,
    Coarse,
}

/// Row-major normalized relative likelihood map.
///
/// `values[row * columns + column]` sums to one.  The values are not calibrated
/// probabilities of a person being at a metric position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseProbabilityMap {
    pub(crate) columns: usize,
    pub(crate) rows: usize,
    pub(crate) origin: FloorPoint,
    pub(crate) cell_size_x_m: f64,
    pub(crate) cell_size_z_m: f64,
    pub(crate) values: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseSecondaryPeak {
    pub(crate) position: FloorPoint,
    pub(crate) distance_from_primary_m: f64,
    pub(crate) secondary_to_primary_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseAmbiguity {
    /// Distance around the primary peak excluded from the competing-peak search.
    pub(crate) minimum_peak_separation_m: f64,
    /// Largest accepted competing-to-primary likelihood ratio.
    pub(crate) maximum_secondary_peak_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) secondary_peak: Option<CoarseSecondaryPeak>,
    pub(crate) exceeds_limit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseUncertainty {
    /// Radius around the maximum-likelihood cell containing 68% of map mass.
    pub(crate) radius_68_m: f64,
    /// Largest accepted 68%-mass radius before the position is withheld.
    pub(crate) maximum_radius_68_m: f64,
    pub(crate) radius_exceeds_limit: bool,
    /// Map-weighted RMS distance from the maximum-likelihood X coordinate.
    pub(crate) rms_x_m: f64,
    /// Map-weighted RMS distance from the maximum-likelihood Z coordinate.
    pub(crate) rms_z_m: f64,
    pub(crate) ambiguity: CoarseAmbiguity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CoarseLocalizationEstimate {
    pub(crate) status: CoarseLocalizationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) position: Option<FloorPoint>,
    /// Relative concentration of this map in `[0, 1]`, not validated accuracy.
    pub(crate) confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) uncertainty: Option<CoarseUncertainty>,
    pub(crate) geometry_links: usize,
    pub(crate) calibrated_links: usize,
    pub(crate) usable_links: usize,
    pub(crate) active_links: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) probability_map: Option<CoarseProbabilityMap>,
}

impl CoarseLocalizationEstimate {
    pub(crate) fn unavailable() -> Self {
        Self::without_position(CoarseLocalizationStatus::Unavailable, 0, 0, 0, 0)
    }

    fn without_position(
        status: CoarseLocalizationStatus,
        geometry_links: usize,
        calibrated_links: usize,
        usable_links: usize,
        active_links: usize,
    ) -> Self {
        Self {
            status,
            position: None,
            confidence: 0.0,
            uncertainty: None,
            geometry_links,
            calibrated_links,
            usable_links,
            active_links,
            probability_map: None,
        }
    }
}

/// Estimate a coarse floor position from D6 per-link anomaly strengths.
///
/// A position is deliberately withheld when presence is false, fewer than
/// three independent links are usable, fewer than two links are active, or the
/// resulting map is effectively flat.
#[must_use]
pub(crate) fn estimate_coarse_location(
    bounds: FloorBounds,
    transmitter: FloorPoint,
    observations: &[CoarseLinkObservation],
    presence: bool,
    config: CoarseLocalizationConfig,
) -> CoarseLocalizationEstimate {
    if !bounds.is_valid()
        || !transmitter.is_finite()
        || !bounds.contains(transmitter)
        || !config.is_valid()
    {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::Unavailable,
            0,
            0,
            0,
            0,
        );
    }

    let geometry_links = independent_geometry_links(transmitter, observations);
    let geometry_count = geometry_links.len();
    if geometry_count < MIN_USABLE_LINKS {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::Unavailable,
            geometry_count,
            0,
            0,
            0,
        );
    }

    let calibrated_count = geometry_links
        .iter()
        .filter(|link| link.reference_ready)
        .count();
    if calibrated_count < MIN_USABLE_LINKS {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::Uncalibrated,
            geometry_count,
            calibrated_count,
            0,
            0,
        );
    }

    let usable_links: Vec<&CoarseLinkObservation> = geometry_links
        .into_iter()
        .filter(|link| {
            link.reference_ready
                && link.evidence_ready
                && link.anomaly_strength.is_finite()
                && (0.0..=1.0).contains(&link.anomaly_strength)
        })
        .collect();
    let usable_count = usable_links.len();
    if usable_count < MIN_USABLE_LINKS {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::Unavailable,
            geometry_count,
            calibrated_count,
            usable_count,
            0,
        );
    }

    let active_count = usable_links
        .iter()
        .filter(|link| link.anomaly_strength >= config.active_strength_threshold)
        .count();
    if !presence || active_count < MIN_ACTIVE_LINKS {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::InsufficientEvidence,
            geometry_count,
            calibrated_count,
            usable_count,
            active_count,
        );
    }

    let observed_signature: Vec<f64> = usable_links
        .iter()
        .map(|link| link.anomaly_strength)
        .collect();
    let Some(observed_norm) = vector_norm(&observed_signature) else {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::InsufficientEvidence,
            geometry_count,
            calibrated_count,
            usable_count,
            active_count,
        );
    };

    let cell_size_x_m = (bounds.max_x - bounds.min_x) / (config.grid_columns - 1) as f64;
    let cell_size_z_m = (bounds.max_z - bounds.min_z) / (config.grid_rows - 1) as f64;
    let mut cell_points = Vec::with_capacity(config.grid_columns * config.grid_rows);
    let mut likelihoods = Vec::with_capacity(config.grid_columns * config.grid_rows);

    for row in 0..config.grid_rows {
        let z = bounds.min_z + row as f64 * cell_size_z_m;
        for column in 0..config.grid_columns {
            let point = FloorPoint {
                x: bounds.min_x + column as f64 * cell_size_x_m,
                z,
            };
            cell_points.push(point);

            if is_inside_radio_exclusion(
                point,
                transmitter,
                &usable_links,
                config.device_exclusion_radius_m,
            ) {
                likelihoods.push(0.0);
                continue;
            }

            let predicted_signature: Vec<f64> = usable_links
                .iter()
                .map(|link| {
                    fresnel_link_weight(
                        transmitter,
                        link.receiver,
                        point,
                        config.excess_path_scale_m,
                    )
                })
                .collect();
            let Some(predicted_norm) = vector_norm(&predicted_signature) else {
                likelihoods.push(0.0);
                continue;
            };
            let cosine_similarity = observed_signature
                .iter()
                .zip(&predicted_signature)
                .map(|(observed, predicted)| observed * predicted)
                .sum::<f64>()
                / (observed_norm * predicted_norm);
            let angular_residual = 1.0 - cosine_similarity.clamp(0.0, 1.0);
            let likelihood = (-angular_residual / (2.0 * config.signature_sigma.powi(2))).exp();
            likelihoods.push(likelihood);
        }
    }

    let likelihood_sum = likelihoods.iter().sum::<f64>();
    if !likelihood_sum.is_finite() || likelihood_sum <= f64::EPSILON {
        return CoarseLocalizationEstimate::without_position(
            CoarseLocalizationStatus::InsufficientEvidence,
            geometry_count,
            calibrated_count,
            usable_count,
            active_count,
        );
    }
    for value in &mut likelihoods {
        *value /= likelihood_sum;
    }

    let best_index = likelihoods
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
        .expect("a valid grid always contains cells");
    let best_position = cell_points[best_index];
    let confidence = relative_concentration(&likelihoods);
    let probability_map = CoarseProbabilityMap {
        columns: config.grid_columns,
        rows: config.grid_rows,
        origin: FloorPoint {
            x: bounds.min_x,
            z: bounds.min_z,
        },
        cell_size_x_m,
        cell_size_z_m,
        values: likelihoods,
    };

    let ambiguity = ambiguity_outside_primary_peak(
        best_index,
        &cell_points,
        &probability_map.values,
        config.secondary_peak_min_distance_m,
        config.maximum_secondary_peak_ratio,
    );
    let uncertainty = uncertainty_around(
        best_position,
        &cell_points,
        &probability_map.values,
        config.maximum_radius_68_m,
        ambiguity,
    );
    if confidence < config.minimum_confidence
        || uncertainty.radius_exceeds_limit
        || uncertainty.ambiguity.exceeds_limit
    {
        return CoarseLocalizationEstimate {
            status: CoarseLocalizationStatus::InsufficientEvidence,
            position: None,
            confidence,
            uncertainty: Some(uncertainty),
            geometry_links: geometry_count,
            calibrated_links: calibrated_count,
            usable_links: usable_count,
            active_links: active_count,
            probability_map: Some(probability_map),
        };
    }

    CoarseLocalizationEstimate {
        status: CoarseLocalizationStatus::Coarse,
        position: Some(best_position),
        confidence,
        uncertainty: Some(uncertainty),
        geometry_links: geometry_count,
        calibrated_links: calibrated_count,
        usable_links: usable_count,
        active_links: active_count,
        probability_map: Some(probability_map),
    }
}

/// Excess path in meters.  It is zero on the direct TX/RX line segment and
/// positive away from it.
#[must_use]
pub(crate) fn excess_path_m(
    transmitter: FloorPoint,
    receiver: FloorPoint,
    point: FloorPoint,
) -> f64 {
    (transmitter.distance_to(point) + point.distance_to(receiver)
        - transmitter.distance_to(receiver))
    .max(0.0)
}

/// Fresnel-like link sensitivity in `[0, 1]` from excess-path geometry.
///
/// The scale is configurable because this is a coarse indoor prior, not an
/// estimate of RF range resolution.
#[must_use]
pub(crate) fn fresnel_link_weight(
    transmitter: FloorPoint,
    receiver: FloorPoint,
    point: FloorPoint,
    excess_path_scale_m: f64,
) -> f64 {
    if !transmitter.is_finite()
        || !receiver.is_finite()
        || !point.is_finite()
        || !excess_path_scale_m.is_finite()
        || excess_path_scale_m <= 0.0
        || transmitter.distance_to(receiver) <= MIN_LINK_LENGTH_M
    {
        return 0.0;
    }
    let normalized_excess = excess_path_m(transmitter, receiver, point) / excess_path_scale_m;
    (-0.5 * normalized_excess.powi(2)).exp()
}

fn independent_geometry_links<'a>(
    transmitter: FloorPoint,
    observations: &'a [CoarseLinkObservation],
) -> Vec<&'a CoarseLinkObservation> {
    let mut links: Vec<&CoarseLinkObservation> = Vec::new();
    for observation in observations {
        if observation.node_id.trim().is_empty()
            || !observation.receiver.is_finite()
            || transmitter.distance_to(observation.receiver) <= MIN_LINK_LENGTH_M
            || links
                .iter()
                .any(|existing| existing.node_id == observation.node_id)
            || links.iter().any(|existing| {
                existing.receiver.distance_to(observation.receiver) <= MIN_LINK_LENGTH_M
            })
        {
            continue;
        }
        links.push(observation);
    }
    links
}

fn is_inside_radio_exclusion(
    point: FloorPoint,
    transmitter: FloorPoint,
    links: &[&CoarseLinkObservation],
    exclusion_radius_m: f64,
) -> bool {
    if exclusion_radius_m <= 0.0 {
        return false;
    }
    point.distance_to(transmitter) < exclusion_radius_m
        || links
            .iter()
            .any(|link| point.distance_to(link.receiver) < exclusion_radius_m)
}

fn vector_norm(values: &[f64]) -> Option<f64> {
    let norm = values.iter().map(|value| value * value).sum::<f64>().sqrt();
    (norm.is_finite() && norm > MIN_SIGNATURE_NORM).then_some(norm)
}

fn relative_concentration(probabilities: &[f64]) -> f64 {
    let valid_cell_count = probabilities
        .iter()
        .filter(|probability| **probability > 0.0)
        .count();
    if valid_cell_count <= 1 {
        return 0.0;
    }
    let entropy = -probabilities
        .iter()
        .filter(|probability| **probability > 0.0)
        .map(|probability| probability * probability.ln())
        .sum::<f64>();
    // Radio-exclusion cells have exactly zero mass. They are an occupancy
    // prior, not measured localization evidence, so they must not make an
    // otherwise uniform valid map look more confident.
    let maximum_entropy = (valid_cell_count as f64).ln();
    (1.0 - entropy / maximum_entropy).clamp(0.0, 1.0)
}

fn ambiguity_outside_primary_peak(
    best_index: usize,
    cell_points: &[FloorPoint],
    probabilities: &[f64],
    minimum_peak_separation_m: f64,
    maximum_secondary_peak_ratio: f64,
) -> CoarseAmbiguity {
    let primary_position = cell_points[best_index];
    let primary_likelihood = probabilities[best_index];
    let secondary_peak = cell_points
        .iter()
        .zip(probabilities)
        .filter_map(|(point, probability)| {
            let distance_from_primary_m = primary_position.distance_to(*point);
            (distance_from_primary_m >= minimum_peak_separation_m && *probability > 0.0)
                .then_some((*point, distance_from_primary_m, *probability))
        })
        .max_by(|left, right| left.2.total_cmp(&right.2))
        .map(
            |(position, distance_from_primary_m, secondary_likelihood)| CoarseSecondaryPeak {
                position,
                distance_from_primary_m,
                secondary_to_primary_ratio: secondary_likelihood / primary_likelihood,
            },
        );
    let exceeds_limit = secondary_peak
        .is_some_and(|peak| peak.secondary_to_primary_ratio > maximum_secondary_peak_ratio);

    CoarseAmbiguity {
        minimum_peak_separation_m,
        maximum_secondary_peak_ratio,
        secondary_peak,
        exceeds_limit,
    }
}

fn uncertainty_around(
    best_position: FloorPoint,
    cell_points: &[FloorPoint],
    probabilities: &[f64],
    maximum_radius_68_m: f64,
    ambiguity: CoarseAmbiguity,
) -> CoarseUncertainty {
    let mut distance_mass: Vec<(f64, f64)> = cell_points
        .iter()
        .zip(probabilities)
        .map(|(point, probability)| (best_position.distance_to(*point), *probability))
        .collect();
    distance_mass.sort_by(|left, right| left.0.total_cmp(&right.0));

    let mut cumulative_mass = 0.0;
    let mut radius_68_m = 0.0;
    for (distance, probability) in distance_mass {
        cumulative_mass += probability;
        radius_68_m = distance;
        if cumulative_mass >= UNCERTAINTY_MASS {
            break;
        }
    }

    let rms_x_m = cell_points
        .iter()
        .zip(probabilities)
        .map(|(point, probability)| probability * (point.x - best_position.x).powi(2))
        .sum::<f64>()
        .sqrt();
    let rms_z_m = cell_points
        .iter()
        .zip(probabilities)
        .map(|(point, probability)| probability * (point.z - best_position.z).powi(2))
        .sum::<f64>()
        .sqrt();

    CoarseUncertainty {
        radius_68_m,
        maximum_radius_68_m,
        radius_exceeds_limit: radius_68_m > maximum_radius_68_m,
        rms_x_m,
        rms_z_m,
        ambiguity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> FloorBounds {
        FloorBounds {
            min_x: 0.0,
            max_x: 4.0,
            min_z: 0.0,
            max_z: 3.0,
        }
    }

    fn transmitter() -> FloorPoint {
        FloorPoint { x: 0.4, z: 1.5 }
    }

    fn receiver_points() -> [FloorPoint; 4] {
        [
            FloorPoint { x: 3.9, z: 0.2 },
            FloorPoint { x: 3.9, z: 1.0 },
            FloorPoint { x: 3.9, z: 2.0 },
            FloorPoint { x: 3.9, z: 2.8 },
        ]
    }

    fn fixed_room_bounds() -> FloorBounds {
        FloorBounds {
            min_x: 0.0,
            max_x: 4.02,
            min_z: 0.0,
            max_z: 3.44,
        }
    }

    fn fixed_room_transmitter() -> FloorPoint {
        FloorPoint { x: 1.51, z: 0.39 }
    }

    fn fixed_room_receivers() -> [FloorPoint; 4] {
        [
            FloorPoint { x: 0.0, z: 0.28 },
            FloorPoint { x: 4.02, z: 0.97 },
            FloorPoint { x: 0.0, z: 2.11 },
            FloorPoint { x: 4.02, z: 2.46 },
        ]
    }

    fn fixed_room_grid_point(column: usize, row: usize) -> FloorPoint {
        FloorPoint {
            x: column as f64 * 4.02 / 19.0,
            z: row as f64 * 3.44 / 19.0,
        }
    }

    fn observations_with_strengths(strengths: [f64; 4]) -> Vec<CoarseLinkObservation> {
        receiver_points()
            .into_iter()
            .zip(strengths)
            .enumerate()
            .map(
                |(index, (receiver, anomaly_strength))| CoarseLinkObservation {
                    node_id: format!("rx{}", index + 1),
                    receiver,
                    anomaly_strength,
                    reference_ready: true,
                    evidence_ready: true,
                },
            )
            .collect()
    }

    fn synthetic_observations_for_geometry(
        transmitter: FloorPoint,
        receivers: [FloorPoint; 4],
        target: FloorPoint,
        config: CoarseLocalizationConfig,
    ) -> Vec<CoarseLinkObservation> {
        let mut strengths = receivers.map(|receiver| {
            fresnel_link_weight(transmitter, receiver, target, config.excess_path_scale_m)
        });
        let maximum = strengths.iter().copied().fold(0.0, f64::max);
        for strength in &mut strengths {
            *strength /= maximum;
        }
        receivers
            .into_iter()
            .zip(strengths)
            .enumerate()
            .map(
                |(index, (receiver, anomaly_strength))| CoarseLinkObservation {
                    node_id: format!("rx{}", index + 1),
                    receiver,
                    anomaly_strength,
                    reference_ready: true,
                    evidence_ready: true,
                },
            )
            .collect()
    }

    fn synthetic_observations(
        target: FloorPoint,
        config: CoarseLocalizationConfig,
    ) -> Vec<CoarseLinkObservation> {
        synthetic_observations_for_geometry(transmitter(), receiver_points(), target, config)
    }

    #[test]
    fn excess_path_is_zero_on_link_and_positive_away_from_it() {
        let tx = FloorPoint { x: 0.0, z: 0.0 };
        let rx = FloorPoint { x: 4.0, z: 0.0 };
        let on_link = FloorPoint { x: 2.0, z: 0.0 };
        let away = FloorPoint { x: 2.0, z: 1.0 };

        assert!(excess_path_m(tx, rx, on_link) < 1e-12);
        assert!(excess_path_m(tx, rx, away) > 0.0);
        assert_eq!(fresnel_link_weight(tx, rx, on_link, 0.35), 1.0);
        assert!(fresnel_link_weight(tx, rx, away, 0.35) < 1.0);
    }

    #[test]
    fn synthetic_link_signature_finds_a_coarse_off_center_target() {
        let config = CoarseLocalizationConfig {
            minimum_confidence: 0.0,
            maximum_radius_68_m: 10.0,
            maximum_secondary_peak_ratio: 1.0,
            ..CoarseLocalizationConfig::default()
        };
        let target = FloorPoint { x: 2.7, z: 0.9 };
        let observations = synthetic_observations(target, config);

        let estimate =
            estimate_coarse_location(bounds(), transmitter(), &observations, true, config);

        assert_eq!(estimate.status, CoarseLocalizationStatus::Coarse);
        let position = estimate.position.expect("coarse position");
        assert!(
            position.distance_to(target) <= 0.55,
            "position={position:?}, target={target:?}"
        );
        assert!(
            position.distance_to(target) < position.distance_to(FloorPoint { x: 2.0, z: 1.5 }),
            "an off-center signature must not be replaced by the room center"
        );
        let map = estimate.probability_map.expect("normalized map");
        assert!((map.values.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(estimate.uncertainty.expect("uncertainty").radius_68_m > 0.0);
    }

    #[test]
    fn fixed_room_unambiguous_signature_passes_uncertainty_gates() {
        let config = CoarseLocalizationConfig {
            grid_columns: 20,
            grid_rows: 20,
            ..CoarseLocalizationConfig::default()
        };
        let target = FloorPoint { x: 0.75, z: 0.75 };
        let observations = synthetic_observations_for_geometry(
            fixed_room_transmitter(),
            fixed_room_receivers(),
            target,
            config,
        );

        let estimate = estimate_coarse_location(
            fixed_room_bounds(),
            fixed_room_transmitter(),
            &observations,
            true,
            config,
        );

        assert_eq!(estimate.status, CoarseLocalizationStatus::Coarse);
        let position = estimate.position.expect("coarse position");
        assert!(position.distance_to(target) <= 0.15);
        let uncertainty = estimate.uncertainty.expect("uncertainty diagnostics");
        assert!(!uncertainty.radius_exceeds_limit);
        assert!(!uncertainty.ambiguity.exceeds_limit);
    }

    #[test]
    fn fixed_room_proportional_far_signature_is_withheld_as_ambiguous() {
        let config = CoarseLocalizationConfig {
            grid_columns: 20,
            grid_rows: 20,
            maximum_radius_68_m: 10.0,
            ..CoarseLocalizationConfig::default()
        };
        let target = fixed_room_grid_point(11, 2);
        let observations = synthetic_observations_for_geometry(
            fixed_room_transmitter(),
            fixed_room_receivers(),
            target,
            config,
        );

        let estimate = estimate_coarse_location(
            fixed_room_bounds(),
            fixed_room_transmitter(),
            &observations,
            true,
            config,
        );

        assert_eq!(
            estimate.status,
            CoarseLocalizationStatus::InsufficientEvidence
        );
        assert!(estimate.position.is_none());
        assert!(estimate.probability_map.is_some());
        let uncertainty = estimate.uncertainty.expect("uncertainty diagnostics");
        assert!(!uncertainty.radius_exceeds_limit);
        assert!(uncertainty.ambiguity.exceeds_limit);
        let secondary = uncertainty
            .ambiguity
            .secondary_peak
            .expect("competing peak");
        assert!(secondary.distance_from_primary_m >= config.secondary_peak_min_distance_m);
        assert!(secondary.secondary_to_primary_ratio > 0.99);
    }

    #[test]
    fn excessive_uncertainty_radius_withholds_position_but_keeps_diagnostics() {
        let config = CoarseLocalizationConfig {
            grid_columns: 20,
            grid_rows: 20,
            maximum_radius_68_m: 0.1,
            maximum_secondary_peak_ratio: 1.0,
            ..CoarseLocalizationConfig::default()
        };
        let target = FloorPoint { x: 0.75, z: 0.75 };
        let observations = synthetic_observations_for_geometry(
            fixed_room_transmitter(),
            fixed_room_receivers(),
            target,
            config,
        );

        let estimate = estimate_coarse_location(
            fixed_room_bounds(),
            fixed_room_transmitter(),
            &observations,
            true,
            config,
        );

        assert_eq!(
            estimate.status,
            CoarseLocalizationStatus::InsufficientEvidence
        );
        assert!(estimate.position.is_none());
        assert!(estimate.probability_map.is_some());
        let uncertainty = estimate.uncertainty.expect("uncertainty diagnostics");
        assert!(uncertainty.radius_exceeds_limit);
        assert!(uncertainty.radius_68_m > config.maximum_radius_68_m);
        assert!(!uncertainty.ambiguity.exceeds_limit);
    }

    #[test]
    fn presence_false_never_emits_a_position() {
        let config = CoarseLocalizationConfig::default();
        let observations = synthetic_observations(FloorPoint { x: 2.5, z: 1.0 }, config);

        let estimate =
            estimate_coarse_location(bounds(), transmitter(), &observations, false, config);

        assert_eq!(
            estimate.status,
            CoarseLocalizationStatus::InsufficientEvidence
        );
        assert!(estimate.position.is_none());
        assert!(estimate.probability_map.is_none());
    }

    #[test]
    fn fewer_than_three_usable_links_are_unavailable() {
        let config = CoarseLocalizationConfig::default();
        let mut observations = synthetic_observations(FloorPoint { x: 2.5, z: 1.0 }, config);
        observations[2].evidence_ready = false;
        observations[3].evidence_ready = false;

        let estimate =
            estimate_coarse_location(bounds(), transmitter(), &observations, true, config);

        assert_eq!(estimate.status, CoarseLocalizationStatus::Unavailable);
        assert_eq!(estimate.usable_links, 2);
        assert!(estimate.position.is_none());
    }

    #[test]
    fn fewer_than_two_active_links_are_insufficient() {
        let config = CoarseLocalizationConfig::default();
        let observations = observations_with_strengths([0.8, 0.01, 0.0, 0.0]);

        let estimate =
            estimate_coarse_location(bounds(), transmitter(), &observations, true, config);

        assert_eq!(
            estimate.status,
            CoarseLocalizationStatus::InsufficientEvidence
        );
        assert_eq!(estimate.active_links, 1);
        assert!(estimate.position.is_none());
    }

    #[test]
    fn incomplete_references_report_uncalibrated() {
        let config = CoarseLocalizationConfig::default();
        let mut observations = synthetic_observations(FloorPoint { x: 2.5, z: 1.0 }, config);
        observations[2].reference_ready = false;
        observations[3].reference_ready = false;

        let estimate =
            estimate_coarse_location(bounds(), transmitter(), &observations, true, config);

        assert_eq!(estimate.status, CoarseLocalizationStatus::Uncalibrated);
        assert_eq!(estimate.calibrated_links, 2);
        assert!(estimate.position.is_none());
    }

    #[test]
    fn excluded_cells_do_not_create_false_confidence() {
        let probabilities = [0.0, 0.25, 0.25, 0.25, 0.25, 0.0];

        assert!(relative_concentration(&probabilities) < 1e-12);
    }
}
