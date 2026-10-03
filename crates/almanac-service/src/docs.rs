//! The log and the facts as recall documents: which documents an event contributes, under which
//! ids and with which labels; the newest-episode rule; the label check on a `Record`.
//!
//! Pure: the index is rebuildable from the log and the files because everything here is a
//! function of them.

use crate::class::tag_of;
use almanac_core::{
    AppName, EventBody, EventRef, Fact, FactId, IndexPart, Integrity, Label, MessageFault,
    ReplicaId, SenderCheck, Seq, UnixSeconds, from_hex,
};
use eventlog::{BodyState, Entry};
use recall::{Doc, DocId, Facets, TrustTier};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// The document id of a fact.
pub(crate) fn fact_doc_id(id: &FactId) -> String {
    format!("f:{id}")
}

/// The document id of an event's body documents (`e:<replica>:<seq>`).
pub(crate) fn event_doc_id(event: &EventRef) -> String {
    IndexPart::Message.doc_id(event)
}

/// What a document id names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DocRef {
    /// `f:<fact>`.
    Fact(FactId),
    /// `e:` or `n:` and the event it came from.
    Event {
        /// The writer's replica.
        replica: ReplicaId,
        /// Its sequence number.
        seq: Seq,
    },
}

impl DocRef {
    /// The reference a document id spells, if it spells one.
    pub(crate) fn parse(id: &str) -> Option<DocRef> {
        let (prefix, rest) = id.split_once(':')?;
        match prefix {
            "f" => FactId::parse(rest).ok().map(DocRef::Fact),
            "e" | "n" => {
                let (replica, seq) = rest.split_once(':')?;
                Some(DocRef::Event {
                    replica: ReplicaId(from_hex::<16>(replica)?),
                    seq: Seq(seq.parse().ok()?),
                })
            }
            _ => None,
        }
    }
}

/// One document of an event, before it is an index `Doc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventDoc {
    /// `e:<replica>:<seq>` or `n:<replica>:<seq>`.
    pub id: String,
    /// What is searched.
    pub text: String,
    /// The document's own label.
    pub label: Label,
    /// The `Facets.kind`.
    pub kind: &'static str,
    /// The app, for thing and search documents.
    pub app: Option<AppName>,
}

pub(crate) fn tier(label: &Label) -> TrustTier {
    match label.integrity {
        Integrity::Trusted => TrustTier::Trusted,
        Integrity::Untrusted => TrustTier::Untrusted,
    }
}

impl EventDoc {
    pub(crate) fn into_doc(self, at: UnixSeconds) -> Doc {
        Doc {
            id: DocId(self.id),
            text: self.text,
            at: at.0,
            facets: Facets {
                kind: self.kind.to_owned(),
                app: self.app.map(|a| a.to_string()),
                trust: tier(&self.label),
            },
            class: tag_of(&self.label),
        }
    }
}

