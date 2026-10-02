//! A clock that stands still, and the instant every fixture uses.

use almanac_core::UnixSeconds;
use almanac_service::Clock;

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
