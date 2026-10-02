//! The clock seam: nothing below memoryd reads the wall clock.

use almanac_core::UnixSeconds;

/// Now, as seconds since the epoch. memoryd's `SystemClock` is the only reader of the real
/// clock; tests pass a fixed one.
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> UnixSeconds;
}
