//! What of an event recall indexes, and under which document id: a pure view of the body, so the
//! index is rebuildable from the log.

use crate::event::{EventBody, EventRef};
use crate::query::RecallOver;
use crate::text::hex_of;
use prov::Label;

/// Which part of an event a document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexPart {
    /// A message's text.
    Message,
    /// An episode's trusted skeleton.
    Skeleton,
    /// An episode's model-written narrative (its own label).
    Narrative,
}

/// One document an event contributes to the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexText {
    /// Which part.
    pub part: IndexPart,
    /// What is searched.
    pub text: String,
    /// Its label: the part's own, so a tainted narrative never taints its skeleton's hit.
    pub label: Label,
}

impl IndexPart {
    /// The document id's prefix: `e` for a message and a skeleton (an event body), `n` for a
    /// narrative.
    pub const fn prefix(self) -> &'static str {
        match self {
            IndexPart::Message | IndexPart::Skeleton => "e",
            IndexPart::Narrative => "n",
        }
    }

    /// The `Facets.kind` of the document.
    pub const fn facet_kind(self) -> &'static str {
        match self {
            IndexPart::Message => "message",
            IndexPart::Skeleton => "episode",
            IndexPart::Narrative => "narrative",
        }
    }

    /// The document id: `e:<replica>:<seq>` or `n:<replica>:<seq>`. Both parts of a hit map back
    /// to `MemoryItem::Event` of the same event.
    pub fn doc_id(self, event: &EventRef) -> String {
        format!(
            "{}:{}:{}",
            self.prefix(),
            hex_of(&event.replica.0),
            event.seq.0
        )
    }
}

impl RecallOver {
    /// The `Facets.kind` values a search over this covers. `Events` is every event document
    /// with text; `Messages` and `Episodes` narrow it.
    pub const fn facet_kinds(self) -> &'static [&'static str] {
        match self {
            RecallOver::Facts => &["fact"],
            RecallOver::Events => &["event", "message", "episode", "narrative"],
            RecallOver::Messages => &["message"],
            RecallOver::Episodes => &["episode", "narrative"],
            RecallOver::Both => &["fact", "event", "message", "episode", "narrative"],
        }
    }
}

impl EventBody {
    /// The documents this body adds to the index. Messages and episodes carry their text in the
    /// typed body, so it is a pure function of it; other bodies' text (thing titles, search
    /// text) is built by the service from their views and is not listed here.
    pub fn index_texts(&self) -> Vec<IndexText> {
        match self {
            EventBody::Message(m) => vec![IndexText {
                part: IndexPart::Message,
                text: m.text(),
                label: m.label.clone(),
            }],
            EventBody::Episode(e) => {
                let skeleton = IndexText {
                    part: IndexPart::Skeleton,
                    text: e.skeleton.text(),
                    label: e.skeleton.label.clone(),
                };
                std::iter::once(skeleton)
                    .chain(e.narrative.iter().map(|n| IndexText {
                        part: IndexPart::Narrative,
                        text: n.text.as_str().to_owned(),
                        label: n.label.clone(),
                    }))
                    .collect()
            }
            EventBody::Thing { .. }
            | EventBody::File { .. }
            | EventBody::Search { .. }
            | EventBody::Memory { .. }
            | EventBody::Area(_) => Vec::new(),
        }
    }
}
