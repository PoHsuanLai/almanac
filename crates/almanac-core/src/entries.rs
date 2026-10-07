//! Paging one stream of the log in append order: the `Entries` request and its page, and the
//! durable append's acknowledgement.
//!
//! `Recent` answers "what happened lately", newest first, cut by time. `Entries` answers "what
//! did this writer append, in order, from where I stopped": the read a daemon uses to rebuild its
//! state from the log (a companion session, a run). The stream is chosen by kind patterns and by
//! one thing the events are about (a writer that wants to read its own stream back names the
//! stream as the `Subject` thing of every payload it records, which is also what lets a forget of
//! that thing erase the stream as one unit).

use crate::event::EventRef;
use crate::ids::{Cursor, KindPattern};
use crate::inject::{BodyMode, RecentEntry};
use crate::thing::ThingRef;
use porter_core::Count;
use serde::{Deserialize, Serialize};

/// "The next events of one stream": oldest first, strictly after `after`. Answered with
/// `MemoryReply::Entries`; allowed for the router and the shell, like `Recent`, and audited as
/// the same `Memory.Read` scope. It returns every event of the stream whether or not recall may
/// use it (see `Recallable`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntriesQuery {
    /// Which kinds (empty: all), e.g. `companion.session.*`.
    pub kinds: Vec<KindPattern>,
    /// Only events whose body names this thing as its `Subject` (none: any).
    pub about: Option<ThingRef>,
    /// Only events after this one: the `next` of the previous page (none: from the oldest).
    pub after: Option<Cursor>,
    /// How many at most.
    pub limit: Count,
    /// Whether each entry carries its body.
    pub bodies: BodyMode,
}

/// One page of a stream, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntriesPage {
    /// The entries, in append order. Each carries its label exactly as stored.
    pub entries: Vec<RecentEntry>,
    /// Pass this as `after` for the next page; none when the stream has nothing more yet.
    pub next: Option<Cursor>,
}

/// The acknowledgement of a durable append: the event is committed to the log's storage with its
/// fsync done, and will be there after a restart.
///
/// `event.seq` is the Space log's own sequence number: strictly increasing, one per event the
/// log appends, so it is also gapless across everything memoryd logs in the Space (its own audit
/// events take numbers between a writer's, which is why a writer reads its stream back with
/// `Entries` and does not count).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ack {
    /// Where the event is.
    pub event: EventRef,
}
