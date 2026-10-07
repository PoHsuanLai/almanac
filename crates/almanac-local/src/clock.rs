//! The wall clock for an app that hosts its own memory.

use almanac_core::UnixSeconds;
use almanac_service::Clock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Reads the system's wall clock, so an app needs no clock code. This is the one place
/// `almanac-local` reads the time, and only because the app chose this type: tests pass a
/// stepped clock, as `LocalBackend` takes the clock as a parameter.
#[derive(Debug, Clone, Copy, Default)]
pub struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> UnixSeconds {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        UnixSeconds(seconds)
    }
}
