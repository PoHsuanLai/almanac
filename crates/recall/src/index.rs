//! `Index`: lexical and vector search together, rebuildable from the truth.

use crate::doc::{Allow, ClassTag, Count, Doc, DocId, Ranked, StoredDoc, TopK, TrustTier};
use crate::embed::{Classed, EmbedError, Embedder, RetryClass};
use crate::exact::VectorIndex;
use crate::fts::Fts5;
use crate::fuse::{Fused, RrfK, chunk, fuse_rrf};
use crate::state::{DegradedWhy, IndexEvent, IndexState, step};
use crate::vector::{EmbedRole, SpaceCheck, Urgency, Vector};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

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
///
/// A long document is chunked to the card's `max_tokens`, the chunks are embedded in batches of
/// `max_batch` and averaged into the document's one vector. The state sits behind a mutex only
/// because `search(&self)` reports a lost embedder; nothing else shares it.
#[derive(Debug)]
pub struct Index<V: VectorIndex> {
    fts: Fts5,
    vectors: V,
    state: Mutex<IndexState>,
    k: RrfK,
}

impl<V: VectorIndex> Index<V> {
    /// An index over its two halves, not yet built.
    pub fn new(fts: Fts5, vectors: V) -> Self {
        Self {
            fts,
            vectors,
            state: Mutex::new(IndexState::Absent),
            k: RrfK(60),
        }
    }

    /// Where the index is.
    pub fn state(&self) -> IndexState {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn apply(&self, event: IndexEvent) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = step(*state, event);
    }

    /// An index filled by `upsert` alone is built: Absent becomes Ready.
    fn built_by_upserts(&self) {
        if self.state() == IndexState::Absent {
            self.apply(IndexEvent::Begin { total: Count(0) });
            self.apply(IndexEvent::Finished);
        }
    }

    /// Records that the embedder failed and how.
    fn degrade(&self, error: &EmbedError) {
        self.lexical_only(degraded_why(error));
    }

    fn lexical_only(&self, why: DegradedWhy) {
        self.built_by_upserts();
        self.apply(IndexEvent::EmbedderLost(why));
    }

    /// Documents all of whose classes were refused leave the index lexical-only; some refused
    /// and some embedded is a normal index (each document answers for its own class).
    fn note_refused(&self, embedded: &Embedded) {
        if embedded.vectors.is_empty() && !embedded.refused.is_empty() {
            self.lexical_only(DegradedWhy::EmbedderRefused);
        }
    }

    /// Records the vector space the stored vectors are in, once they are the embedder's.
    fn record_space(&self, e: &impl Embedder) -> Result<(), IndexError> {
        self.fts.record_space(e.card())
    }

    /// The fusion constant.
    pub fn rrf_k(&self) -> RrfK {
        self.k
    }

    /// The documents of the given facet kinds and trust tier (`None` is any), as the allow-list
    /// of a search.
    pub fn allow_kinds(
        &self,
        kinds: &[&str],
        trust: Option<TrustTier>,
    ) -> Result<Allow, IndexError> {
        self.fts.ids_of(kinds, trust).map(Allow::Only)
    }

    /// The two halves.
    pub fn parts(&self) -> (&Fts5, &V) {
        (&self.fts, &self.vectors)
    }

    fn same_space(&self, e: &impl Embedder) -> Result<(), IndexError> {
        match e.card().space_vs(self.vectors.card()) {
            SpaceCheck::Same => Ok(()),
            SpaceCheck::Different => Err(IndexError::CardMismatch),
        }
    }

    /// Clears and rebuilds from `docs`, embedding in the background. Lexical search works as soon
    /// as the documents are in; a lost embedder leaves the index `LexicalOnly` and returns `Ok`.
    pub async fn rebuild(
        &mut self,
        docs: impl Iterator<Item = Doc>,
        e: &impl Embedder,
    ) -> Result<(), IndexError> {
        self.same_space(e)?;
        let docs: Vec<Doc> = docs.collect();
        self.apply(IndexEvent::Begin {
            total: count_of(docs.len()),
        });
        self.fts.clear()?;
        self.vectors.clear()?;
        self.record_space(e)?;
        self.fts.insert_new(&docs)?;
        let group = usize::try_from(e.card().max_batch.0.max(1)).unwrap_or(1);
        let mut done = 0usize;
        let mut embedded_any = false;
        let mut refused_any = false;
        for part in docs.chunks(group) {
            match embed_docs(part, e, Urgency::Background).await {
                Ok(embedded) => {
                    self.vectors.upsert(&embedded.vectors)?;
                    embedded_any |= !embedded.vectors.is_empty();
                    refused_any |= !embedded.refused.is_empty();
                }
                Err(error) => return self.embedding_failed(error),
            }
            done += part.len();
            self.apply(IndexEvent::Progress {
                done: count_of(done),
            });
        }
        self.apply(IndexEvent::Finished);
        if refused_any && !embedded_any {
            self.lexical_only(DegradedWhy::EmbedderRefused);
        }
        Ok(())
    }

