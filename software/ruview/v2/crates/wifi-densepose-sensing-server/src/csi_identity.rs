//! Fail-closed live CSI identity and bounded retry deduplication.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

const RECENT_SEQUENCE_CAPACITY: usize = 256;
pub(crate) const ACTIVE_BOOT_CONFLICT_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admission {
    Accepted { rebooted: bool, ip_changed: bool },
    Reordered,
    Duplicate,
    OutOfOrder,
    UnknownNode,
    ConflictingBoot,
}

#[derive(Debug)]
struct ActiveRx {
    boot_id: u64,
    source: SocketAddr,
    last_seen: Instant,
    sequences: SequenceWindow,
}

// This bounds transport reordering, not measurement coverage or timestamp age.
const MAX_REORDER_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) struct SequenceWindow {
    first: u32,
    latest: u32,
    span: u64,
    received: u64,
    recent: VecDeque<(u32, Instant)>,
}

impl SequenceWindow {
    pub(crate) fn new(sequence: u32, now: Instant) -> Self {
        Self {
            first: sequence,
            latest: sequence,
            span: 0,
            received: 1,
            recent: VecDeque::from([(sequence, now)]),
        }
    }

    pub(crate) fn gaps(&self) -> u64 {
        self.span + 1 - self.received
    }

    pub(crate) fn admit(
        &mut self,
        sequence: u32,
        now: Instant,
        allow_reordering: bool,
    ) -> Admission {
        if self.recent.iter().any(|(seen, _)| *seen == sequence) {
            return Admission::Duplicate;
        }
        let delta = sequence.wrapping_sub(self.latest);
        let reordered = delta >= 0x8000_0000;
        if reordered {
            let crossed_at = self
                .recent
                .iter()
                .filter(|(seen, _)| seen.wrapping_sub(sequence) < 0x8000_0000)
                .map(|(_, time)| *time)
                .min();
            if !allow_reordering
                || self.latest.wrapping_sub(sequence) >= RECENT_SEQUENCE_CAPACITY as u32
                || crossed_at
                    .is_none_or(|time| now.saturating_duration_since(time) > MAX_REORDER_DELAY)
            {
                return Admission::OutOfOrder;
            }
            let extension = self.first.wrapping_sub(sequence);
            if extension < 0x8000_0000 {
                self.span += u64::from(extension);
                self.first = sequence;
            }
        } else {
            self.span += u64::from(delta);
            self.latest = sequence;
        }
        self.received += 1;
        self.recent
            .retain(|(seen, _)| self.latest.wrapping_sub(*seen) < RECENT_SEQUENCE_CAPACITY as u32);
        self.recent.push_back((sequence, now));
        if reordered {
            Admission::Reordered
        } else {
            Admission::Accepted {
                rebooted: false,
                ip_changed: false,
            }
        }
    }
}

