//! Turns what `almanac_tidy::changes` says changed into the bus: the signal to emit as it
//! is, or the lookup the daemon must make first (the pending count and the status, which the
//! signals carry).

use almanac_core::SpaceId;
use almanac_dbus::Signal;
use almanac_tidy::Change;

/// How the bus is told of a [`Change`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// Emit this signal as it is.
    Emit(Signal),
    /// Count the Space's pending facts, then emit `PendingChanged`.
    PendingChanged(SpaceId),
    /// Read the Space's status, then emit `StatusChanged`.
    StatusChanged(SpaceId),
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// The delivery of `change`; `None` for a change this build has no signal for.
pub fn delivery_of(change: Change) -> Option<Delivery> {
    match change {
        Change::Recorded { event, kind } => Some(Delivery::Emit(Signal::Recorded {
            space: event.space.to_string(),
            event_ref: json(&event),
            kind,
        })),
        Change::ConsolidationReady { space, run } => {
            Some(Delivery::Emit(Signal::ConsolidationReady {
                space: space.to_string(),
                run: run.to_string(),
            }))
        }
        Change::PendingChanged(space) => Some(Delivery::PendingChanged(space)),
        Change::StatusChanged(space) => Some(Delivery::StatusChanged(space)),
        _ => None,
    }
}
