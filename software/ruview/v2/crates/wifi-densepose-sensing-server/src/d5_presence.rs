//! Experimental D5 still-presence detector.
//!
//! D4 remains responsible for obvious movement. D5 adds an explicitly
//! calibrated, per-receiver empty-room reference for the harder
//! `present_still`/`absent` decision. The detector is intentionally inactive
//! until the classification-calibration API has completed successfully.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub(crate) const CALIBRATION_BLOCK: Duration = Duration::from_secs(10);
pub(crate) const MIN_CALIBRATION_BLOCKS: usize = 6;
pub(crate) const MIN_CALIBRATION_SAMPLES_PER_BLOCK: usize = 20;
pub(crate) const RECOMMENDED_CALIBRATION_SECONDS: u64 = 60;
pub(crate) const LIVE_WINDOW: Duration = Duration::from_secs(10);
pub(crate) const MIN_LIVE_SAMPLES: usize = 5;
pub(crate) const MIN_FRAME_RATE_HZ: f64 = 5.0;
pub(crate) const OBSERVATION_FRESHNESS: Duration = Duration::from_secs(5);
pub(crate) const ROBUST_SCALE_FLOOR: f64 = 0.005;
pub(crate) const VOTE_Z_THRESHOLD: f64 = 1.0;
pub(crate) const REQUIRED_VOTES: usize = 2;
pub(crate) const MIN_FRESH_REFERENCES: usize = 3;
pub(crate) const STATE_PERSISTENCE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CalibrationPhase {
    Uncalibrated,
    Collecting,
    Ready,
}

impl CalibrationPhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Uncalibrated => "uncalibrated",
            Self::Collecting => "collecting",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct PresenceReference {
    pub(crate) median: f64,
    pub(crate) mad: f64,
    pub(crate) scale: f64,
    pub(crate) block_count: usize,
    pub(crate) sample_count: usize,
}