impl ActiveRx {
    fn new(boot_id: u64, sequence: u32, source: SocketAddr, now: Instant) -> Self {
        Self {
            boot_id,
            source,
            last_seen: now,
            sequences: SequenceWindow::new(sequence, now),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Registry {
    active: HashMap<u8, ActiveRx>,
    pub(crate) duplicate_packets: u64,
    pub(crate) conflicting_boots: u64,
    pub(crate) out_of_order_packets: u64,
    pub(crate) unknown_nodes: u64,
}

impl Registry {
    pub(crate) fn admit(
        &mut self,
        node_id: u8,
        boot_id: u64,
        sequence: u32,
        source: SocketAddr,
        now: Instant,
    ) -> Admission {
        self.admit_with_reordering(node_id, boot_id, sequence, source, now, false)
    }

    pub(crate) fn admit_with_reordering(
        &mut self,
        node_id: u8,
        boot_id: u64,
        sequence: u32,
        source: SocketAddr,
        now: Instant,
        allow_reordering: bool,
    ) -> Admission {
        if !(1..=4).contains(&node_id) || boot_id == 0 {
            self.unknown_nodes = self.unknown_nodes.saturating_add(1);
            return Admission::UnknownNode;
        }

        let Some(active) = self.active.get_mut(&node_id) else {
            self.active
                .insert(node_id, ActiveRx::new(boot_id, sequence, source, now));
            return Admission::Accepted {
                rebooted: false,
                ip_changed: false,
            };
        };

        if active.boot_id != boot_id {
            if now.saturating_duration_since(active.last_seen) <= ACTIVE_BOOT_CONFLICT_WINDOW {
                self.conflicting_boots = self.conflicting_boots.saturating_add(1);
                return Admission::ConflictingBoot;
            }
            let ip_changed = active.source.ip() != source.ip();
            *active = ActiveRx::new(boot_id, sequence, source, now);
            return Admission::Accepted {
                rebooted: true,
                ip_changed,
            };
        }

        match active.sequences.admit(sequence, now, allow_reordering) {
            Admission::Duplicate => {
                self.duplicate_packets = self.duplicate_packets.saturating_add(1);
                return Admission::Duplicate;
            }
            Admission::OutOfOrder => {
                self.out_of_order_packets = self.out_of_order_packets.saturating_add(1);
                return Admission::OutOfOrder;
            }
            Admission::Reordered => return Admission::Reordered,
            _ => {}
        }

        let ip_changed = active.source.ip() != source.ip();
        active.source = source;
        active.last_seen = now;
        Admission::Accepted {
            rebooted: false,
            ip_changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(host: [u8; 4]) -> SocketAddr {
        SocketAddr::from((host, 5005))
    }

    #[test]
    fn bounded_late_frame_preserves_live_identity_and_deduplicates() {
        let now = Instant::now();
        let mut registry = Registry::default();
        let original = source([192, 0, 2, 2]);
        let changed = source([192, 0, 2, 99]);
        registry.admit(2, 1, 41126, original, now);
        registry.admit(2, 1, 41128, original, now);
        assert_eq!(
            registry.admit_with_reordering(
                2,
                1,
                41127,
                changed,
                now + Duration::from_micros(7191),
                true
            ),
            Admission::Reordered
        );
        let active = &registry.active[&2];
        assert_eq!(active.sequences.latest, 41128);
        assert_eq!(active.source, original);
        assert_eq!(active.last_seen, now);
        assert_eq!(active.sequences.gaps(), 0);
        assert_eq!(
            registry.admit_with_reordering(2, 1, 41127, original, now, true),
            Admission::Duplicate
        );
        assert_eq!(
            registry.admit_with_reordering(2, 2, 41129, original, now, true),
            Admission::ConflictingBoot
        );
    }

    #[test]
    fn fixed_reorder_bounds_wraparound_and_gap_accounting() {
        let now = Instant::now();
        let mut sequence = SequenceWindow::new(10, now);
        sequence.admit(12, now, true);
        assert_eq!(
            sequence.admit(11, now + MAX_REORDER_DELAY, true),
            Admission::Reordered
        );
        let mut expired = SequenceWindow::new(10, now);
        expired.admit(12, now, true);
        expired.admit(13, now + MAX_REORDER_DELAY, true);
        assert_eq!(
            expired.admit(11, now + MAX_REORDER_DELAY + Duration::from_nanos(1), true),
            Admission::OutOfOrder
        );
        let mut far = SequenceWindow::new(1000, now);
        assert_eq!(far.admit(744, now, true), Admission::OutOfOrder);
        let mut wrap = SequenceWindow::new(u32::MAX - 1, now);
        wrap.admit(0, now, true);
        assert_eq!(wrap.gaps(), 1);
        assert_eq!(wrap.admit(u32::MAX, now, true), Admission::Reordered);
        assert_eq!(wrap.gaps(), 0);
        assert_eq!(wrap.admit(u32::MAX, now, true), Admission::Duplicate);
    }

    #[test]
    fn late_arrivals_cannot_evict_a_duplicate_inside_the_sequence_window() {
        let now = Instant::now();
        let mut sequence = SequenceWindow::new(0, now);
        sequence.admit(255, now, true);
        for n in 1..255 {
            assert_eq!(sequence.admit(n, now, true), Admission::Reordered);
        }
        sequence.admit(256, now, true);
        sequence.admit(257, now, true);
        assert_eq!(sequence.admit(255, now, true), Admission::Duplicate);
        assert_eq!(sequence.gaps(), 0);
        assert!(sequence.recent.len() <= RECENT_SEQUENCE_CAPACITY);
        let mut beginning = SequenceWindow::new(12, now);
        assert_eq!(beginning.admit(11, now, true), Admission::Reordered);
        assert_eq!(beginning.gaps(), 0);
    }

    #[test]
    fn equal_sequences_from_different_receivers_remain_independent() {
        let now = Instant::now();
        let mut registry = Registry::default();
        for rx_id in 1..=4 {
            assert!(matches!(
                registry.admit(rx_id, u64::from(rx_id), 42, source([192, 0, 2, rx_id]), now,),
                Admission::Accepted { .. }
            ));
        }
    }

    #[test]
    fn retry_is_deduplicated_within_one_boot() {
        let now = Instant::now();
        let mut registry = Registry::default();
        registry.admit(1, 11, 7, source([192, 0, 2, 1]), now);
        assert_eq!(
            registry.admit(1, 11, 7, source([192, 0, 2, 1]), now),
            Admission::Duplicate
        );
        assert_eq!(registry.duplicate_packets, 1);
    }

    #[test]
    fn sequence_regression_is_rejected_but_wraparound_is_forward() {
        let now = Instant::now();
        let mut registry = Registry::default();
        registry.admit(1, 11, 10, source([192, 0, 2, 1]), now);
        assert!(matches!(
            registry.admit(1, 11, 12, source([192, 0, 2, 1]), now),
            Admission::Accepted { .. }
        ));
        assert_eq!(
            registry.admit(1, 11, 11, source([192, 0, 2, 1]), now),
            Admission::OutOfOrder
        );
        assert_eq!(registry.out_of_order_packets, 1);

        let mut wrapping = Registry::default();
        wrapping.admit(1, 11, u32::MAX - 1, source([192, 0, 2, 1]), now);
        assert!(matches!(
            wrapping.admit(1, 11, 0, source([192, 0, 2, 1]), now),
            Admission::Accepted { .. }
        ));
    }

    #[test]
    fn ip_change_does_not_change_identity() {
        let now = Instant::now();
        let mut registry = Registry::default();
        registry.admit(1, 11, 1, source([192, 0, 2, 1]), now);
        assert_eq!(
            registry.admit(1, 11, 2, source([192, 0, 2, 99]), now),
            Admission::Accepted {
                rebooted: false,
                ip_changed: true,
            }
        );
    }

    #[test]
    fn fresh_conflicting_boot_is_rejected_but_stale_reboot_is_accepted() {
        let now = Instant::now();
        let mut registry = Registry::default();
        registry.admit(1, 11, 1, source([192, 0, 2, 1]), now);
        registry.admit(2, 22, 1, source([192, 0, 2, 2]), now);
        assert_eq!(
            registry.admit(1, 33, 0, source([192, 0, 2, 3]), now),
            Admission::ConflictingBoot
        );
        assert_eq!(
            registry.admit(
                1,
                33,
                0,
                source([192, 0, 2, 3]),
                now + ACTIVE_BOOT_CONFLICT_WINDOW + Duration::from_millis(1),
            ),
            Admission::Accepted {
                rebooted: true,
                ip_changed: true,
            }
        );
        assert_eq!(
            registry.admit(2, 22, 2, source([192, 0, 2, 2]), now),
            Admission::Accepted {
                rebooted: false,
                ip_changed: false,
            },
            "RX1 reboot must not reset RX2"
        );
    }

    #[test]
    fn unknown_receiver_ids_fail_closed() {
        let mut registry = Registry::default();
        assert_eq!(
            registry.admit(5, 55, 1, source([192, 0, 2, 5]), Instant::now()),
            Admission::UnknownNode
        );
    }
}
