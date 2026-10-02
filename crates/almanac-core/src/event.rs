//! What a writer sends and what the log stores: `Record`, `EventBody`, and where an event came
//! from.

use crate::area::AreaPayload;
use crate::file::{FileChange, FileView, FileWhy};
use crate::ids::{KindTag, ReplicaId, Seq};
use crate::op::MemoryOp;
use crate::text::UserText;
use crate::thing::{ThingRef, ThingRole, ThingView, Verb};
use porter_core::{AppName, Count, SpaceId, UnixSeconds};
use prov::{Actor, Effect, Label, SessionId};
use serde::{Deserialize, Serialize};

/// One event in one Space: the log's own address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventRef {
    /// The Space.
    pub space: SpaceId,
    /// The replica that wrote it.
    pub replica: ReplicaId,
    /// Its sequence number there.
    pub seq: Seq,
}

/// What caused an event, when something did.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Cause {
    /// Nothing recorded.
    None,
    /// Another event.
    Event(EventRef),
    /// An undo (the owner's token).
    Undo(String),
    /// A companion session's plan.
    Plan(SessionId),
}

/// What a writer sends. memoryd stamps it with a sequence number, the recorded time, the
/// replica and the caller, and checks `actor` against the caller's class.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Record {
    /// Which Space.
    pub space: SpaceId,
    /// When it happened (event time).
    pub occurred: UnixSeconds,
    /// Who did it.
    pub actor: Actor,
    /// How consequential it was.
    pub effect: Effect,
    /// Its provenance label.
    pub label: Label,
    /// What it was.
    pub body: EventBody,
    /// What caused it.
    pub cause: Cause,
}

/// One event's typed body. Other areas' payloads are [`AreaPayload`]s.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum EventBody {
    /// Something happened to a thing.
    Thing {
        /// What.
        verb: Verb,
        /// To which thing, as seen.
        thing: ThingView,
        /// Where it came from.
        sources: Vec<ThingView>,
    },
    /// A file changed.
    File {
        /// How.
        change: FileChange,
        /// Which file.
        file: FileView,
        /// Why, if anything said.
        why: FileWhy,
    },
    /// Someone searched inside an app.
    Search {
        /// Which app.
        app: AppName,
        /// What they typed.
        text: UserText,
        /// Which kinds of thing.
        scope: crate::thing::ThingKind,
        /// How many results.
        results: Count,
    },
    /// memoryd's own audit.
    Memory {
        /// What it did.
        op: MemoryOp,
    },
    /// An opaque payload from another area.
    Area(AreaPayload),
}

impl EventBody {
    /// The header's kind tag. The one match.
    pub fn kind(&self) -> KindTag {
        match self {
            EventBody::Thing { verb, .. } => KindTag::of("thing", verb.slug()),
            EventBody::File { change, .. } => KindTag::of("file", change.slug()),
            EventBody::Search { .. } => KindTag::of("search", "performed"),
            EventBody::Memory { op } => KindTag::of("memory", op.slug()),
            EventBody::Area(payload) => payload.kind.clone(),
        }
    }

    /// The things the event is about or came from: the rows of the log's `things` table.
    pub fn things(&self) -> Vec<(&ThingView, ThingRole)> {
        match self {
            EventBody::Thing { thing, sources, .. } => std::iter::once((thing, ThingRole::Subject))
                .chain(sources.iter().map(|s| (s, ThingRole::Source)))
                .collect(),
            EventBody::Area(payload) => payload.things.iter().map(|(t, r)| (t, *r)).collect(),
            EventBody::File { .. } | EventBody::Search { .. } | EventBody::Memory { .. } => {
                Vec::new()
            }
        }
    }

    /// Whether the event names `thing`, in either role.
    pub fn names(&self, thing: &ThingRef) -> bool {
        self.things().iter().any(|(view, _)| &view.thing == thing)
    }
}
