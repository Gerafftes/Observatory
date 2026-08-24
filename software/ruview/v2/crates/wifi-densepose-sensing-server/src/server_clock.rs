//! Process-local host clock used to align independently received sensor data.
//!
//! Unix time is retained for auditability. Alignment exclusively uses the
//! monotonic value and requires an identical clock epoch on both streams.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct ServerClock {
    started_at: Instant,
    epoch_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostTimestamp {
    pub(crate) host_unix_ns: u64,
    pub(crate) host_monotonic_ns: u64,
    pub(crate) clock_epoch_id: String,
}

impl From<u64> for HostTimestamp {
    fn from(value: u64) -> Self {
        Self {
            host_unix_ns: value,
            host_monotonic_ns: value,
            clock_epoch_id: "synthetic-test-clock".to_string(),
        }
    }
}

static SERVER_CLOCK: OnceLock<ServerClock> = OnceLock::new();

pub(crate) fn now() -> HostTimestamp {
    let clock = SERVER_CLOCK.get_or_init(|| {
        let unix_ns = unix_ns();
        ServerClock {
            started_at: Instant::now(),
            epoch_id: format!("{:016x}-{:08x}", unix_ns, std::process::id()),
        }
    });
    HostTimestamp {
        host_unix_ns: unix_ns(),
        host_monotonic_ns: u64::try_from(clock.started_at.elapsed().as_nanos()).unwrap_or(u64::MAX),
        clock_epoch_id: clock.epoch_id.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_share_an_epoch_and_never_move_backwards_monotonically() {
        let first = now();
        let second = now();
        assert_eq!(first.clock_epoch_id, second.clock_epoch_id);
        assert!(second.host_monotonic_ns >= first.host_monotonic_ns);
    }
}