    /// Brings the index in line with `docs` (the truth) without embedding what is already
    /// right. An index whose recorded vector space is the embedder's is *adopted*: documents it
    /// holds with the same text and facets keep their vectors, documents that are new, changed
    /// or still without a vector (an embedder that was away, a class it refused) are embedded,
    /// and documents the truth no longer has are removed. Anything else (no recorded space, a
    /// different model, length or prefix) is a full [`Index::rebuild`].
    pub async fn sync(
        &mut self,
        docs: impl Iterator<Item = Doc>,
        e: &impl Embedder,
    ) -> Result<(), IndexError> {
        self.same_space(e)?;
        let docs: Vec<Doc> = docs.collect();
        if self.fts.recorded_space()? != Some(e.card().space_key()) {
            return self.rebuild(docs.into_iter(), e).await;
        }
        let held: BTreeMap<DocId, StoredDoc> = self
            .fts
            .stored()?
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect();
        let vectored = self.vectors.ids()?;
        let truth: BTreeSet<&DocId> = docs.iter().map(|d| &d.id).collect();
        let gone: Vec<DocId> = held
            .keys()
            .chain(vectored.iter())
            .filter(|id| !truth.contains(id))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let max_tokens = e.card().max_tokens;
        let due: Vec<Doc> = docs
            .into_iter()
            .filter(|d| match held.get(&d.id) {
                Some(stored) if *stored == d.stored() => {
                    !vectored.contains(&d.id) && !chunk(&d.text, max_tokens).is_empty()
                }
                _ => true,
            })
            .collect();
        if !gone.is_empty() {
            self.remove(&gone)?;
        }
        if due.is_empty() {
            self.built_by_upserts();
            return Ok(());
        }
        self.upsert(&due, e).await
    }

    /// A Retry-class failure leaves the lexical index in place and the work to retry; a fatal
    /// one is reported.
    fn embedding_failed(&self, error: EmbedError) -> Result<(), IndexError> {
        self.degrade(&error);
        match error.retry_class() {
            RetryClass::Retry => Ok(()),
            RetryClass::Fatal => Err(error.into()),
        }
    }

    /// Adds or replaces documents. The lexical half is written first; if embedding then fails
    /// with a Retry-class error the index is `LexicalOnly` and the call succeeds.
    pub async fn upsert(&mut self, docs: &[Doc], e: &impl Embedder) -> Result<(), IndexError> {
        self.same_space(e)?;
        self.fts.upsert(docs)?;
        if self.fts.recorded_space()?.is_none() {
            self.record_space(e)?;
        }
        match embed_docs(docs, e, Urgency::Background).await {
            Ok(embedded) => {
                let ids: Vec<DocId> = docs.iter().map(|d| d.id.clone()).collect();
                self.vectors.remove(&ids)?;
                self.vectors.upsert(&embedded.vectors)?;
                self.built_by_upserts();
                self.note_refused(&embedded);
                Ok(())
            }
            Err(error) => self.embedding_failed(error),
        }
    }

    /// Removes documents from both halves.
    pub fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError> {
        let lexical = self.fts.remove(ids)?;
        let semantic = self.vectors.remove(ids)?;
        Ok(lexical.max(semantic))
    }

    /// Fused lexical and semantic results.
    pub async fn search(
        &self,
        q: &SearchQuery,
        e: &impl Embedder,
    ) -> Result<Vec<Fused>, IndexError> {
        let lexical = self.fts.search(&q.text, q.k, &q.allow)?;
        let semantic = match self.same_space(e) {
            Err(_) => {
                self.apply(IndexEvent::CardChanged);
                Vec::new()
            }
            Ok(()) => self.semantic(q, e).await?,
        };
        let mut fused = fuse_rrf(&[lexical, semantic], self.k);
        fused.truncate(usize::try_from(q.k.0).unwrap_or(usize::MAX));
        Ok(fused)
    }

    /// The vector half's list; empty, with the state updated, when the embedder is away.
    async fn semantic(
        &self,
        q: &SearchQuery,
        e: &impl Embedder,
    ) -> Result<Vec<Ranked>, IndexError> {
        let Some(first) = chunk(&q.text, e.card().max_tokens).into_iter().next() else {
            return Ok(Vec::new());
        };
        match e.embed(&[first.text], EmbedRole::Query, q.urgency).await {
            Ok(mut vectors) => match vectors.pop() {
                Some(v) => self.vectors.nearest(&v, q.k, &q.allow),
                None => Ok(Vec::new()),
            },
            Err(error) => {
                self.degrade(&error);
                match error.retry_class() {
                    RetryClass::Retry => Ok(Vec::new()),
                    RetryClass::Fatal => Err(error.into()),
                }
            }
        }
    }

    /// The lexical half's list for `q`, for callers that fuse themselves.
    pub fn lexical(&self, q: &SearchQuery) -> Result<Vec<Ranked>, IndexError> {
        self.fts.search(&q.text, q.k, &q.allow)
    }
}

