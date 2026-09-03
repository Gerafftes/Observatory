//! Static CSI fingerprint detector for D6 presence and coarse localization.
//!
//! D5 observes temporal motion. Once a person stops moving, that signal can
//! legitimately return to the empty-room level. D6 instead compares the
//! gain-normalized subcarrier *shape* with an explicitly recorded empty-room
//! reference. The same per-link anomaly ratio is also the observation used by
//! the geometry-based localization layer.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub(crate) const CALIBRATION_BLOCK: Duration = Duration::from_secs(10);
pub(crate) const MIN_CALIBRATION_BLOCKS: usize = 6;
pub(crate) const MIN_CALIBRATION_SAMPLES_PER_BLOCK: usize = 20;
pub(crate) const LIVE_WINDOW: Duration = Duration::from_secs(3);
pub(crate) const MIN_LIVE_SAMPLES: usize = 15;
pub(crate) const MIN_FRAME_RATE_HZ: f64 = 5.0;
pub(crate) const OBSERVATION_FRESHNESS: Duration = Duration::from_secs(5);
pub(crate) const ANOMALY_RATIO_THRESHOLD: f64 = 1.0;

const INPUT_GAP: Duration = Duration::from_secs(1);
const ROBUST_SIGMA_MULTIPLIER: f64 = 4.0;
const CALIBRATION_MAX_MARGIN: f64 = 1.25;
const MIN_DISTANCE_THRESHOLD: f64 = 1.0;
const DEAD_BIN_RELATIVE_FLOOR: f64 = 1e-3;
const MIN_UNSTABLE_BIN_MAD: f64 = 0.05;
const UNSTABLE_BIN_SIGMA_MULTIPLIER: f64 = 6.0;
const UNSTABLE_BIN_MEDIAN_MULTIPLIER: f64 = 2.0;
const MIN_BIN_SCALE: f64 = 0.01;
const MIN_STABLE_BINS: usize = 2;
const STANDARDIZED_RESIDUAL_CLIP: f64 = 6.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FingerprintReference {
    centroid: Vec<f64>,
    bin_mad: Vec<f64>,
    stable_bins: Vec<bool>,
    dead_bin_count: usize,
    unstable_bin_count: usize,
    bin_scale_floor: f64,
    pub(crate) distance_median: f64,
    pub(crate) distance_mad: f64,
    pub(crate) distance_threshold: f64,
    pub(crate) block_count: usize,
    pub(crate) sample_count: usize,
}

/// Serializable, detector-independent view of the calibrated empty-room shape.
///
/// Position feature extraction intentionally reuses D6's exact stable-bin
/// selection, gain normalization, and robust scale. It does not reuse D6's
/// presence threshold or vote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FingerprintProjectionReference {
    centroid: Vec<f64>,
    bin_mad: Vec<f64>,
    stable_bins: Vec<bool>,
    bin_scale_floor: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FingerprintProjection {
    pub(crate) normalized_shape: Vec<f64>,
    pub(crate) signed_residuals: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct FingerprintReferenceSummary {
    pub(crate) dimensions: usize,
    pub(crate) stable_dimensions: usize,
    pub(crate) masked_dimensions: usize,
    pub(crate) dead_dimensions: usize,
    pub(crate) unstable_dimensions: usize,
    pub(crate) stable_bin_mad_median: f64,
    pub(crate) bin_scale_floor: f64,
    pub(crate) distance_median: f64,
    pub(crate) distance_mad: f64,
    pub(crate) distance_threshold: f64,
    pub(crate) block_count: usize,
    pub(crate) sample_count: usize,
}

impl FingerprintReference {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let dimensions = self.centroid.len();
        if dimensions == 0
            || self.bin_mad.len() != dimensions
            || self.stable_bins.len() != dimensions
        {
            return Err("D6 reference dimensions do not match".to_string());
        }
        if self
            .centroid
            .iter()
            .chain(&self.bin_mad)
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err("D6 reference contains an invalid bin value".to_string());
        }
        if !self.bin_scale_floor.is_finite()
            || self.bin_scale_floor <= 0.0
            || !self.distance_median.is_finite()
            || self.distance_median < 0.0
            || !self.distance_mad.is_finite()
            || self.distance_mad < 0.0
            || !self.distance_threshold.is_finite()
            || self.distance_threshold <= 0.0
        {
            return Err("D6 reference contains an invalid distance scale".to_string());
        }
        if self.stable_bins.iter().filter(|stable| **stable).count() < MIN_STABLE_BINS {
            return Err(format!(
                "D6 reference needs at least {MIN_STABLE_BINS} stable subcarriers"
            ));
        }
        let minimum_samples = self
            .block_count
            .checked_mul(MIN_CALIBRATION_SAMPLES_PER_BLOCK)
            .ok_or_else(|| "D6 reference calibration sample count is too large".to_string())?;
        if self.block_count < MIN_CALIBRATION_BLOCKS || self.sample_count < minimum_samples {
            return Err("D6 reference does not contain enough calibration data".to_string());
        }
        Ok(())
    }

    fn summary(&self) -> FingerprintReferenceSummary {
        let stable_bin_mads: Vec<f64> = self
            .bin_mad
            .iter()
            .zip(&self.stable_bins)
            .filter_map(|(mad, stable)| stable.then_some(*mad))
            .collect();
        let stable_dimensions = stable_bin_mads.len();
        FingerprintReferenceSummary {
            dimensions: self.centroid.len(),
            stable_dimensions,
            masked_dimensions: self.centroid.len().saturating_sub(stable_dimensions),
            dead_dimensions: self.dead_bin_count,
            unstable_dimensions: self.unstable_bin_count,
            stable_bin_mad_median: median(&stable_bin_mads),
            bin_scale_floor: self.bin_scale_floor,
            distance_median: self.distance_median,
            distance_mad: self.distance_mad,
            distance_threshold: self.distance_threshold,
            block_count: self.block_count,
            sample_count: self.sample_count,
        }
    }

    fn projection_reference(&self) -> FingerprintProjectionReference {
        FingerprintProjectionReference {
            centroid: self.centroid.clone(),
            bin_mad: self.bin_mad.clone(),
            stable_bins: self.stable_bins.clone(),
            bin_scale_floor: self.bin_scale_floor,
        }
    }
}

