//! What the watcher sees and what apps say: the inputs of the join.

use almanac_core::{Actor, AppName, ContentDigest, FileChange, FileWhyClaim, SpacePath};

/// A point in time in milliseconds since the epoch: the join's window is finer than seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Stamp(pub i64);

/// How far apart an observation and an app's explanation may be to belong together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JoinWindow(pub u32);

impl JoinWindow {
    /// Plus or minus two seconds (design/22 `memory.join_window_ms`).
    pub const PROPOSED: JoinWindow = JoinWindow(2_000);
}

/// One change the filesystem reported, after rename halves were paired by their cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// The path.
    pub path: SpacePath,
    /// What happened.
    pub change: FileChange,
    /// Its inode: how renames are followed.
    pub inode: u64,
    /// Its content digest, if the watcher computed one.
    pub content: Option<ContentDigest>,
    /// When it was seen.
    pub at: Stamp,
    /// The app that made the change, when the process could be resolved to one.
    pub by_app: Option<AppName>,
}

/// An app's explanation, with when it arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhyAt {
    /// What the app said.
    pub claim: FileWhyClaim,
    /// When it said it.
    pub at: Stamp,
}

/// A file event ready to record, with the actor memoryd stamps on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvent {
    /// What happened.
    pub change: FileChange,
    /// Which file.
    pub file: almanac_core::FileView,
    /// Why.
    pub why: almanac_core::FileWhy,
    /// Who: the app's claim, the resolved app, or `Actor::Unknown`.
    pub actor: Actor,
}

/// A path that moved, for the log's `aliases` table: memory follows the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// Where it was.
    pub from: SpacePath,
    /// Where it is.
    pub to: SpacePath,
}

/// One settled result of the join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
    /// The event to record.
    pub event: FileEvent,
    /// For a rename, the alias row.
    pub alias: Option<Alias>,
}
