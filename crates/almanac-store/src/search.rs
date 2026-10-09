//! The search index seam: what the service asks of an index, and nothing about how it stores.

use crate::doc::{Allow, Doc, DocId, Ranked, TopK, TrustTier};
use crate::embed::{EmbedError, Embedder};
use crate::fuse::RrfK;
use crate::state::IndexState;
use crate::vector::{EmbedderCard, Urgency, Vector};
use std::future::Future;

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

/// Why an index operation failed, whatever the index stores in.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexFailure {
    /// Embedding failed.
    #[error("embedding: {0}")]
    Embed(#[from] EmbedError),
    /// The vectors and the embedder are in different spaces (rebuild).
    #[error("the embedder is not the one the index was built with")]
    CardMismatch,
    /// The index's storage said no.
    #[error("index storage: {0}")]
    Storage(String),
}

/// A search index the service keeps per Space: lexical and vector halves, rebuildable from the
/// truth. `recall::Index` is the implementation; the service never names it.
///
/// The async methods return `Send` futures, so a service over any implementation can run on a
/// multi-threaded runtime.
pub trait SearchIndex: Send {
    /// Where the index is.
    fn state(&self) -> IndexState;
    /// The reciprocal-rank-fusion constant the index fuses with.
    fn rrf_k(&self) -> RrfK;
    /// The vector space the stored vectors live in.
    fn card(&self) -> &EmbedderCard;
    /// The documents of the given facet kinds and trust tier (`None` is any), as the allow-list
    /// of a search.
    fn allow_kinds(&self, kinds: &[&str], trust: Option<TrustTier>) -> Result<Allow, IndexFailure>;
    /// The lexical half's ranked list for `q`.
    fn lexical(&self, q: &SearchQuery) -> Result<Vec<Ranked>, IndexFailure>;
    /// The `k` documents nearest to `v` among the allowed ones, best first.
    fn nearest(&self, v: &Vector, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexFailure>;
    /// Clears and rebuilds from `docs`, embedding in the background. A lost embedder that can
    /// come back leaves the index lexical-only and returns `Ok`.
    fn rebuild<E: Embedder>(
        &mut self,
        docs: Vec<Doc>,
        e: &E,
    ) -> impl Future<Output = Result<(), IndexFailure>> + Send;
    /// Brings the index in line with `docs` (the truth) without re-embedding what is already
    /// right.
    fn sync<E: Embedder>(
        &mut self,
        docs: Vec<Doc>,
        e: &E,
    ) -> impl Future<Output = Result<(), IndexFailure>> + Send;
    /// Adds or replaces documents.
    fn upsert<E: Embedder>(
        &mut self,
        docs: &[Doc],
        e: &E,
    ) -> impl Future<Output = Result<(), IndexFailure>> + Send;
    /// Removes documents from both halves; how many were there.
    fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexFailure>;
}
