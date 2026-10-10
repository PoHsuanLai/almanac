//! What a writer sends and what the log stores: `Record`, `EventBody`, and where an event came
//! from.

use crate::area::AreaPayload;
use crate::episode::Episode;
use crate::file::{FileChange, FileView, FileWhy};
use crate::ids::{KindTag, ReplicaId, Seq};
use crate::op::MemoryOp;
use crate::text::UserText;
use crate::thing::{ThingRef, ThingRole, ThingView, Verb};
use porter_core::{AppName, Count, SpaceId, UnixSeconds};
use prov::{Actor, Effect, Label, Message, Part, SessionId};
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
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// A message between agents (prov's one message model): a request, a report, the person's
    /// turn to a subagent, a note, possibly across Spaces. Stored as input with its label; it
    /// grants nothing. `Record.label` is the message's own label and `Record.actor` is the
    /// stamped sender.
    Message(Box<Message>),
    /// What happened in one task, run or side conversation: a trusted skeleton and an optional
    /// narrative with its own label. `Record.label` is the join of both parts' labels.
    Episode(Box<Episode>),
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
            EventBody::Message(_) => KindTag::of("companion", "message"),
            EventBody::Episode(_) => KindTag::of("companion", "episode"),
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
            EventBody::Episode(episode) => episode
                .skeleton
                .touched
                .iter()
                .map(|(t, r)| (t, *r))
                .collect(),
            EventBody::File { .. }
            | EventBody::Search { .. }
            | EventBody::Memory { .. }
            | EventBody::Message(_) => Vec::new(),
        }
    }

    /// Every thing the body names, with or without a view: [`EventBody::things`] plus the
    /// entities a message's parts reference (as sources). The service writes the log's `things`
    /// rows from this, so forgetting a thing also forgets the messages that name it.
    pub fn thing_refs(&self) -> Vec<(ThingRef, ThingRole)> {
        let viewed = self.things().into_iter().map(|(v, r)| (v.thing.clone(), r));
        let named = match self {
            EventBody::Message(m) => m
                .parts
                .iter()
                .filter_map(|p| match p {
                    Part::Entity(id) => Some((id.clone(), ThingRole::Source)),
                    Part::Text(_) | Part::Outcome(_) | Part::Undo(_) => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        viewed.chain(named).collect()
    }

    /// Whether the event names `thing`, in either role.
    pub fn names(&self, thing: &ThingRef) -> bool {
        self.thing_refs().iter().any(|(named, _)| named == thing)
    }
}
