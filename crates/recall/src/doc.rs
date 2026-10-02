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
