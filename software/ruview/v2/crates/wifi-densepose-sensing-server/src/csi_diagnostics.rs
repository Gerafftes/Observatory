//! Low-overhead, process-local diagnostics for the UDP CSI ingress boundary.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CsiDatagramOutcome {
    AcceptedNew,
    Duplicate,
}

#[derive(Debug, Default)]
pub(crate) struct CsiIngressDiagnostics {
    balance_lock: Mutex<()>,
    udp_datagrams_from_os: AtomicU64,
    udp_bytes_from_os: AtomicU64,
    udp_receive_errors: AtomicU64,
    udp_queue_enqueued: AtomicU64,
    udp_queue_overflow_datagrams: AtomicU64,
    udp_queue_stale_datagrams: AtomicU64,
    udp_queue_depth: AtomicU64,
    udp_queue_high_watermark: AtomicU64,
    udp_queue_age_total_us: AtomicU64,
    udp_queue_age_max_us: AtomicU64,
    udp_queue_age_last_us: AtomicU64,
    udp_queue_age_samples: AtomicU64,
    csi_datagrams_received: AtomicU64,
    csi_frames_discarded: AtomicU64,
    csi_duplicate_datagrams: AtomicU64,
    csi_out_of_order_datagrams: AtomicU64,
    csi_frames_accepted_new: AtomicU64,
    csi_queue_overflow_datagrams: AtomicU64,
    csi_queue_stale_datagrams: AtomicU64,
    csi_processing_current: AtomicU64,
    last_server_receive_unix_ns: AtomicU64,
    last_server_receive_monotonic_ns: AtomicU64,
    processing_duration_total_us: AtomicU64,
    processing_duration_max_us: AtomicU64,
    processing_duration_last_us: AtomicU64,
    processing_duration_samples: AtomicU64,
    csi_processing_duration_total_us: AtomicU64,
    csi_processing_duration_max_us: AtomicU64,
    csi_processing_duration_last_us: AtomicU64,
    csi_processing_duration_samples: AtomicU64,
    last_processing_completed_unix_ns: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CsiIngressDiagnosticsSnapshot {
    pub(crate) udp_datagrams_from_os: u64,
    pub(crate) udp_bytes_from_os: u64,
    pub(crate) udp_receive_errors: u64,
    pub(crate) udp_queue_enqueued: u64,
    pub(crate) udp_queue_overflow_datagrams: u64,
    pub(crate) udp_queue_stale_datagrams: u64,
    pub(crate) udp_queue_depth: u64,
    pub(crate) udp_queue_high_watermark: u64,
    pub(crate) udp_queue_age_total_us: u64,
    pub(crate) udp_queue_age_avg_us: Option<u64>,
    pub(crate) udp_queue_age_max_us: u64,
    pub(crate) udp_queue_age_last_us: u64,
    pub(crate) udp_queue_age_samples: u64,
    pub(crate) csi_datagrams_received: u64,
    pub(crate) csi_frames_discarded: u64,
    pub(crate) csi_duplicate_datagrams: u64,
    pub(crate) csi_out_of_order_datagrams: u64,
    pub(crate) csi_frames_accepted_new: u64,
    pub(crate) csi_queue_overflow_datagrams: u64,
    pub(crate) csi_queue_stale_datagrams: u64,
    pub(crate) csi_processing_current: u64,
    pub(crate) csi_balance_closed: bool,
    pub(crate) csi_outcomes_unaccounted: u64,
    pub(crate) last_server_receive_unix_ns: Option<u64>,
    pub(crate) last_server_receive_monotonic_ns: Option<u64>,
    pub(crate) processing_duration_total_us: u64,
    pub(crate) processing_duration_avg_us: Option<u64>,
    pub(crate) processing_duration_max_us: u64,
    pub(crate) processing_duration_last_us: u64,
    pub(crate) processing_duration_samples: u64,
    pub(crate) csi_processing_duration_total_us: u64,
    pub(crate) csi_processing_duration_avg_us: Option<u64>,
    pub(crate) csi_processing_duration_max_us: u64,
    pub(crate) csi_processing_duration_last_us: u64,
    pub(crate) csi_processing_duration_samples: u64,
    pub(crate) last_processing_completed_unix_ns: Option<u64>,
}

impl CsiIngressDiagnostics {
    pub(crate) fn note_udp_datagram_from_os(&self, len: usize) {
        self.udp_datagrams_from_os.fetch_add(1, Ordering::Relaxed);
        self.udp_bytes_from_os
            .fetch_add(u64::try_from(len).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    pub(crate) fn note_udp_receive_error(&self) {
        self.udp_receive_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_queue_enqueued(&self) {
        self.udp_queue_enqueued.fetch_add(1, Ordering::Relaxed);
        let depth = self
            .udp_queue_depth
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        self.udp_queue_high_watermark
            .fetch_max(depth, Ordering::Relaxed);
    }

    pub(crate) fn note_queue_overflow(&self, is_csi: bool) {
        self.udp_queue_overflow_datagrams
            .fetch_add(1, Ordering::Relaxed);
        if is_csi {
            self.csi_queue_overflow_datagrams
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn note_csi_order_error(&self) {
        self.csi_out_of_order_datagrams
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_queue_dequeued(&self, queued_at: Instant) {
        let _ = self
            .udp_queue_depth
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |depth| {
                Some(depth.saturating_sub(1))
            });
        let age_us = u64::try_from(queued_at.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.udp_queue_age_total_us
            .fetch_add(age_us, Ordering::Relaxed);
        self.udp_queue_age_last_us.store(age_us, Ordering::Relaxed);
        self.udp_queue_age_max_us
            .fetch_max(age_us, Ordering::Relaxed);
        self.udp_queue_age_samples.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_csi_stale_in_queue(&self) {
        self.csi_queue_stale_datagrams
            .fetch_add(1, Ordering::Relaxed);
        self.udp_queue_stale_datagrams
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_non_csi_stale_in_queue(&self) {
        self.udp_queue_stale_datagrams
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn begin_udp_processing(self: &Arc<Self>) -> UdpProcessingObservation {
        UdpProcessingObservation {
            diagnostics: Arc::clone(self),
            started_at: Instant::now(),
        }
    }

    pub(crate) fn begin_csi_datagram(
        self: &Arc<Self>,
        host_unix_ns: u64,
        host_monotonic_ns: u64,
    ) -> CsiDatagramObservation {
        let _balance_guard = self
            .balance_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.csi_datagrams_received.fetch_add(1, Ordering::Relaxed);
        self.csi_processing_current.fetch_add(1, Ordering::Relaxed);
        self.last_server_receive_unix_ns
            .store(host_unix_ns, Ordering::Relaxed);
        self.last_server_receive_monotonic_ns
            .store(host_monotonic_ns, Ordering::Relaxed);
        CsiDatagramObservation {
            diagnostics: Arc::clone(self),
            processing_started_at: None,
            finished: false,
        }
    }

    pub(crate) fn snapshot(&self) -> CsiIngressDiagnosticsSnapshot {
        let _balance_guard = self
            .balance_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let csi_datagrams_received = self.csi_datagrams_received.load(Ordering::Relaxed);
        let csi_frames_discarded = self.csi_frames_discarded.load(Ordering::Relaxed);
        let csi_duplicate_datagrams = self.csi_duplicate_datagrams.load(Ordering::Relaxed);
        let csi_frames_accepted_new = self.csi_frames_accepted_new.load(Ordering::Relaxed);
        let csi_processing_current = self.csi_processing_current.load(Ordering::Relaxed);
        let accounted = csi_frames_discarded
            .saturating_add(csi_duplicate_datagrams)
            .saturating_add(csi_frames_accepted_new)
            .saturating_add(csi_processing_current);
        let processing_duration_samples = self.processing_duration_samples.load(Ordering::Relaxed);
        let processing_duration_total_us =
            self.processing_duration_total_us.load(Ordering::Relaxed);
        let queue_age_samples = self.udp_queue_age_samples.load(Ordering::Relaxed);
        let queue_age_total_us = self.udp_queue_age_total_us.load(Ordering::Relaxed);
        let csi_processing_duration_samples =
            self.csi_processing_duration_samples.load(Ordering::Relaxed);
        let csi_processing_duration_total_us = self
            .csi_processing_duration_total_us
            .load(Ordering::Relaxed);

        CsiIngressDiagnosticsSnapshot {
            udp_datagrams_from_os: self.udp_datagrams_from_os.load(Ordering::Relaxed),
            udp_bytes_from_os: self.udp_bytes_from_os.load(Ordering::Relaxed),
            udp_receive_errors: self.udp_receive_errors.load(Ordering::Relaxed),
            udp_queue_enqueued: self.udp_queue_enqueued.load(Ordering::Relaxed),
            udp_queue_overflow_datagrams: self.udp_queue_overflow_datagrams.load(Ordering::Relaxed),
            udp_queue_stale_datagrams: self.udp_queue_stale_datagrams.load(Ordering::Relaxed),
            udp_queue_depth: self.udp_queue_depth.load(Ordering::Relaxed),
            udp_queue_high_watermark: self.udp_queue_high_watermark.load(Ordering::Relaxed),
            udp_queue_age_total_us: queue_age_total_us,
            udp_queue_age_avg_us: (queue_age_samples > 0)
                .then_some(queue_age_total_us / queue_age_samples.max(1)),
            udp_queue_age_max_us: self.udp_queue_age_max_us.load(Ordering::Relaxed),
            udp_queue_age_last_us: self.udp_queue_age_last_us.load(Ordering::Relaxed),
            udp_queue_age_samples: queue_age_samples,
            csi_datagrams_received,
            csi_frames_discarded,
            csi_duplicate_datagrams,
            csi_out_of_order_datagrams: self.csi_out_of_order_datagrams.load(Ordering::Relaxed),
            csi_frames_accepted_new,
            csi_queue_overflow_datagrams: self.csi_queue_overflow_datagrams.load(Ordering::Relaxed),
            csi_queue_stale_datagrams: self.csi_queue_stale_datagrams.load(Ordering::Relaxed),
            csi_processing_current,
            csi_balance_closed: csi_datagrams_received == accounted,
            csi_outcomes_unaccounted: csi_datagrams_received.saturating_sub(accounted),
            last_server_receive_unix_ns: nonzero(
                self.last_server_receive_unix_ns.load(Ordering::Relaxed),
            ),
            last_server_receive_monotonic_ns: nonzero(
                self.last_server_receive_monotonic_ns
                    .load(Ordering::Relaxed),
            ),
            processing_duration_total_us,
            processing_duration_avg_us: if processing_duration_samples == 0 {
                None
            } else {
                Some(processing_duration_total_us / processing_duration_samples)
            },
            processing_duration_max_us: self.processing_duration_max_us.load(Ordering::Relaxed),
            processing_duration_last_us: self.processing_duration_last_us.load(Ordering::Relaxed),
            processing_duration_samples,
            csi_processing_duration_total_us,
            csi_processing_duration_avg_us: (csi_processing_duration_samples > 0).then_some(
                csi_processing_duration_total_us / csi_processing_duration_samples.max(1),
            ),
            csi_processing_duration_max_us: self
                .csi_processing_duration_max_us
                .load(Ordering::Relaxed),
            csi_processing_duration_last_us: self
                .csi_processing_duration_last_us
                .load(Ordering::Relaxed),
            csi_processing_duration_samples,
            last_processing_completed_unix_ns: nonzero(
                self.last_processing_completed_unix_ns
                    .load(Ordering::Relaxed),
            ),
        }
    }

    fn finish(&self, outcome: Option<CsiDatagramOutcome>, processing_started_at: Option<Instant>) {
        {
            let _balance_guard = self
                .balance_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match outcome {
                Some(CsiDatagramOutcome::AcceptedNew) => {
                    self.csi_frames_accepted_new.fetch_add(1, Ordering::Relaxed);
                }
                Some(CsiDatagramOutcome::Duplicate) => {
                    self.csi_duplicate_datagrams.fetch_add(1, Ordering::Relaxed);
                }
                None => {
                    self.csi_frames_discarded.fetch_add(1, Ordering::Relaxed);
                }
            }
            self.csi_processing_current.fetch_sub(1, Ordering::Relaxed);
        }

        if let Some(started_at) = processing_started_at {
            let duration_us = u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX);
            self.csi_processing_duration_total_us
                .fetch_add(duration_us, Ordering::Relaxed);
            self.csi_processing_duration_last_us
                .store(duration_us, Ordering::Relaxed);
            self.csi_processing_duration_max_us
                .fetch_max(duration_us, Ordering::Relaxed);
            self.csi_processing_duration_samples
                .fetch_add(1, Ordering::Relaxed);
            self.last_processing_completed_unix_ns
                .store(unix_ns(), Ordering::Relaxed);
        }
    }
}

pub(crate) struct UdpProcessingObservation {
    diagnostics: Arc<CsiIngressDiagnostics>,
    started_at: Instant,
}

impl Drop for UdpProcessingObservation {
    fn drop(&mut self) {
        let duration_us = u64::try_from(self.started_at.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.diagnostics
            .processing_duration_total_us
            .fetch_add(duration_us, Ordering::Relaxed);
        self.diagnostics
            .processing_duration_last_us
            .store(duration_us, Ordering::Relaxed);
        self.diagnostics
            .processing_duration_max_us
            .fetch_max(duration_us, Ordering::Relaxed);
        self.diagnostics
            .processing_duration_samples
            .fetch_add(1, Ordering::Relaxed);
        self.diagnostics
            .last_processing_completed_unix_ns
            .store(unix_ns(), Ordering::Relaxed);
    }
}

pub(crate) struct CsiDatagramObservation {
    diagnostics: Arc<CsiIngressDiagnostics>,
    processing_started_at: Option<Instant>,
    finished: bool,
}

impl CsiDatagramObservation {
    pub(crate) fn begin_processing(&mut self) {
        if self.processing_started_at.is_none() {
            self.processing_started_at = Some(Instant::now());
        }
    }

    pub(crate) fn finish(mut self, outcome: CsiDatagramOutcome) {
        self.diagnostics
            .finish(Some(outcome), self.processing_started_at);
        self.finished = true;
    }
}

impl Drop for CsiDatagramObservation {
    fn drop(&mut self) {
        if !self.finished {
            self.diagnostics.finish(None, self.processing_started_at);
        }
    }
}

fn unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn nonzero(value: u64) -> Option<u64> {
    (value != 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_csi_datagram_has_exactly_one_outcome() {
        let diagnostics = Arc::new(CsiIngressDiagnostics::default());
        for len in [80, 100, 120, 40] {
            diagnostics.note_udp_datagram_from_os(len);
        }
        diagnostics.note_udp_receive_error();

        {
            let _processing = diagnostics.begin_udp_processing();
            let mut accepted = diagnostics.begin_csi_datagram(10, 1);
            accepted.begin_processing();
            accepted.finish(CsiDatagramOutcome::AcceptedNew);
        }
        {
            let _processing = diagnostics.begin_udp_processing();
            let mut duplicate = diagnostics.begin_csi_datagram(20, 2);
            duplicate.begin_processing();
            duplicate.finish(CsiDatagramOutcome::Duplicate);
        }
        {
            let _processing = diagnostics.begin_udp_processing();
            let mut discarded = diagnostics.begin_csi_datagram(30, 3);
            discarded.begin_processing();
            drop(discarded);
        }

        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.udp_datagrams_from_os, 4);
        assert_eq!(snapshot.udp_bytes_from_os, 340);
        assert_eq!(snapshot.udp_receive_errors, 1);
        assert_eq!(snapshot.csi_datagrams_received, 3);
        assert_eq!(snapshot.csi_frames_accepted_new, 1);
        assert_eq!(snapshot.csi_duplicate_datagrams, 1);
        assert_eq!(snapshot.csi_out_of_order_datagrams, 0);
        assert_eq!(snapshot.csi_frames_discarded, 1);
        assert_eq!(snapshot.csi_processing_current, 0);
        assert!(snapshot.csi_balance_closed);
        assert_eq!(snapshot.csi_outcomes_unaccounted, 0);
        assert_eq!(snapshot.processing_duration_samples, 3);
        assert_eq!(snapshot.csi_processing_duration_samples, 3);
    }

    #[test]
    fn in_progress_datagram_keeps_the_balance_explainable() {
        let diagnostics = Arc::new(CsiIngressDiagnostics::default());
        let observation = diagnostics.begin_csi_datagram(10, 1);
        let in_progress = diagnostics.snapshot();
        assert_eq!(in_progress.csi_processing_current, 1);
        assert!(in_progress.csi_balance_closed);
        assert_eq!(in_progress.csi_outcomes_unaccounted, 0);

        drop(observation);
        let finished = diagnostics.snapshot();
        assert_eq!(finished.csi_processing_current, 0);
        assert_eq!(finished.csi_frames_discarded, 1);
    }

    #[test]
    fn queue_metrics_count_occupancy_age_and_overflow_separately() {
        let diagnostics = Arc::new(CsiIngressDiagnostics::default());
        diagnostics.note_queue_enqueued();
        diagnostics.note_queue_enqueued();
        diagnostics.note_queue_overflow(true);
        diagnostics.note_csi_order_error();

        let queued_at = Instant::now() - std::time::Duration::from_millis(2);
        diagnostics.note_queue_dequeued(queued_at);
        let snapshot = diagnostics.snapshot();

        assert_eq!(snapshot.udp_queue_enqueued, 2);
        assert_eq!(snapshot.udp_queue_depth, 1);
        assert_eq!(snapshot.udp_queue_high_watermark, 2);
        assert_eq!(snapshot.udp_queue_overflow_datagrams, 1);
        assert_eq!(snapshot.csi_queue_overflow_datagrams, 1);
        assert_eq!(snapshot.csi_out_of_order_datagrams, 1);
        assert_eq!(snapshot.udp_queue_age_samples, 1);
        assert!(snapshot.udp_queue_age_last_us >= 1_000);

        diagnostics.note_queue_dequeued(Instant::now());
        assert_eq!(diagnostics.snapshot().udp_queue_depth, 0);
    }
}
