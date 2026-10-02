//! The system clock: with the environment and the bus, one of the few things below no seam.

use almanac_core::UnixSeconds;
use almanac_service::Clock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Reads the wall clock. memoryd is its only user.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixSeconds {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        UnixSeconds(seconds)
    }
}
