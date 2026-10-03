//! A clock that stands still, and the instant every fixture uses.

use almanac_core::UnixSeconds;
use almanac_service::Clock;
use std::sync::atomic::{AtomicI64, Ordering};

/// Every fixture happens at this instant (2026-09-21T10:13:20Z).
pub const NOW: UnixSeconds = UnixSeconds(1_790_000_000);

/// A clock that always answers the same instant.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub UnixSeconds);

impl Default for FixedClock {
    fn default() -> Self {
        Self(NOW)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> UnixSeconds {
        self.0
    }
}

/// A clock that starts at [`NOW`] and moves only when a test says so.
#[derive(Debug, Default)]
pub struct SteppedClock(AtomicI64);

impl SteppedClock {
    /// Moves the clock forward.
    pub fn advance(&self, seconds: i64) {
        self.0.fetch_add(seconds, Ordering::Relaxed);
    }
}

impl Clock for SteppedClock {
    fn now(&self) -> UnixSeconds {
        UnixSeconds(NOW.0 + self.0.load(Ordering::Relaxed))
    }
}