/// The documents `body` contributes: messages and episodes through `index_texts` (each part
/// with its own label), things by their title and subtitle, searches by their text, under the
/// header's label. Other bodies are not searched.
pub(crate) fn event_docs(
    event: &EventRef,
    header_label: &Label,
    body: &EventBody,
) -> Vec<EventDoc> {
    match body {
        EventBody::Message(_) | EventBody::Episode(_) => body
            .index_texts()
            .into_iter()
            .map(|t| EventDoc {
                id: t.part.doc_id(event),
                text: t.text,
                label: t.label,
                kind: t.part.facet_kind(),
                app: None,
            })
            .collect(),
        EventBody::Thing { thing, .. } => vec![EventDoc {
            id: event_doc_id(event),
            text: [thing.title.as_str(), thing.subtitle.as_str()]
                .iter()
                .filter(|t| !t.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            label: header_label.clone(),
            kind: "event",
            app: Some(thing.thing.app.clone()),
        }],
        EventBody::Search { app, text, .. } => vec![EventDoc {
            id: event_doc_id(event),
            text: text.as_str().to_owned(),
            label: header_label.clone(),
            kind: "event",
            app: Some(app.clone()),
        }],
        EventBody::File { .. } | EventBody::Memory { .. } | EventBody::Area(_) => Vec::new(),
    }
}

/// The sequence numbers of the newest event of each episode id (an episode narrated later is a
/// second event with the same id; only the newest is indexed).
pub(crate) fn newest_episodes(entries: &[Entry]) -> BTreeSet<Seq> {
    let mut newest: BTreeMap<String, Seq> = BTreeMap::new();
    for entry in entries {
        if let BodyState::Present(EventBody::Episode(e)) = &entry.body {
            let slot = newest.entry(e.id.to_string()).or_insert(entry.header.seq);
            *slot = (*slot).max(entry.header.seq);
        }
    }
    newest.into_values().collect()
}

/// The reference of an entry in `space`.
pub(crate) fn event_ref(space: &almanac_core::SpaceId, entry: &Entry) -> EventRef {
    EventRef {
        space: space.clone(),
        replica: entry.header.replica,
        seq: entry.header.seq,
    }
}

/// The documents the index holds for `entry`: none for an erased body or for an episode that a
/// newer event narrates.
pub(crate) fn indexed_docs(
    space: &almanac_core::SpaceId,
    entry: &Entry,
    newest: &BTreeSet<Seq>,
) -> Vec<EventDoc> {
    match &entry.body {
        BodyState::Present(EventBody::Episode(_)) if !newest.contains(&entry.header.seq) => {
            Vec::new()
        }
        BodyState::Present(body) => event_docs(&event_ref(space, entry), &entry.header.label, body),
        BodyState::Erased => Vec::new(),
    }
}

/// The document of an active fact.
pub(crate) fn fact_doc(fact: &Fact) -> Doc {
    Doc {
        id: DocId(fact_doc_id(&fact.id)),
        text: fact.text.as_str().to_owned(),
        at: fact.recorded.0,
        facets: Facets {
            kind: "fact".to_owned(),
            app: None,
            trust: tier(&fact.label),
        },
        class: tag_of(&fact.label),
    }
}

/// Whether `outer` is at least as restrictive as `inner`: no more trusted, no less
/// confidential, every class and source of `inner` carried. (`outer` joined with `inner` is
/// `outer`, as the join normalises.)
pub(crate) fn label_covers(outer: &Label, inner: &Label) -> bool {
    outer.join(inner) == outer.join(outer)
}

/// The join of the labels of every document of `body`, if it has any.
pub(crate) fn documents_label(body: &EventBody) -> Option<Label> {
    body.index_texts()
        .into_iter()
        .map(|t| t.label)
        .reduce(|a, b| a.join(&b))
}

/// Why a `Record` is malformed, or `None` if it is well formed: its label covers the join of its
/// documents' labels, a message passes `Message::check` and was sent by its actor, and an
/// episode's skeleton is trusted and its Space is the record's.
pub(crate) fn record_fault(record: &almanac_core::Record) -> Option<String> {
    let body_fault = match &record.body {
        EventBody::Message(m) => m
            .check()
            .err()
            .map(|f: MessageFault| format!("message: {f:?}"))
            .or_else(|| {
                (m.sender_matches(&record.actor) == SenderCheck::Mismatch)
                    .then(|| "message: sender is not the recording actor".to_owned())
            }),
        EventBody::Episode(e) => (e.skeleton.label.integrity != Integrity::Trusted)
            .then(|| "episode: the skeleton must be trusted".to_owned())
            .or_else(|| (e.space != record.space).then(|| "episode: wrong Space".to_owned())),
        EventBody::Thing { .. }
        | EventBody::File { .. }
        | EventBody::Search { .. }
        | EventBody::Memory { .. }
        | EventBody::Area(_) => None,
    };
    body_fault.or_else(|| {
        documents_label(&record.body)
            .filter(|joined| !label_covers(&record.label, joined))
            .map(|_| "label is less restrictive than the documents it carries".to_owned())
    })
}
