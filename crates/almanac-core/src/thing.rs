//! Things: what an app owns and an event is about.

use crate::slug::slug_enum;
use crate::text::UserText;
use serde::{Deserialize, Serialize};

/// A typed thing an app owns: the same identity the action router uses.
pub type ThingRef = prov::EntityId;
/// What kind of thing (`mail.thread`, `files.file`).
pub type ThingKind = prov::EntityKind;
/// Which one, app-local and opaque.
pub type ThingKey = prov::EntityKey;

/// A thing as the person saw it when the event happened. It is forgettable: it lives in the
/// event body, never in the chained header.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThingView {
    /// Which thing.
    pub thing: ThingRef,
    /// Its title then.
    pub title: UserText,
    /// Its subtitle then.
    pub subtitle: UserText,
}

impl std::fmt::Debug for ThingView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThingView")
            .field("thing", &self.thing)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

slug_enum!(
    /// What happened to a thing.
    Verb {
        /// Opened.
        Opened => "opened",
        /// Looked at.
        Viewed => "viewed",
        /// Made.
        Created => "created",
        /// Changed.
        Edited => "edited",
        /// Written out.
        Saved => "saved",
        /// Given another name.
        Renamed => "renamed",
        /// Moved elsewhere.
        Moved => "moved",
        /// Copied.
        Copied => "copied",
        /// Removed for good.
        Deleted => "deleted",
        /// Filed away.
        Archived => "archived",
        /// Brought back.
        Restored => "restored",
        /// Sent to someone.
        Sent => "sent",
        /// Arrived from someone.
        Received => "received",
        /// Answered.
        Replied => "replied",
        /// Passed on.
        Forwarded => "forwarded",
        /// Shared with others.
        Shared => "shared",
        /// Fetched to this computer.
        Downloaded => "downloaded",
        /// Brought in from elsewhere.
        Imported => "imported",
        /// Written out for elsewhere.
        Exported => "exported",
        /// Searched for.
        Searched => "searched",
        /// Chosen.
        Selected => "selected",
        /// Pinned.
        Pinned => "pinned",
        /// Unpinned.
        Unpinned => "unpinned",
    }
);

slug_enum!(
    /// How a thing figures in an event.
    ThingRole {
        /// What the event is about.
        Subject => "subject",
        /// What the event came from.
        Source => "source",
    }
);