fn count_of(n: usize) -> Count {
    Count(u32::try_from(n).unwrap_or(u32::MAX))
}

fn degraded_why(error: &EmbedError) -> DegradedWhy {
    match error {
        EmbedError::Refused(_) => DegradedWhy::EmbedderRefused,
        EmbedError::Unavailable
        | EmbedError::Busy
        | EmbedError::TooLong
        | EmbedError::Failed { .. } => DegradedWhy::EmbedderUnavailable,
    }
}

/// What embedding a set of documents gave.
#[derive(Debug)]
struct Embedded {
    /// One vector per embedded document.
    vectors: Vec<(DocId, Vector)>,
    /// Documents whose data class the embedder refused: they stay lexical.
    refused: Vec<DocId>,
}

/// One batch of chunks embedded as documents, each answering for itself: `None` is a text whose
/// class the embedder refused. A refusal of the whole batch is retried class by class (a refused
/// class fails its own documents only, never another class's); any other failure fails the batch.
async fn embed_batch(
    e: &impl Embedder,
    texts: &[Classed],
    urgency: Urgency,
    width: usize,
) -> Result<Vec<Option<Vector>>, EmbedError> {
    match e.embed_classed(texts, EmbedRole::Document, urgency).await {
        Ok(vectors) => checked(vectors, texts.len(), width),
        Err(EmbedError::Refused(_)) => {
            let mut groups: BTreeMap<&ClassTag, Vec<usize>> = BTreeMap::new();
            for (at, t) in texts.iter().enumerate() {
                groups.entry(&t.class).or_default().push(at);
            }
            let mut out: Vec<Option<Vector>> = vec![None; texts.len()];
            if groups.len() < 2 {
                return Ok(out);
            }
            for at in groups.into_values() {
                let part: Vec<Classed> = at.iter().map(|&i| texts[i].clone()).collect();
                match e.embed_classed(&part, EmbedRole::Document, urgency).await {
                    Ok(vectors) => {
                        for (slot, v) in at.iter().zip(checked(vectors, part.len(), width)?) {
                            out[*slot] = v;
                        }
                    }
                    Err(EmbedError::Refused(_)) => {}
                    Err(other) => return Err(other),
                }
            }
            Ok(out)
        }
        Err(other) => Err(other),
    }
}

fn checked(
    vectors: Vec<Vector>,
    count: usize,
    width: usize,
) -> Result<Vec<Option<Vector>>, EmbedError> {
    if vectors.len() != count || vectors.iter().any(|v| v.0.len() != width) {
        return Err(EmbedError::Failed {
            class: RetryClass::Fatal,
            why: format!("expected {count} vectors of {width} floats"),
        });
    }
    Ok(vectors.into_iter().map(Some).collect())
}

/// One vector per document with text: its chunks embedded in batches of `max_batch` (as
/// documents, so the model's prefix applies), checked for count and width, and averaged. A
/// document with a chunk the embedder refused has no vector and is listed in `refused`.
async fn embed_docs(
    docs: &[Doc],
    e: &impl Embedder,
    urgency: Urgency,
) -> Result<Embedded, EmbedError> {
    let card = e.card();
    let pieces: Vec<(usize, Classed)> = docs
        .iter()
        .enumerate()
        .flat_map(|(slot, d)| {
            chunk(&d.text, card.max_tokens).into_iter().map(move |c| {
                let class = d.class.clone();
                (
                    slot,
                    Classed {
                        class,
                        text: c.text,
                    },
                )
            })
        })
        .collect();
    let width = usize::try_from(card.dims).unwrap_or(usize::MAX);
    let mut sums: Vec<Option<(Vec<f32>, u32)>> = vec![None; docs.len()];
    let mut refused: BTreeSet<usize> = BTreeSet::new();
    let mut from = 0usize;
    for size in card.batch_sizes(pieces.len()) {
        let batch = &pieces[from..from + size];
        let texts: Vec<Classed> = batch.iter().map(|(_, t)| t.clone()).collect();
        let vectors = embed_batch(e, &texts, urgency, width).await?;
        for ((slot, _), vector) in batch.iter().zip(vectors) {
            let Some(vector) = vector else {
                refused.insert(*slot);
                continue;
            };
            let (sum, n) = sums[*slot].get_or_insert_with(|| (vec![0.0; width], 0));
            sum.iter_mut().zip(&vector.0).for_each(|(s, x)| *s += x);
            *n += 1;
        }
        from += size;
    }
    let vectors = docs
        .iter()
        .zip(sums)
        .enumerate()
        .filter(|(slot, _)| !refused.contains(slot))
        .filter_map(|(_, (d, sum))| {
            sum.map(|(mut v, n)| {
                let n = n as f32;
                v.iter_mut().for_each(|x| *x /= n);
                (d.id.clone(), Vector(v))
            })
        })
        .collect();
    let refused = refused
        .into_iter()
        .map(|slot| docs[slot].id.clone())
        .collect();
    Ok(Embedded { vectors, refused })
}
