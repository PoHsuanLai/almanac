//! What becomes of a removed desktop-wide Space's memories.

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

/// What a removal did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Relocation {
    /// Facts now in an App Space (counting the pending ones).
    pub moved: Count,
    /// Of those, the ones still waiting for the person.
    pub kept_pending: Count,
    /// Facts deleted with the Space.
    pub deleted: Count,
}

impl Relocation {
    /// Nothing moved and nothing deleted.
    pub const NONE: Relocation = Relocation {
        moved: Count(0),
        kept_pending: Count(0),
        deleted: Count(0),
    };
}
