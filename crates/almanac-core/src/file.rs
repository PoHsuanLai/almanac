//! File events: what the watcher saw and why (when an app said why).

use crate::ids::SpacePath;
use crate::text::ContentDigest;
use crate::thing::{ThingRef, Verb};
use prov::Actor;
use serde::{Deserialize, Serialize};

/// What happened to a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum FileChange {
    /// Appeared.
    Created,
    /// Its content changed.
    Modified,
    /// Moved or renamed; memory follows the file through `aliases`.
    Renamed {
        /// Where it was.
        from: SpacePath,
    },
    /// Gone.
    Deleted,
    /// Closed after writing.
    Closed,
}

impl FileChange {
    /// The last element of the event's kind tag (`file.created`).
    pub fn slug(&self) -> &'static str {
        match self {
            FileChange::Created => "created",
            FileChange::Modified => "modified",
            FileChange::Renamed { .. } => "renamed",
            FileChange::Deleted => "deleted",
            FileChange::Closed => "closed",
        }
    }
}

/// The file an event is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileView {
    /// Where it is.
    pub path: SpacePath,
    /// Its inode: how renames are followed.
    pub inode: u64,
    /// Its content digest, as the app or watcher computed it.
    pub content: ContentDigest,
}

/// Whether anything explained a file change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum FileWhy {
    /// An app said what caused it.
    Explained {
        /// The thing it came from.
        cause: ThingRef,
        /// What was done.
        verb: Verb,
        /// Who did it.
        by: Actor,
    },
    /// Nobody did.
    Unexplained,
}
