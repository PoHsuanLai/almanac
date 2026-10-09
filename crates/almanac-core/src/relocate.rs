//! What becomes of a removed desktop-wide Space's memories and event history.

use crate::slug::slug_enum;
use porter_core::Count;
use serde::{Deserialize, Serialize};

slug_enum!(
    /// What the person chose for the memories of a removed desktop-wide Space.
    MemoryFate {
        /// Each memory moves to the App Space of the app that wrote it (the default; pending
        /// ones stay pending).
        MoveToApps => "move_to_apps",
        /// The memories are deleted with the Space.
        Delete => "delete",
    }
);

slug_enum!(
    /// What the person chose for the event history of a removed desktop-wide Space.
    HistoryFate {
        /// Each event moves to the App Space of the app that recorded it (the default).
        Keep => "keep",
        /// The history is erased with the Space.
        Delete => "delete",
    }
);

/// What the person chose for a removed Space: its memories and its history, separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Removal {
    /// What becomes of the memories.
    pub memories: MemoryFate,
    /// What becomes of the event history.
    pub history: HistoryFate,
}

impl Removal {
    /// The non-destructive choice: memoryd's own when the registry removed the Space and no
    /// one asked the person.
    pub const KEEP_ALL: Removal = Removal {
        memories: MemoryFate::MoveToApps,
        history: HistoryFate::Keep,
    };
}

/// What a removal did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Relocation {
    /// Facts now in an App Space (counting the pending ones).
    pub moved: Count,
    /// Of those, the ones still waiting for the person.
    pub kept_pending: Count,
    /// Facts deleted with the Space.
    pub deleted: Count,
    /// Events now in an App Space's log.
    pub events_moved: Count,
    /// Events erased with the Space.
    pub events_deleted: Count,
}

impl Relocation {
    /// Nothing moved and nothing deleted.
    pub const NONE: Relocation = Relocation {
        moved: Count(0),
        kept_pending: Count(0),
        deleted: Count(0),
        events_moved: Count(0),
        events_deleted: Count(0),
    };
}
