//! `Index` as the service's search seam (`almanac_store::SearchIndex`): each method is the
//! inherent one, with its error as the seam's.

use crate::doc::{Allow, Doc, DocId, Ranked, TopK, TrustTier};
use crate::embed::Embedder;
use crate::exact::VectorIndex;
use crate::fuse::RrfK;
use crate::index::{Index, SearchQuery};
use crate::state::IndexState;
use crate::vector::{EmbedderCard, Vector};
use almanac_store::{IndexFailure, SearchIndex};

impl<V: VectorIndex> SearchIndex for Index<V> {
    fn state(&self) -> IndexState {
        Index::<V>::state(self)
    }

    fn rrf_k(&self) -> RrfK {
        Index::<V>::rrf_k(self)
    }

    fn card(&self) -> &EmbedderCard {
        self.parts().1.card()
    }

    fn allow_kinds(&self, kinds: &[&str], trust: Option<TrustTier>) -> Result<Allow, IndexFailure> {
        Index::<V>::allow_kinds(self, kinds, trust).map_err(IndexFailure::from)
    }

    fn lexical(&self, q: &SearchQuery) -> Result<Vec<Ranked>, IndexFailure> {
        Index::<V>::lexical(self, q).map_err(IndexFailure::from)
    }

    fn nearest(&self, v: &Vector, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexFailure> {
        self.parts()
            .1
            .nearest(v, k, allow)
            .map_err(IndexFailure::from)
    }

    async fn rebuild<E: Embedder>(&mut self, docs: Vec<Doc>, e: &E) -> Result<(), IndexFailure> {
        Index::<V>::rebuild(self, docs.into_iter(), e)
            .await
            .map_err(IndexFailure::from)
    }

    async fn sync<E: Embedder>(&mut self, docs: Vec<Doc>, e: &E) -> Result<(), IndexFailure> {
        Index::<V>::sync(self, docs.into_iter(), e)
            .await
            .map_err(IndexFailure::from)
    }

    async fn upsert<E: Embedder>(&mut self, docs: &[Doc], e: &E) -> Result<(), IndexFailure> {
        Index::<V>::upsert(self, docs, e)
            .await
            .map_err(IndexFailure::from)
    }

    fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexFailure> {
        Index::<V>::remove(self, ids).map_err(IndexFailure::from)
    }
}
