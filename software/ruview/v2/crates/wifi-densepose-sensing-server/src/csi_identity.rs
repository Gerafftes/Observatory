//! Fail-closed live CSI identity and bounded retry deduplication.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

const RECENT_SEQUENCE_CAPACITY: usize = 256;
pub(crate) const ACTIVE_BOOT_CONFLICT_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admission {
    Accepted { rebooted: bool, ip_changed: bool },
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
    latest_sequence: u32,
    recent_sequences: VecDeque<u32>,
}

impl ActiveRx {
    fn new(boot_id: u64, sequence: u32, source: SocketAddr, now: Instant) -> Self {
        let mut recent_sequences = VecDeque::with_capacity(RECENT_SEQUENCE_CAPACITY);
        recent_sequences.push_back(sequence);
        Self {
            boot_id,
            source,
            last_seen: now,
            latest_sequence: sequence,
            recent_sequences,
        }
    }

    fn remember(&mut self, sequence: u32) {
        self.recent_sequences.push_back(sequence);
        if self.recent_sequences.len() > RECENT_SEQUENCE_CAPACITY {
            self.recent_sequences.pop_front();
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

        if active.recent_sequences.contains(&sequence) {
            self.duplicate_packets = self.duplicate_packets.saturating_add(1);
            return Admission::Duplicate;
        }

        let sequence_delta = sequence.wrapping_sub(active.latest_sequence);
        if sequence_delta >= 0x8000_0000 {
            self.out_of_order_packets = self.out_of_order_packets.saturating_add(1);
            return Admission::OutOfOrder;
        }

        let ip_changed = active.source.ip() != source.ip();
        active.source = source;
        active.last_seen = now;
        active.latest_sequence = sequence;
        active.remember(sequence);
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
