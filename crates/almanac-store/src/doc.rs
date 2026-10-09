//! What is indexed and what searches return.

use std::collections::BTreeSet;

/// A document's id: opaque to recall (almanac uses `f:<fact>` and `e:<replica>:<seq>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocId(pub String);

/// A count of documents or results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Count(pub u32);

/// How many results a nearest-neighbour or lexical query returns at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TopK(pub u32);

/// How much a document's source can be trusted, as a facet to filter on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrustTier {
    /// Written by the person or authored by an app.
    Trusted,
    /// Written by someone else.
    Untrusted,
}

/// A document's data class, as an opaque tag. recall names no porter type, so it carries the tag
/// from the caller to the embedder untouched and never reads it: the caller that builds the
/// documents and the embedder that receives them agree on its spelling (almanac uses the
/// data-class slug, `mail`, `notes`, and so on). The empty tag, the default, says "no class": an
/// embedder treats it as its own strictest pin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ClassTag(pub String);

/// What a search can filter on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Facets {
    /// What kind of document (`fact`, `event`).
    pub kind: String,
    /// The app it is about, if any.
    pub app: Option<String>,
    /// Its trust tier.
    pub trust: TrustTier,
}

/// One indexed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    /// Its id.
    pub id: DocId,
    /// What is searched.
    pub text: String,
    /// When it was learned or happened (seconds since the epoch).
    pub at: i64,
    /// What to filter on.
    pub facets: Facets,
    /// The data class of its text: the embedder keeps documents of different classes in
    /// different sessions, so a class's on-device floor holds for exactly its own texts.
    pub class: ClassTag,
}

/// Which documents a query may return.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Allow {
    /// All of them.
    #[default]
    Everything,
    /// Only these.
    Only(BTreeSet<DocId>),
}

impl Allow {
    /// Whether `id` may be returned.
    pub fn permits(&self, id: &DocId) -> bool {
        match self {
            Allow::Everything => true,
            Allow::Only(ids) => ids.contains(id),
        }
    }
}

/// One entry of a ranked list: its place, from 1.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ranked {
    /// The document.
    pub id: DocId,
    /// Its rank in the list that produced it, from 1.
    pub rank: u32,
}

/// A piece of a long text, sized for the embedder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Its position in the text, from 0.
    pub index: u32,
    /// Its text.
    pub text: String,
}

/// A document as the lexical half stores it: [`Doc`] without its class (the class decides who
/// may embed a text, so it is not kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDoc {
    /// The id.
    pub id: DocId,
    /// The text.
    pub text: String,
    /// When (seconds).
    pub at: i64,
    /// Its facets.
    pub facets: Facets,
}

impl Doc {
    /// This document as the lexical half would store it.
    pub fn stored(&self) -> StoredDoc {
        StoredDoc {
            id: self.id.clone(),
            text: self.text.clone(),
            at: self.at,
            facets: self.facets.clone(),
        }
    }
}
