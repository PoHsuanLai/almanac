//! The data the timeline UI draws (labels belong to the UI: memoryd never sends prose).

use crate::event::EventRef;
use crate::ids::{Cursor, KindPattern, KindTag};
use crate::slug::slug_enum;
use crate::thing::ThingView;
use porter_core::{AppName, Count, UnixSeconds};
use prov::{Actor, Effect, Label};
use serde::{Deserialize, Serialize};

slug_enum!(
    /// Which actors a timeline shows.
    ActorFilter {
        /// All.
        Everyone => "everyone",
        /// The person.
        You => "you",
        /// The companion (including computer-use runs).
        Companion => "companion",
        /// A terminal (`quire-do`): the person or an agent typing in it, which cannot be told
        /// apart.
        Terminal => "terminal",
        /// An outside agent over MCP (Claude Desktop and the like), by whatever client name it
        /// gave.
        Mcp => "mcp",
        /// Apps acting on their own.
        Apps => "apps",
        /// Unexplained changes.
        Unknown => "unknown"
    }
);

slug_enum!(
    /// Which provenance a timeline shows.
    TrustFilter {
        /// All.
        Any => "any",
        /// Trusted labels only.
        TrustedOnly => "trusted_only",
        /// Untrusted labels only.
        UntrustedOnly => "untrusted_only"
    }
);

/// A timeline filter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimelineFilter {
    /// Which actors.
    pub actors: ActorFilter,
    /// Which apps (empty: all).
    pub apps: Vec<AppName>,
    /// Which kinds (empty: all).
    pub kinds: Vec<KindPattern>,
    /// Which provenance.
    pub trust: TrustFilter,
    /// Only events that happened in this closed range.
    pub range: Option<(UnixSeconds, UnixSeconds)>,
}

/// One page request, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimelineQuery {
    /// Events strictly before this cursor (none: from the newest).
    pub before: Option<Cursor>,
    /// How many at most.
    pub limit: Count,
    /// What to show.
    pub filter: TimelineFilter,
}

/// One page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimelinePage {
    /// The entries, newest first.
    pub entries: Vec<TimelineEntry>,
    /// Where the next page starts, if there is one.
    pub next: Option<Cursor>,
}

slug_enum!(
    /// Why a body is gone.
    EraseCause {
        /// The person forgot it.
        Forgotten => "forgotten",
        /// Retention expired.
        Expired => "expired",
        /// It was only ever recorded as a header.
        HeaderOnly => "header_only"
    }
);

/// Whether an entry still has its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum EntryBody {
    /// Present.
    Present,
    /// Erased.
    Erased {
        /// Why.
        by: EraseCause,
    },
}

/// What can undo an event, if anything: the owner's token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum UndoRef {
    /// Nothing.
    None,
    /// The token the owning app understands.
    Token(String),
}

/// One row of the timeline. The UI renders "You archived *Q4 budget*" from `actor`, `kind` and
/// `things`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// Which event.
    pub event: EventRef,
    /// When it happened.
    pub occurred: UnixSeconds,
    /// Who did it.
    pub actor: Actor,
    /// What kind.
    pub kind: KindTag,
    /// How consequential.
    pub effect: Effect,
    /// Its provenance.
    pub label: Label,
    /// What it is about; empty when the body is erased.
    pub things: Vec<ThingView>,
    /// Whether the body is present.
    pub body: EntryBody,
    /// How many facts were derived from it.
    pub derived_facts: Count,
    /// What can undo it.
    pub undo: UndoRef,
}
