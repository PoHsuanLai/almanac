//! Retention: when a body or a header has outlived its keep. Pure.

use almanac_core::{DayCount, HEADER_DAYS, Retention, UnixSeconds};

const SECONDS_PER_DAY: i64 = 86_400;

/// Whether the thing an event is about still exists (for `WhileSourceExists`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    /// It does.
    Exists,
    /// It was deleted.
    Gone,
}

/// What the retention sweep does with a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sweep {
    /// Leave it.
    Keep,
    /// Erase the body (the header and its digest stay).
    EraseBody,
}

fn older_than(days: DayCount, occurred: UnixSeconds, now: UnixSeconds) -> bool {
    now.0.saturating_sub(occurred.0) >= i64::from(days.0) * SECONDS_PER_DAY
}

/// What to do with a body recorded under `retention`.
pub fn sweep_body(
    retention: Retention,
    occurred: UnixSeconds,
    now: UnixSeconds,
    source: SourceState,
) -> Sweep {
    let expired = match retention {
        Retention::Days(days) => older_than(days, occurred, now),
        Retention::WhileSourceExists => source == SourceState::Gone,
        Retention::UntilForgotten => false,
    };
    if expired {
        Sweep::EraseBody
    } else {
        Sweep::Keep
    }
}

/// Whether a header (of an audit event whose body is gone) may now be pruned: a year on.
pub fn header_expired(occurred: UnixSeconds, now: UnixSeconds) -> bool {
    header_expired_after(HEADER_DAYS, occurred, now)
}

/// [`header_expired`] with the person's keep (`memory.retention.audit_header_days`) in place of
/// the year.
pub fn header_expired_after(days: DayCount, occurred: UnixSeconds, now: UnixSeconds) -> bool {
    older_than(days, occurred, now)
}
