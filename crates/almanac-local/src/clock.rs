//! The system clock: the one real implementation of `almanac_service::Clock`.

use almanac_core::UnixSeconds;
use almanac_service::Clock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Reads the system's wall clock. The daemon and an app hosting its own memory both pass this;
/// tests pass a stepped clock instead, as the backends take the clock as a parameter.
///
/// ```
/// use almanac_core::UnixSeconds;
/// use almanac_local::SystemClock;
/// use almanac_service::Clock;
///
/// assert!(SystemClock.now() > UnixSeconds(1_577_836_800));
/// ```
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
