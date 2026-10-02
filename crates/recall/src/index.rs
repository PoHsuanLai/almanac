//! `Index`: lexical and vector search together, rebuildable from the truth.

use crate::doc::{Allow, Doc, DocId, Ranked, TopK};
use crate::embed::{EmbedError, Embedder};
use crate::exact::VectorIndex;
use crate::fts::Fts5;
use crate::fuse::{Fused, RrfK};
use crate::state::IndexState;
use crate::vector::Urgency;

/// Why an index operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    /// Embedding failed.
    #[error("embedding: {0}")]
    Embed(#[from] EmbedError),
    /// The vectors and the embedder are in different spaces (rebuild).
    #[error("the embedder is not the one the index was built with")]
    CardMismatch,
    /// SQLite said no.
    #[error("sqlite: {0}")]
    Sqlite(String),
}

impl From<rusqlite::Error> for IndexError {
    fn from(e: rusqlite::Error) -> Self {
        IndexError::Sqlite(e.to_string())
    }
}

/// One search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// What to look for.
    pub text: String,
    /// How many results.
    pub k: TopK,
    /// Which documents may match.
    pub allow: Allow,
    /// How urgent the query embedding is.
    pub urgency: Urgency,
}

/// Lexical + vector search. `search` falls back to lexical only, and says so in `state`, when
/// the embedder is unavailable: search never fails because the GPU is busy.
#[derive(Debug)]
pub struct Index<V: VectorIndex> {
    fts: Fts5,
    vectors: V,
    state: IndexState,
    k: RrfK,
}

impl<V: VectorIndex> Index<V> {
    /// An index over its two halves, not yet built.
    pub fn new(fts: Fts5, vectors: V) -> Self {
        Self {
            fts,
            vectors,
            state: IndexState::Absent,
            k: RrfK(60),
        }
    }

    /// Where the index is.
    pub fn state(&self) -> IndexState {
        self.state
    }

    /// The fusion constant.
    pub fn rrf_k(&self) -> RrfK {
        self.k
    }

    /// The two halves.
    pub fn parts(&self) -> (&Fts5, &V) {
        (&self.fts, &self.vectors)
    }

    /// Clears and rebuilds from `docs`, embedding in the background.
    pub async fn rebuild(
        &mut self,
        docs: impl Iterator<Item = Doc>,
        e: &impl Embedder,
    ) -> Result<(), IndexError> {
        let _ = (docs, e);
        todo!(
            "clear both halves, chunk, embed in batches (Urgency::Background), upsert, step the state"
        )
    }

    /// Adds or replaces documents.
    pub async fn upsert(&mut self, docs: &[Doc], e: &impl Embedder) -> Result<(), IndexError> {
        let _ = (docs, e);
        todo!(
            "fts upsert; embed and vector upsert, or mark LexicalOnly when the embedder is unavailable"
        )
    }

    /// Removes documents from both halves.
    pub fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError> {
        let _ = ids;
        todo!("remove from fts and vectors; the count is the larger of the two")
    }

    /// Fused lexical and semantic results.
    pub async fn search(
        &self,
        q: &SearchQuery,
        e: &impl Embedder,
    ) -> Result<Vec<Fused>, IndexError> {
        let _ = (q, e);
        todo!(
            "fts search; embed the query; vector nearest; fuse_rrf; lexical only when Unavailable"
        )
    }

    /// The lexical half's list for `q`, for callers that fuse themselves.
    pub fn lexical(&self, q: &SearchQuery) -> Result<Vec<Ranked>, IndexError> {
        self.fts.search(&q.text, q.k, &q.allow)
    }
}
