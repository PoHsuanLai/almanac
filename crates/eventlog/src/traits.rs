//! The log's seams: reading, writing and what can go wrong.

use crate::chain::Entry;
use crate::header::NewHeader;
use almanac_core::{Checkpoint, Count, EventBody, Head, Seq, ThingRef, TimelineQuery};

/// A page of the log, newest first: the timeline's own query.
pub type PageQuery = TimelineQuery;

/// Which role of a thing a lookup wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoleFilter {
    /// Subject or source.
    Either,
    /// Events about the thing.
    Subject,
    /// Events that came from the thing.
    Source,
}

/// Why a log operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogError {
    /// The key does not open the database.
    #[error("the event log is locked")]
    Locked,
    /// The stored chain is not consistent at this entry.
    #[error("the event log is corrupt at {at:?}")]
    Corrupt {
        /// Where.
        at: Seq,
    },
    /// The disk is full.
    #[error("the event log is full")]
    Full,
    /// SQLite said no.
    #[error("sqlite: {0}")]
    Sqlite(String),
    /// A schema version this build does not know.
    #[error("unknown event log schema {found}")]
    Schema {
        /// What was found.
        found: u32,
    },
    /// The body does not match the digest in its header.
    #[error("body does not match its header's digest")]
    BadDigest,
    /// No entry at that sequence number.
    #[error("no entry at {0:?}")]
    NoSuchEntry(Seq),
}

/// Reading the log.
pub trait LogRead {
    /// The newest entry, or the checkpoint's position when the log is empty.
    fn head(&self) -> Result<Head, LogError>;
    /// Where the retained entries start: genesis, or the last prune.
    fn checkpoint(&self) -> Result<Checkpoint, LogError>;
    /// A page, newest first, from a cursor, filtered.
    fn page(&self, q: &PageQuery) -> Result<Vec<Entry>, LogError>;
    /// The entries whose bodies name `thing` in the given role.
    fn touching(&self, thing: &ThingRef, role: RoleFilter) -> Result<Vec<Seq>, LogError>;
    /// Every entry from `from`, ascending: for verification and index rebuilds.
    fn scan(&self, from: Seq) -> Result<Vec<Entry>, LogError>;
}

/// Writing the log.
pub trait LogWrite: LogRead {
    /// Chains and stores a header and, unless the admission was header-only, its body.
    fn append(&mut self, header: NewHeader, body: Option<EventBody>) -> Result<Entry, LogError>;
    /// Erases bodies and their `things` rows; headers and digests stay.
    fn erase_bodies(&mut self, seqs: &[Seq]) -> Result<Count, LogError>;
    /// Removes the prefix up to and including `cut`, leaving a checkpoint. Prefix only.
    fn prune_before(&mut self, cut: Seq) -> Result<Checkpoint, LogError>;
}