impl FingerprintProjectionReference {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let dimensions = self.centroid.len();
        if dimensions == 0
            || self.bin_mad.len() != dimensions
            || self.stable_bins.len() != dimensions
        {
            return Err("D6 projection dimensions do not match".to_string());
        }
        if self
            .centroid
            .iter()
            .chain(&self.bin_mad)
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err("D6 projection contains an invalid reference value".to_string());
        }
        if !self.bin_scale_floor.is_finite() || self.bin_scale_floor <= 0.0 {
            return Err("D6 projection has an invalid scale floor".to_string());
        }
        if self.stable_bins.iter().filter(|stable| **stable).count() < MIN_STABLE_BINS {
            return Err(format!(
                "D6 projection needs at least {MIN_STABLE_BINS} stable subcarriers"
            ));
        }
        Ok(())
    }

    pub(crate) fn dimensions(&self) -> usize {
        self.centroid.len()
    }

    pub(crate) fn stable_bins(&self) -> &[bool] {
        &self.stable_bins
    }

    /// Project raw non-negative CSI amplitudes into D6's gain-normalized shape
    /// and a signed, robustly standardized empty-room residual.
    pub(crate) fn project(&self, amplitudes: &[f64]) -> Option<FingerprintProjection> {
        self.validate().ok()?;
        let normalized_shape = normalize_shape_with_mask(amplitudes, &self.stable_bins)?;
        let signed_residuals = normalized_shape
            .iter()
            .zip(&self.centroid)
            .zip(&self.bin_mad)
            .zip(&self.stable_bins)
            .map(|(((value, reference), mad), stable)| {
                if !stable {
                    return 0.0;
                }
                let scale = (1.4826 * mad).max(self.bin_scale_floor);
                ((value - reference) / scale)
                    .clamp(-STANDARDIZED_RESIDUAL_CLIP, STANDARDIZED_RESIDUAL_CLIP)
            })
            .collect();
        Some(FingerprintProjection {
            normalized_shape,
            signed_residuals,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct NodeFingerprintSnapshot {
    pub(crate) reference_ready: bool,
    pub(crate) calibration_samples: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reference: Option<FingerprintReferenceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) distance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) anomaly_ratio: Option<f64>,
    pub(crate) anomaly_strength: f64,
    pub(crate) vote: bool,
    pub(crate) accepted_frame_rate_hz: f64,
    pub(crate) accepted_frame_rate_samples: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_observation_ms: Option<u64>,
    pub(crate) observation_fresh: bool,
    pub(crate) evidence_ready: bool,
}

pub(crate) struct NodeFingerprintState {
    calibration_samples: Vec<(Instant, Vec<f64>)>,
    dimensions: Option<usize>,
    reference: Option<FingerprintReference>,
    live_window: VecDeque<(Instant, Vec<f64>)>,
    live_sum: Vec<f64>,
    live_started_at: Option<Instant>,
    distance: Option<f64>,
    anomaly_ratio: Option<f64>,
    vote: bool,
    last_observation_at: Option<Instant>,
    accepted_frame_rate_hz: f64,
    accepted_frame_rate_samples: u32,
}

impl Default for NodeFingerprintState {
    fn default() -> Self {
        Self {
            calibration_samples: Vec::new(),
            dimensions: None,
            reference: None,
            live_window: VecDeque::new(),
            live_sum: Vec::new(),
            live_started_at: None,
            distance: None,
            anomaly_ratio: None,
            vote: false,
            last_observation_at: None,
            accepted_frame_rate_hz: 0.0,
            accepted_frame_rate_samples: 0,
        }
    }
}

impl NodeFingerprintState {
    pub(crate) fn reset_for_calibration(&mut self) {
        self.calibration_samples.clear();
        self.dimensions = None;
        self.reference = None;
        self.reset_live_window();
        self.reset_observation_quality();
    }

    pub(crate) fn invalidate_reference(&mut self) {
        self.reset_for_calibration();
    }

    pub(crate) fn observe_calibration(&mut self, now: Instant, amplitudes: &[f64]) {
        let Some(amplitudes) = self.accept_calibration_amplitudes(amplitudes) else {
            return;
        };
        self.observe_accepted_frame(now);
        self.calibration_samples.push((now, amplitudes));
    }

    pub(crate) fn build_reference(
        &self,
        started_at: Instant,
        ended_at: Instant,
    ) -> Result<FingerprintReference, String> {
        let dimensions = self
            .dimensions
            .ok_or_else(|| "no valid fingerprint samples were collected".to_string())?;
        let calibration_duration = ended_at.saturating_duration_since(started_at);
        let complete_block_count =
            (calibration_duration.as_nanos() / CALIBRATION_BLOCK.as_nanos()) as usize;
        if complete_block_count < MIN_CALIBRATION_BLOCKS {
            return Err(format!(
                "need at least {MIN_CALIBRATION_BLOCKS} complete fingerprint blocks"
            ));
        }

        let mut sums = vec![vec![0.0; dimensions]; complete_block_count];
        let mut counts = vec![0usize; complete_block_count];
        for (timestamp, shape) in &self.calibration_samples {
            if *timestamp < started_at || *timestamp >= ended_at || shape.len() != dimensions {
                continue;
            }
            let block_index = (timestamp.saturating_duration_since(started_at).as_nanos()
                / CALIBRATION_BLOCK.as_nanos()) as usize;
            if block_index >= complete_block_count {
                continue;
            }
            for (sum, value) in sums[block_index].iter_mut().zip(shape) {
                *sum += *value;
            }
            counts[block_index] += 1;
        }

        let qualified_blocks: Vec<(Vec<f64>, usize)> = sums
            .into_iter()
            .zip(counts)
            .filter_map(|(mut sum, count)| {
                if count < MIN_CALIBRATION_SAMPLES_PER_BLOCK {
                    return None;
                }
                for value in &mut sum {
                    *value /= count as f64;
                }
                Some((sum, count))
            })
            .collect();
        if qualified_blocks.len() < MIN_CALIBRATION_BLOCKS {
            return Err(format!(
                "only {} complete fingerprint blocks had at least {} samples",
                qualified_blocks.len(),
                MIN_CALIBRATION_SAMPLES_PER_BLOCK,
            ));
        }

        let raw_block_means: Vec<Vec<f64>> = qualified_blocks
            .iter()
            .map(|(block, _)| block.clone())
            .collect();
        let raw_bin_medians = per_bin_medians(&raw_block_means, dimensions);
        let strongest_bin = raw_bin_medians.iter().copied().fold(0.0, f64::max);
        let dead_bin_floor = strongest_bin * DEAD_BIN_RELATIVE_FLOOR;
        let dead_bins: Vec<bool> = raw_bin_medians
            .iter()
            .map(|median| *median <= dead_bin_floor || *median <= f64::EPSILON)
            .collect();
        let non_dead_bins: Vec<bool> = dead_bins.iter().map(|dead| !dead).collect();
        if non_dead_bins.iter().filter(|active| **active).count() < MIN_STABLE_BINS {
            return Err("too few non-dead fingerprint subcarriers".to_string());
        }

        let preliminary_shapes: Vec<Vec<f64>> = raw_block_means
            .iter()
            .filter_map(|block| normalize_shape_with_mask(block, &non_dead_bins))
            .collect();
        if preliminary_shapes.len() != raw_block_means.len() {
            return Err("fingerprint blocks could not be gain-normalized".to_string());
        }
        let preliminary_centroid = per_bin_medians(&preliminary_shapes, dimensions);
        let preliminary_bin_mad =
            per_bin_mads(&preliminary_shapes, &preliminary_centroid, dimensions);
        let non_dead_mads: Vec<f64> = preliminary_bin_mad
            .iter()
            .zip(&non_dead_bins)
            .filter_map(|(mad, active)| active.then_some(*mad))
            .collect();
        let median_bin_mad = median(&non_dead_mads);
        let mad_of_bin_mads = median_absolute_deviation(&non_dead_mads, median_bin_mad);
        let robust_unstable_cutoff = (median_bin_mad
            + UNSTABLE_BIN_SIGMA_MULTIPLIER * 1.4826 * mad_of_bin_mads)
            .max(MIN_UNSTABLE_BIN_MAD);
        let relative_unstable_cutoff =
            (median_bin_mad * UNSTABLE_BIN_MEDIAN_MULTIPLIER).max(MIN_UNSTABLE_BIN_MAD);
        let unstable_bin_cutoff = robust_unstable_cutoff.min(relative_unstable_cutoff);
        let unstable_bins: Vec<bool> = preliminary_bin_mad
            .iter()
            .zip(&non_dead_bins)
            .map(|(mad, active)| *active && *mad > unstable_bin_cutoff)
            .collect();
        let stable_bins: Vec<bool> = dead_bins
            .iter()
            .zip(&unstable_bins)
            .map(|(dead, unstable)| !dead && !unstable)
            .collect();
        let stable_bin_count = stable_bins.iter().filter(|stable| **stable).count();
        if stable_bin_count < MIN_STABLE_BINS {
            return Err(format!(
                "only {stable_bin_count} stable fingerprint subcarriers remain"
            ));
        }

        let block_means: Vec<Vec<f64>> = raw_block_means
            .iter()
            .filter_map(|block| normalize_shape_with_mask(block, &stable_bins))
            .collect();
        if block_means.len() != raw_block_means.len() {
            return Err("stable fingerprint bins could not be gain-normalized".to_string());
        }
        let centroid: Vec<f64> = (0..dimensions)
            .map(|index| {
                let values: Vec<f64> = block_means.iter().map(|block| block[index]).collect();
                median(&values)
            })
            .collect();
        let bin_mad = per_bin_mads(&block_means, &centroid, dimensions);
        let positive_stable_mads: Vec<f64> = bin_mad
            .iter()
            .zip(&stable_bins)
            .filter_map(|(mad, stable)| (*stable && *mad > 0.0).then_some(*mad))
            .collect();
        let bin_scale_floor = if positive_stable_mads.is_empty() {
            MIN_BIN_SCALE
        } else {
            (1.4826 * median(&positive_stable_mads) * 0.25).max(MIN_BIN_SCALE)
        };
        let distances: Vec<f64> = block_means
            .iter()
            .map(|block| {
                robust_shape_distance(block, &centroid, &bin_mad, &stable_bins, bin_scale_floor)
            })
            .collect();
        let distance_median = median(&distances);
        let distance_mad = median_absolute_deviation(&distances, distance_median);
        let robust_threshold = distance_median + ROBUST_SIGMA_MULTIPLIER * 1.4826 * distance_mad;
        let calibration_max = distances.iter().copied().fold(0.0, f64::max);
        let distance_threshold = robust_threshold
            .max(calibration_max * CALIBRATION_MAX_MARGIN)
            .max(MIN_DISTANCE_THRESHOLD);

        Ok(FingerprintReference {
            centroid,
            bin_mad,
            stable_bins,
            dead_bin_count: dead_bins.iter().filter(|dead| **dead).count(),
            unstable_bin_count: unstable_bins.iter().filter(|unstable| **unstable).count(),
            bin_scale_floor,
            distance_median,
            distance_mad,
            distance_threshold,
            block_count: block_means.len(),
            sample_count: qualified_blocks.iter().map(|(_, count)| count).sum(),
        })
    }

    pub(crate) fn install_reference(&mut self, reference: FingerprintReference) {
        self.dimensions = Some(reference.centroid.len());
        self.reference = Some(reference);
        self.calibration_samples.clear();
        self.reset_live_window();
        self.reset_observation_quality();
    }

    pub(crate) fn observe_live(&mut self, now: Instant, amplitudes: &[f64]) {
        let Some(reference_dimensions) = self.reference.as_ref().map(|r| r.centroid.len()) else {
            self.reset_live_window();
            return;
        };
        if amplitudes.len() != reference_dimensions {
            self.invalidate_reference();
            return;
        }
        let Some(shape) = self
            .reference
            .as_ref()
            .and_then(|reference| normalize_shape_with_mask(amplitudes, &reference.stable_bins))
        else {
            return;
        };

        let input_was_interrupted = self.observe_accepted_frame(now);
        if input_was_interrupted {
            self.reset_live_window();
        }
        self.live_started_at.get_or_insert(now);
        if self.live_sum.is_empty() {
            self.live_sum.resize(reference_dimensions, 0.0);
        }
        for (sum, value) in self.live_sum.iter_mut().zip(&shape) {
            *sum += *value;
        }
        self.live_window.push_back((now, shape));

        let cutoff = now.checked_sub(LIVE_WINDOW).unwrap_or(now);
        while self
            .live_window
            .front()
            .is_some_and(|(timestamp, _)| *timestamp < cutoff)
        {
            if let Some((_, removed)) = self.live_window.pop_front() {
                for (sum, value) in self.live_sum.iter_mut().zip(removed) {
                    *sum -= value;
                }
            }
        }

        let has_full_window = self
            .live_started_at
            .is_some_and(|started| now.saturating_duration_since(started) >= LIVE_WINDOW);
        if !has_full_window || self.live_window.len() < MIN_LIVE_SAMPLES {
            self.distance = None;
            self.anomaly_ratio = None;
            self.vote = false;
            return;
        }

        let count = self.live_window.len() as f64;
        let live_mean: Vec<f64> = self.live_sum.iter().map(|sum| sum / count).collect();
        let reference = self
            .reference
            .as_ref()
            .expect("reference was checked before the live window update");
        let distance = robust_shape_distance(
            &live_mean,
            &reference.centroid,
            &reference.bin_mad,
            &reference.stable_bins,
            reference.bin_scale_floor,
        );
        let anomaly_ratio = distance / reference.distance_threshold;
        self.distance = Some(distance);
        self.anomaly_ratio = Some(anomaly_ratio);
        self.vote = anomaly_ratio > ANOMALY_RATIO_THRESHOLD;
    }

    pub(crate) fn reference_ready(&self) -> bool {
        self.reference.is_some()
    }

    pub(crate) fn projection_reference(&self) -> Option<FingerprintProjectionReference> {
        self.reference
            .as_ref()
            .map(FingerprintReference::projection_reference)
    }

    pub(crate) fn vote(&self) -> bool {
        self.vote
    }

    pub(crate) fn anomaly_ratio(&self) -> Option<f64> {
        self.anomaly_ratio
    }

    pub(crate) fn anomaly_strength(&self) -> f64 {
        self.anomaly_ratio
            .map(anomaly_strength_from_ratio)
            .unwrap_or(0.0)
    }

    pub(crate) fn observation_fresh(&self, now: Instant) -> bool {
        self.last_observation_at
            .is_some_and(|seen| now.saturating_duration_since(seen) <= OBSERVATION_FRESHNESS)
    }

    pub(crate) fn observation_ready(&self, now: Instant) -> bool {
        self.observation_fresh(now)
            && self.accepted_frame_rate_samples >= 5
            && self.accepted_frame_rate_hz >= MIN_FRAME_RATE_HZ
    }

    pub(crate) fn evidence_ready(&self, now: Instant) -> bool {
        self.reference_ready()
            && self.distance.is_some()
            && self.anomaly_ratio.is_some()
            && self.observation_ready(now)
    }

    pub(crate) fn snapshot(&self, now: Instant) -> NodeFingerprintSnapshot {
        NodeFingerprintSnapshot {
            reference_ready: self.reference_ready(),
            calibration_samples: self.calibration_samples.len(),
            reference: self.reference.as_ref().map(FingerprintReference::summary),
            distance: self.distance,
            anomaly_ratio: self.anomaly_ratio,
            anomaly_strength: self.anomaly_strength(),
            vote: self.vote,
            accepted_frame_rate_hz: if self.accepted_frame_rate_samples >= 5 {
                self.accepted_frame_rate_hz
            } else {
                0.0
            },
            accepted_frame_rate_samples: self.accepted_frame_rate_samples,
            last_observation_ms: self
                .last_observation_at
                .map(|seen| now.saturating_duration_since(seen).as_millis() as u64),
            observation_fresh: self.observation_fresh(now),
            evidence_ready: self.evidence_ready(now),
        }
    }

    #[cfg(test)]
    pub(crate) fn install_reference_for_test(&mut self, amplitudes: &[f64]) -> Result<(), String> {
        let centroid = normalize_shape(amplitudes)
            .ok_or_else(|| "test fingerprint reference is invalid".to_string())?;
        let strongest_bin = centroid.iter().copied().fold(0.0, f64::max);
        let dead_bin_floor = strongest_bin * DEAD_BIN_RELATIVE_FLOOR;
        let stable_bins: Vec<bool> = centroid
            .iter()
            .map(|value| *value > dead_bin_floor && *value > f64::EPSILON)
            .collect();
        let stable_bin_count = stable_bins.iter().filter(|stable| **stable).count();
        if stable_bin_count < MIN_STABLE_BINS {
            return Err(format!(
                "test reference needs at least {MIN_STABLE_BINS} stable subcarriers"
            ));
        }
        let centroid = normalize_shape_with_mask(amplitudes, &stable_bins)
            .ok_or_else(|| "test fingerprint reference cannot be normalized".to_string())?;
        self.install_reference(FingerprintReference {
            bin_mad: vec![0.0; centroid.len()],
            dead_bin_count: centroid.len() - stable_bin_count,
            unstable_bin_count: 0,
            stable_bins,
            bin_scale_floor: MIN_BIN_SCALE,
            centroid,
            distance_median: 0.0,
            distance_mad: 0.0,
            distance_threshold: MIN_DISTANCE_THRESHOLD,
            block_count: MIN_CALIBRATION_BLOCKS,
            sample_count: MIN_CALIBRATION_BLOCKS * MIN_CALIBRATION_SAMPLES_PER_BLOCK,
        });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn reference_for_test(&self) -> Option<FingerprintReference> {
        self.reference.clone()
    }

    fn accept_calibration_amplitudes(&mut self, amplitudes: &[f64]) -> Option<Vec<f64>> {
        normalize_shape(amplitudes)?;
        match self.dimensions {
            Some(dimensions) if dimensions != amplitudes.len() => None,
            None => {
                self.dimensions = Some(amplitudes.len());
                Some(amplitudes.to_vec())
            }
            Some(_) => Some(amplitudes.to_vec()),
        }
    }

    fn observe_accepted_frame(&mut self, now: Instant) -> bool {
        let mut input_was_interrupted = false;
        if let Some(previous) = self.last_observation_at {
            let Some(delta) = now.checked_duration_since(previous) else {
                return false;
            };
            let dt_seconds = delta.as_secs_f64();
            if dt_seconds > 0.0 && delta < INPUT_GAP {
                let instantaneous = 1.0 / dt_seconds;
                self.accepted_frame_rate_hz = if self.accepted_frame_rate_samples == 0 {
                    instantaneous
                } else {
                    self.accepted_frame_rate_hz
                        + (instantaneous - self.accepted_frame_rate_hz) / 8.0
                };
                self.accepted_frame_rate_samples =
                    self.accepted_frame_rate_samples.saturating_add(1);
            } else if delta >= INPUT_GAP {
                self.accepted_frame_rate_hz = 0.0;
                self.accepted_frame_rate_samples = 0;
                input_was_interrupted = true;
            }
        }
        self.last_observation_at = Some(now);
        input_was_interrupted
    }

    fn reset_observation_quality(&mut self) {
        self.last_observation_at = None;
        self.accepted_frame_rate_hz = 0.0;
        self.accepted_frame_rate_samples = 0;
    }

    fn reset_live_window(&mut self) {
        self.live_window.clear();
        self.live_sum.clear();
        self.live_started_at = None;
        self.distance = None;
        self.anomaly_ratio = None;
        self.vote = false;
    }
}

fn normalize_shape(amplitudes: &[f64]) -> Option<Vec<f64>> {
    let mask = vec![true; amplitudes.len()];
    normalize_shape_with_mask(amplitudes, &mask)
}

fn normalize_shape_with_mask(amplitudes: &[f64], active_bins: &[bool]) -> Option<Vec<f64>> {
    if amplitudes.is_empty()
        || amplitudes.len() != active_bins.len()
        || amplitudes
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    let active_count = active_bins.iter().filter(|active| **active).count();
    if active_count == 0 {
        return None;
    }
    let rms = (amplitudes
        .iter()
        .zip(active_bins)
        .filter_map(|(value, active)| active.then_some(value * value))
        .sum::<f64>()
        / active_count as f64)
        .sqrt();
    if rms <= f64::EPSILON {
        return None;
    }
    Some(
        amplitudes
            .iter()
            .zip(active_bins)
            .map(|(value, active)| if *active { value / rms } else { 0.0 })
            .collect(),
    )
}

fn shape_distance(left: &[f64], right: &[f64]) -> f64 {
    if left.is_empty() || left.len() != right.len() {
        return f64::INFINITY;
    }
    (left
        .iter()
        .zip(right)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f64>()
        / left.len() as f64)
        .sqrt()
}

fn robust_shape_distance(
    shape: &[f64],
    centroid: &[f64],
    bin_mad: &[f64],
    stable_bins: &[bool],
    bin_scale_floor: f64,
) -> f64 {
    if shape.is_empty()
        || shape.len() != centroid.len()
        || shape.len() != bin_mad.len()
        || shape.len() != stable_bins.len()
    {
        return f64::INFINITY;
    }
    let mut squared_distance = 0.0;
    let mut stable_count = 0usize;
    for (((value, reference), mad), stable) in
        shape.iter().zip(centroid).zip(bin_mad).zip(stable_bins)
    {
        if !stable {
            continue;
        }
        let absolute_deviation = (value - reference).abs();
        let scale = (1.4826 * mad).max(bin_scale_floor);
        let standardized_deviation = absolute_deviation / scale;
        squared_distance += standardized_deviation * standardized_deviation;
        stable_count += 1;
    }
    if stable_count == 0 {
        return f64::INFINITY;
    }
    (squared_distance / stable_count as f64).sqrt()
}

fn anomaly_strength_from_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() {
        return 0.0;
    }
    let excess = (ratio - ANOMALY_RATIO_THRESHOLD).max(0.0);
    excess / (2.0 + excess)
}

fn per_bin_medians(samples: &[Vec<f64>], dimensions: usize) -> Vec<f64> {
    (0..dimensions)
        .map(|index| {
            let values: Vec<f64> = samples.iter().map(|sample| sample[index]).collect();
            median(&values)
        })
        .collect()
}

fn per_bin_mads(samples: &[Vec<f64>], medians: &[f64], dimensions: usize) -> Vec<f64> {
    (0..dimensions)
        .map(|index| {
            let deviations: Vec<f64> = samples
                .iter()
                .map(|sample| (sample[index] - medians[index]).abs())
                .collect();
            median(&deviations)
        })
        .collect()
}

fn median_absolute_deviation(values: &[f64], center: f64) -> f64 {
    let deviations: Vec<f64> = values.iter().map(|value| (value - center).abs()).collect();
    median(&deviations)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn populate_reference(
        state: &mut NodeFingerprintState,
        started: Instant,
        amplitudes: &[f64],
    ) -> FingerprintReference {
        populate_reference_with(state, started, |_, _| amplitudes.to_vec())
    }

    fn populate_reference_with(
        state: &mut NodeFingerprintState,
        started: Instant,
        amplitudes: impl Fn(usize, usize) -> Vec<f64>,
    ) -> FingerprintReference {
        for block in 0..MIN_CALIBRATION_BLOCKS {
            for sample in 0..MIN_CALIBRATION_SAMPLES_PER_BLOCK {
                state.observe_calibration(
                    started
                        + Duration::from_secs(block as u64 * 10)
                        + Duration::from_millis(sample as u64 * 100 + 1),
                    &amplitudes(block, sample),
                );
            }
        }
        state
            .build_reference(
                started,
                started + Duration::from_secs(MIN_CALIBRATION_BLOCKS as u64 * 10),
            )
            .unwrap()
    }

    fn observe_complete_live_window(
        state: &mut NodeFingerprintState,
        started: Instant,
        amplitudes: &[f64],
    ) {
        for sample in 0..=40 {
            state.observe_live(started + Duration::from_millis(sample * 100), amplitudes);
        }
    }

    #[test]
    fn normalization_ignores_uniform_gain() {
        let first = normalize_shape(&[1.0, 2.0, 4.0, 2.0]).unwrap();
        let gained = normalize_shape(&[3.5, 7.0, 14.0, 7.0]).unwrap();
        assert!(shape_distance(&first, &gained) < 1e-12);
    }

    #[test]
    fn projection_reuses_d6_shape_and_keeps_residual_direction() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference(&mut state, started, &[1.0, 1.0, 1.0, 1.0]);
        state.install_reference(reference);
        let projection = state.projection_reference().unwrap();

        let positive = projection.project(&[1.0, 2.0, 1.0, 1.0]).unwrap();
        let negative = projection.project(&[1.0, 0.25, 1.0, 1.0]).unwrap();
        let gained = projection.project(&[5.0, 5.0, 5.0, 5.0]).unwrap();

        assert!(positive.signed_residuals[1] > 0.0);
        assert!(negative.signed_residuals[1] < 0.0);
        assert!(gained
            .signed_residuals
            .iter()
            .all(|residual| residual.abs() < 1e-12));
        assert_eq!(projection.dimensions(), 4);
    }

    #[test]
    fn projection_preserves_the_d6_stable_bin_mask() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference(&mut state, started, &[0.0, 1.0, 2.0, 1.0, 0.0]);
        state.install_reference(reference);
        let projection = state.projection_reference().unwrap();

        let projected = projection.project(&[100.0, 1.0, 2.0, 1.0, 100.0]).unwrap();

        assert_eq!(projection.stable_bins(), &[false, true, true, true, false]);
        assert_eq!(projected.normalized_shape[0], 0.0);
        assert_eq!(projected.normalized_shape[4], 0.0);
        assert_eq!(projected.signed_residuals[0], 0.0);
        assert_eq!(projected.signed_residuals[4], 0.0);
    }

    #[test]
    fn anomaly_strength_preserves_order_above_ratio_three() {
        let ratio_four = anomaly_strength_from_ratio(4.0);
        let ratio_ten = anomaly_strength_from_ratio(10.0);

        assert!(ratio_four > 0.0);
        assert!(ratio_ten > ratio_four);
        assert!(ratio_ten < 1.0);
    }

    #[test]
    fn static_shape_change_remains_detectable_after_motion_stops() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference(&mut state, started, &[1.0, 2.0, 1.0, 2.0]);
        state.install_reference(reference);

        let live_started = started + Duration::from_secs(70);
        for sample in 0..=40 {
            state.observe_live(
                live_started + Duration::from_millis(sample * 100),
                &[1.0, 4.0, 2.0, 1.0],
            );
        }

        assert!(state.evidence_ready(live_started + Duration::from_secs(4)));
        assert!(state.vote());
        assert!(state.anomaly_ratio().unwrap() > ANOMALY_RATIO_THRESHOLD);
    }

    #[test]
    fn uniform_gain_change_does_not_vote() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference(&mut state, started, &[1.0, 2.0, 1.0, 2.0]);
        state.install_reference(reference);

        let live_started = started + Duration::from_secs(70);
        observe_complete_live_window(&mut state, live_started, &[5.0, 10.0, 5.0, 10.0]);

        assert!(state.evidence_ready(live_started + Duration::from_secs(4)));
        assert!(!state.vote());
        assert!(state.anomaly_ratio().unwrap() < 1e-9);
    }

    #[test]
    fn dead_subcarriers_are_masked_from_live_distance() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference(&mut state, started, &[0.0, 1.0, 2.0, 1.0, 0.0]);
        let summary = reference.summary();
        assert_eq!(summary.dead_dimensions, 2);
        assert_eq!(summary.unstable_dimensions, 0);
        assert_eq!(summary.stable_dimensions, 3);
        assert_eq!(summary.masked_dimensions, 2);
        state.install_reference(reference);

        let live_started = started + Duration::from_secs(70);
        observe_complete_live_window(&mut state, live_started, &[50.0, 1.0, 2.0, 1.0, 80.0]);

        assert!(state.evidence_ready(live_started + Duration::from_secs(4)));
        assert!(!state.vote());
        assert!(state.anomaly_ratio().unwrap() < 1e-9);
    }

    #[test]
    fn unstable_subcarrier_is_masked_from_live_distance() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference_with(&mut state, started, |block, _| {
            let unstable = if block % 2 == 0 { 0.2 } else { 5.0 };
            vec![unstable, 1.0, 2.0, 1.0]
        });
        let summary = reference.summary();
        assert_eq!(summary.dead_dimensions, 0);
        assert_eq!(summary.unstable_dimensions, 1);
        assert_eq!(summary.stable_dimensions, 3);
        assert_eq!(summary.masked_dimensions, 1);
        state.install_reference(reference);

        let live_started = started + Duration::from_secs(70);
        observe_complete_live_window(&mut state, live_started, &[20.0, 1.0, 2.0, 1.0]);

        assert!(state.evidence_ready(live_started + Duration::from_secs(4)));
        assert!(!state.vote());
        assert!(state.anomaly_ratio().unwrap() < 1e-9);
    }

    #[test]
    fn positive_and_negative_shape_changes_both_vote() {
        fn anomaly_ratio_for(live_amplitudes: &[f64]) -> f64 {
            let started = Instant::now();
            let mut state = NodeFingerprintState::default();
            let reference = populate_reference(&mut state, started, &[1.0, 1.0, 1.0, 1.0]);
            state.install_reference(reference);
            let live_started = started + Duration::from_secs(70);
            observe_complete_live_window(&mut state, live_started, live_amplitudes);
            assert!(state.vote());
            state.anomaly_ratio().unwrap()
        }

        let positive_ratio = anomaly_ratio_for(&[1.0, 2.0, 1.0, 1.0]);
        let negative_ratio = anomaly_ratio_for(&[1.0, 0.25, 1.0, 1.0]);

        assert!(positive_ratio > ANOMALY_RATIO_THRESHOLD);
        assert!(negative_ratio > ANOMALY_RATIO_THRESHOLD);
    }

    #[test]
    fn test_reference_helper_skips_the_calibration_wait() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        state
            .install_reference_for_test(&[1.0, 2.0, 1.0, 2.0])
            .unwrap();

        observe_complete_live_window(&mut state, started, &[1.0, 4.0, 2.0, 1.0]);

        assert!(state.reference_ready());
        assert!(state.vote());
    }

    #[test]
    fn incomplete_calibration_is_rejected() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        for sample in 0..MIN_CALIBRATION_SAMPLES_PER_BLOCK {
            state.observe_calibration(
                started + Duration::from_millis(sample as u64 * 100 + 1),
                &[1.0, 2.0],
            );
        }

        let result = state.build_reference(started, started + Duration::from_secs(60));
        assert!(result.is_err());
    }

    #[test]
    fn changed_subcarrier_grid_invalidates_a_live_reference() {
        let started = Instant::now();
        let mut state = NodeFingerprintState::default();
        let reference = populate_reference(&mut state, started, &[1.0, 2.0, 1.0, 2.0]);
        state.install_reference(reference);

        state.observe_live(
            started + Duration::from_secs(70),
            &[1.0, 2.0, 3.0, 4.0, 5.0],
        );

        assert!(!state.reference_ready());
    }
}
