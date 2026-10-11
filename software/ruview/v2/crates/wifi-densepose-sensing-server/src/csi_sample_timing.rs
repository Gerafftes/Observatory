//! Per-RX sample cadence inferred from device callback or host receive time.

use serde::{Deserialize, Serialize};

const MIN_SAMPLE_RATE_HZ: f64 = 1.0;
const MAX_SAMPLE_RATE_HZ: f64 = 100.0;
const MIN_INTERVAL_US: u64 = 10_000;
const MAX_INTERVAL_US: u64 = 1_000_000;
const EMA_ALPHA: f64 = 0.125;
const RATE_CHANGE_THRESHOLD: f64 = 0.20;
const RATE_CHANGE_CONFIRMATION_RATIO: f64 = 0.10;
const RATE_CHANGE_CONFIRMATIONS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TimingSource {
    DeviceCallback,
    HostReceive,
}

#[derive(Debug, Clone, Copy, Default)]
struct SampleAnchor {
    sequence: u32,
    timestamp_us: u64,
    source: Option<TimingSource>,
}

#[derive(Debug, Clone, Copy)]
struct PendingRateChange {
    hz: f64,
    confirmations: u8,
}

#[derive(Debug, Default)]
pub(super) struct CsiSampleRateEstimator {
    anchor: Option<SampleAnchor>,
    rate_hz: Option<f64>,
    mean_interval_us: Option<f64>,
    jitter_us: Option<f64>,
    source: Option<TimingSource>,
    samples: u64,
    gap_events: u64,
    missing_frames: u64,
    pauses: u64,
    rate_changes: u64,
    sequence_regressions: u64,
    clock_resets: u64,
    invalid_intervals: u64,
    pending_rate_change: Option<PendingRateChange>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub(super) struct CsiSampleTimingSnapshot {
    pub(super) rate_hz: Option<f64>,
    pub(super) jitter_us: Option<f64>,
    pub(super) source: Option<TimingSource>,
    pub(super) samples: u64,
    pub(super) gap_events: u64,
    pub(super) missing_frames: u64,
    pub(super) pauses: u64,
    pub(super) rate_changes: u64,
    pub(super) sequence_regressions: u64,
    pub(super) clock_resets: u64,
    pub(super) invalid_intervals: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CsiSampleTimingUpdate {
    pub(super) snapshot: CsiSampleTimingSnapshot,
    /// True when this frame must start a new temporal signal-processing window.
    pub(super) discontinuity: bool,
}

impl CsiSampleRateEstimator {
    pub(super) fn observe(
        &mut self,
        sequence: u32,
        device_callback_us: Option<u64>,
        host_receive_monotonic_ns: u64,
    ) -> CsiSampleTimingUpdate {
        let (source, timestamp_us) = match device_callback_us.filter(|value| *value > 0) {
            Some(timestamp_us) => (TimingSource::DeviceCallback, timestamp_us),
            None if host_receive_monotonic_ns > 0 => {
                (TimingSource::HostReceive, host_receive_monotonic_ns / 1_000)
            }
            None => return self.update(false),
        };
        let current = SampleAnchor {
            sequence,
            timestamp_us,
            source: Some(source),
        };

        let Some(previous) = self.anchor else {
            self.anchor = Some(current);
            self.source = Some(source);
            return self.update(false);
        };

        if previous.source != Some(source) {
            self.clock_resets = self.clock_resets.saturating_add(1);
            self.reset_cadence();
            self.anchor = Some(current);
            self.source = Some(source);
            return self.update(true);
        }

        let sequence_delta = sequence.wrapping_sub(previous.sequence);
        if sequence_delta == 0 {
            return self.update(false);
        }
        if sequence_delta >= 0x8000_0000 {
            self.sequence_regressions = self.sequence_regressions.saturating_add(1);
            self.reset_at(current);
            return self.update(true);
        }

        let Some(interval_us) = timestamp_us.checked_sub(previous.timestamp_us) else {
            self.clock_resets = self.clock_resets.saturating_add(1);
            self.reset_at(current);
            return self.update(true);
        };
        self.anchor = Some(current);

        if sequence_delta > 1 {
            self.gap_events = self.gap_events.saturating_add(1);
            self.missing_frames = self
                .missing_frames
                .saturating_add(u64::from(sequence_delta - 1));
        }

        if interval_us > MAX_INTERVAL_US {
            self.pauses = self.pauses.saturating_add(1);
            self.reset_cadence();
            return self.update(true);
        }
        if interval_us < MIN_INTERVAL_US {
            self.invalid_intervals = self.invalid_intervals.saturating_add(1);
            self.reset_cadence();
            return self.update(true);
        }

        let observed_rate = f64::from(sequence_delta) * 1_000_000.0 / interval_us as f64;
        if !(MIN_SAMPLE_RATE_HZ..=MAX_SAMPLE_RATE_HZ).contains(&observed_rate) {
            self.invalid_intervals = self.invalid_intervals.saturating_add(1);
            self.reset_cadence();
            return self.update(true);
        }

        let observed_interval_us = interval_us as f64 / f64::from(sequence_delta);
        let Some(previous_rate) = self.rate_hz else {
            self.rate_hz = Some(observed_rate);
            self.mean_interval_us = Some(observed_interval_us);
            self.samples = self.samples.saturating_add(1);
            return self.update(sequence_delta > 1);
        };

        let relative_change = (observed_rate - previous_rate).abs() / previous_rate;
        if relative_change >= RATE_CHANGE_THRESHOLD {
            let pending = match self.pending_rate_change {
                Some(pending)
                    if (observed_rate - pending.hz).abs() / pending.hz
                        <= RATE_CHANGE_CONFIRMATION_RATIO =>
                {
                    PendingRateChange {
                        hz: pending.hz + (observed_rate - pending.hz) / 2.0,
                        confirmations: pending.confirmations.saturating_add(1),
                    }
                }
                _ => PendingRateChange {
                    hz: observed_rate,
                    confirmations: 1,
                },
            };

            if pending.confirmations >= RATE_CHANGE_CONFIRMATIONS {
                self.rate_changes = self.rate_changes.saturating_add(1);
                self.rate_hz = Some(pending.hz);
                self.mean_interval_us = Some(1_000_000.0 / pending.hz);
                self.jitter_us = None;
                self.pending_rate_change = None;
                self.samples = self.samples.saturating_add(1);
                return self.update(true);
            }

            self.pending_rate_change = Some(pending);
            return self.update(sequence_delta > 1);
        }

        self.pending_rate_change = None;
        if let Some(mean_interval_us) = self.mean_interval_us {
            let deviation = (observed_interval_us - mean_interval_us).abs();
            self.jitter_us = Some(match self.jitter_us {
                Some(jitter) => jitter + EMA_ALPHA * (deviation - jitter),
                None => deviation,
            });
            let next_interval =
                mean_interval_us + EMA_ALPHA * (observed_interval_us - mean_interval_us);
            self.mean_interval_us = Some(next_interval);
            self.rate_hz = Some(1_000_000.0 / next_interval);
        }
        self.samples = self.samples.saturating_add(1);
        self.update(sequence_delta > 1)
    }

    pub(super) fn snapshot(&self) -> CsiSampleTimingSnapshot {
        CsiSampleTimingSnapshot {
            rate_hz: self.rate_hz,
            jitter_us: self.jitter_us,
            source: self.source,
            samples: self.samples,
            gap_events: self.gap_events,
            missing_frames: self.missing_frames,
            pauses: self.pauses,
            rate_changes: self.rate_changes,
            sequence_regressions: self.sequence_regressions,
            clock_resets: self.clock_resets,
            invalid_intervals: self.invalid_intervals,
        }
    }

    fn reset_at(&mut self, current: SampleAnchor) {
        self.reset_cadence();
        self.anchor = Some(current);
        self.source = current.source;
    }

    fn reset_cadence(&mut self) {
        self.rate_hz = None;
        self.mean_interval_us = None;
        self.jitter_us = None;
        self.pending_rate_change = None;
    }

    fn update(&self, discontinuity: bool) -> CsiSampleTimingUpdate {
        CsiSampleTimingUpdate {
            snapshot: self.snapshot(),
            discontinuity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_uniform_rate(rate_hz: u64, count: u32) -> CsiSampleRateEstimator {
        let interval_us = 1_000_000 / rate_hz;
        let mut estimator = CsiSampleRateEstimator::default();
        for sequence in 0..count {
            estimator.observe(
                sequence,
                Some(u64::from(sequence) * interval_us),
                u64::from(sequence) * interval_us * 1_000,
            );
        }
        estimator
    }

    #[test]
    fn estimates_supported_callback_rates_with_low_jitter() {
        for rate_hz in [20, 25, 50, 100] {
            let snapshot = feed_uniform_rate(rate_hz, 200).snapshot();
            assert!(
                (snapshot.rate_hz.expect("measured rate") - rate_hz as f64).abs() < 0.1,
                "wrong estimate for {rate_hz} Hz: {snapshot:?}"
            );
            assert_eq!(snapshot.source, Some(TimingSource::DeviceCallback));
            assert!(snapshot.jitter_us.unwrap_or(0.0) < 1.0);
        }
    }

    #[test]
    fn missing_sequence_frames_are_counted_and_reset_temporal_windows() {
        let mut estimator = feed_uniform_rate(50, 4);
        let update = estimator.observe(5, Some(100_000), 100_000_000);
        assert!(update.discontinuity);
        assert_eq!(update.snapshot.gap_events, 1);
        assert_eq!(update.snapshot.missing_frames, 1);
        assert!((update.snapshot.rate_hz.unwrap() - 50.0).abs() < 0.1);
    }

    #[test]
    fn confirmed_rate_change_resets_temporal_windows_once() {
        let mut estimator = feed_uniform_rate(20, 8);
        let first_fast = estimator.observe(8, Some(360_000), 360_000_000);
        assert!(!first_fast.discontinuity);
        assert!((first_fast.snapshot.rate_hz.unwrap() - 20.0).abs() < 0.1);

        let second_fast = estimator.observe(9, Some(370_000), 370_000_000);
        assert!(second_fast.discontinuity);
        assert!((second_fast.snapshot.rate_hz.unwrap() - 100.0).abs() < 0.1);
        assert_eq!(second_fast.snapshot.rate_changes, 1);
    }

    #[test]
    fn long_pause_restarts_rate_warmup_without_blending_across_it() {
        let mut estimator = feed_uniform_rate(25, 4);
        let pause = estimator.observe(4, Some(1_120_001), 1_120_001_000);
        assert!(pause.discontinuity);
        assert_eq!(pause.snapshot.pauses, 1);
        assert_eq!(pause.snapshot.rate_hz, None);

        estimator.observe(5, Some(1_160_001), 1_160_001_000);
        assert!((estimator.snapshot().rate_hz.unwrap() - 25.0).abs() < 0.1);
    }

    #[test]
    fn sequence_regression_and_clock_source_change_reset_cadence() {
        let mut estimator = feed_uniform_rate(50, 4);
        let regressed = estimator.observe(2, Some(80_000), 80_000_000);
        assert!(regressed.discontinuity);
        assert_eq!(regressed.snapshot.sequence_regressions, 1);
        assert_eq!(regressed.snapshot.rate_hz, None);

        let changed_clock = estimator.observe(3, None, 90_000_000);
        assert!(changed_clock.discontinuity);
        assert_eq!(changed_clock.snapshot.clock_resets, 1);
        assert_eq!(
            changed_clock.snapshot.source,
            Some(TimingSource::HostReceive)
        );
    }

    #[test]
    fn host_receive_clock_is_used_only_when_device_callback_time_is_missing() {
        let mut estimator = CsiSampleRateEstimator::default();
        estimator.observe(1, None, 10_000_000);
        let update = estimator.observe(2, None, 30_000_000);
        assert_eq!(update.snapshot.source, Some(TimingSource::HostReceive));
        assert!((update.snapshot.rate_hz.unwrap() - 50.0).abs() < 0.1);
    }
}