impl PresenceReference {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.median.is_finite() || !self.mad.is_finite() || !self.scale.is_finite() {
            return Err("D5 reference contains a non-finite value".to_string());
        }
        if self.mad < 0.0 || self.scale <= 0.0 {
            return Err("D5 reference has an invalid robust scale".to_string());
        }
        let minimum_samples = self
            .block_count
            .checked_mul(MIN_CALIBRATION_SAMPLES_PER_BLOCK)
            .ok_or_else(|| "D5 reference calibration sample count is too large".to_string())?;
        if self.block_count < MIN_CALIBRATION_BLOCKS || self.sample_count < minimum_samples {
            return Err("D5 reference does not contain enough calibration data".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct NodePresenceSnapshot {
    pub(crate) reference_ready: bool,
    pub(crate) calibration_samples: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reference: Option<PresenceReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rolling_mean_10s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) z_score: Option<f64>,
    pub(crate) vote: bool,
    pub(crate) accepted_frame_rate_hz: f64,
    pub(crate) accepted_frame_rate_samples: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_observation_ms: Option<u64>,
    pub(crate) observation_fresh: bool,
    pub(crate) evidence_ready: bool,
}

pub(crate) struct NodePresenceState {
    calibration_samples: Vec<(Instant, f64)>,
    reference: Option<PresenceReference>,
    live_window: VecDeque<(Instant, f64)>,
    live_started_at: Option<Instant>,
    live_sum: f64,
    rolling_mean: Option<f64>,
    z_score: Option<f64>,
    vote: bool,
    last_observation_at: Option<Instant>,
    accepted_frame_rate_hz: f64,
    accepted_frame_rate_samples: u32,
}

impl Default for NodePresenceState {
    fn default() -> Self {
        Self {
            calibration_samples: Vec::new(),
            reference: None,
            live_window: VecDeque::new(),
            live_started_at: None,
            live_sum: 0.0,
            rolling_mean: None,
            z_score: None,
            vote: false,
            last_observation_at: None,
            accepted_frame_rate_hz: 0.0,
            accepted_frame_rate_samples: 0,
        }
    }
}

impl NodePresenceState {
    pub(crate) fn reset_for_calibration(&mut self) {
        self.calibration_samples.clear();
        self.reference = None;
        self.reset_live_window();
        self.reset_observation_quality();
    }

    pub(crate) fn invalidate_reference(&mut self) {
        self.reset_for_calibration();
    }

    pub(crate) fn observe_calibration(&mut self, now: Instant, score: f64) {
        if score.is_finite() {
            self.observe_accepted_score(now);
            self.calibration_samples.push((now, score));
        }
    }

    pub(crate) fn build_reference(
        &self,
        started_at: Instant,
        ended_at: Instant,
    ) -> Result<PresenceReference, String> {
        let calibration_duration = ended_at.saturating_duration_since(started_at);
        let complete_block_count =
            (calibration_duration.as_nanos() / CALIBRATION_BLOCK.as_nanos()) as usize;
        if complete_block_count < MIN_CALIBRATION_BLOCKS {
            return Err(format!(
                "need at least {MIN_CALIBRATION_BLOCKS} complete 10-second blocks"
            ));
        }

        let mut sums = vec![0.0; complete_block_count];
        let mut counts = vec![0usize; complete_block_count];
        for &(timestamp, score) in &self.calibration_samples {
            if timestamp < started_at || timestamp >= ended_at {
                continue;
            }
            let block_index = (timestamp.saturating_duration_since(started_at).as_nanos()
                / CALIBRATION_BLOCK.as_nanos()) as usize;
            if block_index < complete_block_count {
                sums[block_index] += score;
                counts[block_index] += 1;
            }
        }

        let block_means: Vec<f64> = sums
            .into_iter()
            .zip(counts)
            .filter_map(|(sum, count)| {
                (count >= MIN_CALIBRATION_SAMPLES_PER_BLOCK).then_some(sum / count as f64)
            })
            .collect();
        if block_means.len() < MIN_CALIBRATION_BLOCKS {
            return Err(format!(
                "only {} complete blocks had at least {} samples",
                block_means.len(),
                MIN_CALIBRATION_SAMPLES_PER_BLOCK,
            ));
        }

        let reference_median = median(&block_means);
        let deviations: Vec<f64> = block_means
            .iter()
            .map(|value| (value - reference_median).abs())
            .collect();
        let mad = median(&deviations);
        let scale = (1.4826 * mad).max(ROBUST_SCALE_FLOOR);

        Ok(PresenceReference {
            median: reference_median,
            mad,
            scale,
            block_count: block_means.len(),
            sample_count: self.calibration_samples.len(),
        })
    }

    pub(crate) fn install_reference(&mut self, reference: PresenceReference) {
        self.reference = Some(reference);
        self.calibration_samples.clear();
        self.reset_live_window();
        self.reset_observation_quality();
    }

    pub(crate) fn observe_live(&mut self, now: Instant, score: f64) {
        let Some(reference) = self.reference else {
            self.reset_live_window();
            return;
        };
        if !score.is_finite() {
            return;
        }
        let input_was_interrupted = self.observe_accepted_score(now);
        if input_was_interrupted {
            self.reset_live_window();
        }

        if self
            .live_window
            .back()
            .is_some_and(|(previous, _)| now.saturating_duration_since(*previous) > LIVE_WINDOW)
        {
            self.reset_live_window();
        }
        self.live_started_at.get_or_insert(now);
        self.live_window.push_back((now, score));
        self.live_sum += score;
        let cutoff = now.checked_sub(LIVE_WINDOW).unwrap_or(now);
        while self
            .live_window
            .front()
            .is_some_and(|(timestamp, _)| *timestamp < cutoff)
        {
            if let Some((_, removed_score)) = self.live_window.pop_front() {
                self.live_sum -= removed_score;
            }
        }

        let has_full_window = self
            .live_started_at
            .is_some_and(|started| now.saturating_duration_since(started) >= LIVE_WINDOW);
        if !has_full_window || self.live_window.len() < MIN_LIVE_SAMPLES {
            self.rolling_mean = None;
            self.z_score = None;
            self.vote = false;
            return;
        }

        let rolling_mean = self.live_sum / self.live_window.len() as f64;
        let z_score = (rolling_mean - reference.median) / reference.scale;
        self.rolling_mean = Some(rolling_mean);
        self.z_score = Some(z_score);
        self.vote = z_score > VOTE_Z_THRESHOLD;
    }

    pub(crate) fn reference_ready(&self) -> bool {
        self.reference.is_some()
    }

    pub(crate) fn live_ready(&self) -> bool {
        self.rolling_mean.is_some()
    }

    pub(crate) fn vote(&self) -> bool {
        self.vote
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
        self.reference_ready() && self.live_ready() && self.observation_ready(now)
    }

    pub(crate) fn snapshot(&self, now: Instant) -> NodePresenceSnapshot {
        NodePresenceSnapshot {
            reference_ready: self.reference_ready(),
            calibration_samples: self.calibration_samples.len(),
            reference: self.reference,
            rolling_mean_10s: self.rolling_mean,
            z_score: self.z_score,
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

    fn observe_accepted_score(&mut self, now: Instant) -> bool {
        let mut input_was_interrupted = false;
        if let Some(previous) = self.last_observation_at {
            let Some(delta) = now.checked_duration_since(previous) else {
                return false;
            };
            let dt_seconds = delta.as_secs_f64();
            if dt_seconds > 0.0 && dt_seconds < 1.0 {
                let instantaneous = 1.0 / dt_seconds;
                self.accepted_frame_rate_hz = if self.accepted_frame_rate_samples == 0 {
                    instantaneous
                } else {
                    self.accepted_frame_rate_hz
                        + (instantaneous - self.accepted_frame_rate_hz) / 8.0
                };
                self.accepted_frame_rate_samples =
                    self.accepted_frame_rate_samples.saturating_add(1);
            } else if dt_seconds >= 1.0 {
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
        self.live_started_at = None;
        self.live_sum = 0.0;
        self.rolling_mean = None;
        self.z_score = None;
        self.vote = false;
    }

    #[cfg(test)]
    pub(crate) fn install_reference_for_test(&mut self, median: f64, scale: f64) {
        self.install_reference(PresenceReference {
            median,
            mad: 0.0,
            scale,
            block_count: MIN_CALIBRATION_BLOCKS,
            sample_count: 100,
        });
    }
}

pub(crate) struct PresenceFusionState {
    phase: CalibrationPhase,
    calibration_started_at: Option<Instant>,
    calibrated_at: Option<Instant>,
    present: bool,
    candidate: Option<(bool, Instant)>,
}

impl Default for PresenceFusionState {
    fn default() -> Self {
        Self {
            phase: CalibrationPhase::Uncalibrated,
            calibration_started_at: None,
            calibrated_at: None,
            present: false,
            candidate: None,
        }
    }
}

impl PresenceFusionState {
    pub(crate) fn phase(&self) -> CalibrationPhase {
        self.phase
    }

    pub(crate) fn start_calibration(&mut self, now: Instant) -> Result<(), &'static str> {
        if self.phase == CalibrationPhase::Collecting {
            return Err("classification calibration is already collecting");
        }
        self.phase = CalibrationPhase::Collecting;
        self.calibration_started_at = Some(now);
        self.calibrated_at = None;
        self.present = false;
        self.candidate = None;
        Ok(())
    }

    pub(crate) fn calibration_started_at(&self) -> Option<Instant> {
        self.calibration_started_at
    }

    pub(crate) fn finish_calibration(&mut self, now: Instant) {
        self.phase = CalibrationPhase::Ready;
        self.calibration_started_at = None;
        self.calibrated_at = Some(now);
        self.present = false;
        self.candidate = None;
    }

    /// Restore a previously persisted D5/D6 calibration after a server
    /// restart. This is deliberately separate from `finish_calibration`,
    /// which is reserved for a fresh measurement run.
    pub(crate) fn restore_ready(&mut self, now: Instant) {
        self.phase = CalibrationPhase::Ready;
        self.calibration_started_at = None;
        self.calibrated_at = Some(now);
        self.present = false;
        self.candidate = None;
    }

    pub(crate) fn calibrated_at(&self) -> Option<Instant> {
        self.calibrated_at
    }

    pub(crate) fn update(&mut self, raw_present: bool, evidence_ready: bool, now: Instant) -> bool {
        if self.phase != CalibrationPhase::Ready || !evidence_ready {
            self.candidate = None;
            self.present = false;
            return false;
        }

        if raw_present == self.present {
            self.candidate = None;
            return self.present;
        }

        match self.candidate {
            Some((candidate, since)) if candidate == raw_present => {
                if now.saturating_duration_since(since) >= STATE_PERSISTENCE {
                    self.present = raw_present;
                    self.candidate = None;
                }
            }
            _ => self.candidate = Some((raw_present, now)),
        }
        self.present
    }

    pub(crate) fn present(&self) -> bool {
        self.present
    }

    #[cfg(test)]
    pub(crate) fn mark_ready_for_test(&mut self, now: Instant) {
        self.phase = CalibrationPhase::Ready;
        self.calibrated_at = Some(now);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_uses_complete_block_means_and_robust_scale_floor() {
        let started = Instant::now();
        let mut node = NodePresenceState::default();
        for block in 0..6 {
            for sample in 0..MIN_CALIBRATION_SAMPLES_PER_BLOCK {
                node.observe_calibration(
                    started
                        + Duration::from_secs(block * 10)
                        + Duration::from_millis(sample as u64 * 100 + 1),
                    0.02,
                );
            }
        }

        let reference = node
            .build_reference(started, started + Duration::from_secs(60))
            .unwrap();

        assert!((reference.median - 0.02).abs() < 1e-12);
        assert_eq!(reference.scale, ROBUST_SCALE_FLOOR);
        assert_eq!(reference.block_count, 6);
    }

    #[test]
    fn sparse_calibration_blocks_are_rejected() {
        let started = Instant::now();
        let mut node = NodePresenceState::default();
        for block in 0..MIN_CALIBRATION_BLOCKS {
            for sample in 0..(MIN_CALIBRATION_SAMPLES_PER_BLOCK - 1) {
                node.observe_calibration(
                    started
                        + Duration::from_secs(block as u64 * 10)
                        + Duration::from_millis(sample as u64 * 100 + 1),
                    0.02,
                );
            }
        }

        let result = node.build_reference(
            started,
            started + Duration::from_secs(MIN_CALIBRATION_BLOCKS as u64 * 10),
        );

        assert!(result.is_err());
    }

    #[test]
    fn live_vote_requires_a_complete_time_window() {
        let started = Instant::now();
        let mut node = NodePresenceState::default();
        node.install_reference_for_test(0.01, 0.005);

        for sample in 0..12 {
            node.observe_live(started + Duration::from_millis(sample * 900), 0.03);
        }
        assert!(!node.live_ready());
        assert!(!node.vote());

        node.observe_live(started + Duration::from_millis(10_800), 0.03);
        assert!(node.live_ready());
        assert!(node.vote());
    }

    #[test]
    fn long_input_gap_requires_a_new_complete_window() {
        let started = Instant::now();
        let mut node = NodePresenceState::default();
        node.install_reference_for_test(0.01, 0.005);
        for sample in 0..=100 {
            node.observe_live(started + Duration::from_millis(sample * 100), 0.03);
        }
        assert!(node.live_ready());

        node.observe_live(started + Duration::from_secs(21), 0.03);

        assert!(!node.live_ready());
        assert!(!node.vote());
    }

    #[test]
    fn six_second_gap_requires_a_new_complete_window() {
        let started = Instant::now();
        let mut node = NodePresenceState::default();
        node.install_reference_for_test(0.01, 0.005);
        for sample in 0..=100 {
            node.observe_live(started + Duration::from_millis(sample * 100), 0.03);
        }
        assert!(node.evidence_ready(started + LIVE_WINDOW));

        let resumed_at = started + Duration::from_secs(16);
        node.observe_live(resumed_at, 0.03);
        for sample in 1..=5 {
            node.observe_live(resumed_at + Duration::from_millis(sample * 100), 0.03);
        }

        assert!(node.observation_ready(resumed_at + Duration::from_millis(500)));
        assert!(!node.live_ready());
        assert!(!node.evidence_ready(resumed_at + Duration::from_millis(500)));

        for sample in 6..=100 {
            node.observe_live(resumed_at + Duration::from_millis(sample * 100), 0.03);
        }
        assert!(node.evidence_ready(resumed_at + LIVE_WINDOW));
    }

    #[test]
    fn fusion_requires_two_seconds_of_persistent_quorum() {
        let started = Instant::now();
        let mut fusion = PresenceFusionState::default();
        fusion.mark_ready_for_test(started);

        assert!(!fusion.update(true, true, started));
        assert!(!fusion.update(
            true,
            true,
            started + STATE_PERSISTENCE - Duration::from_millis(1)
        ));
        assert!(fusion.update(true, true, started + STATE_PERSISTENCE));
    }

    #[test]
    fn degraded_evidence_clears_the_last_state() {
        let started = Instant::now();
        let mut fusion = PresenceFusionState::default();
        fusion.mark_ready_for_test(started);
        fusion.update(true, true, started);
        assert!(fusion.update(true, true, started + STATE_PERSISTENCE));

        assert!(!fusion.update(false, false, started + Duration::from_secs(10)));
        assert!(!fusion.present());
    }

    #[test]
    fn d5_evidence_uses_only_fresh_accepted_scores() {
        let started = Instant::now();
        let mut node = NodePresenceState::default();
        node.install_reference_for_test(0.01, 0.005);
        for sample in 0..=100 {
            node.observe_live(started + Duration::from_millis(sample * 100), 0.03);
        }

        assert!(node.evidence_ready(started + LIVE_WINDOW));
        assert!(!node.evidence_ready(
            started + LIVE_WINDOW + OBSERVATION_FRESHNESS + Duration::from_millis(1)
        ));
    }
}
