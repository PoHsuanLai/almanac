//! A rebuildable search index, generic and reusable: FTS5 lexical search through rusqlite, an
//! `Embedder` trait and a `VectorIndex` trait with exact scan as the default backend, and
//! reciprocal-rank fusion of the two. It knows nothing of almanac.
//!
//! No `unsafe` and no SQLite extension: exact scan is a BLOB column compared in Rust.

mod doc;
mod embed;
mod exact;
#[cfg(feature = "testing")]
mod fake;
mod fts;
mod fuse;
mod index;
mod state;
mod vector;

pub use doc::{
    Allow, Chunk, ClassTag, Count, Doc, DocId, Facets, Ranked, StoredDoc, TopK, TrustTier,
};
pub use embed::{Classed, EmbedError, Embedder, RetryClass};
pub use exact::{ExactScan, VectorIndex};
#[cfg(feature = "testing")]
pub use fake::{FAKE_DIMS, FakeEmbedder};
pub use fts::{Fts5, SCHEMA_V1, match_expression};
pub use fuse::{CHARS_PER_TOKEN, Fused, HitWhy, RrfK, chunk, fuse_rrf};
pub use index::{Index, IndexError, SearchQuery};
pub use state::{DegradedWhy, IndexEvent, IndexState, StaleWhy, step};
pub use vector::{
    EmbedRole, EmbedderCard, MaxBatch, Metric, PromptPrefixes, SpaceCheck, Urgency, Vector,
    nearest_exact,
};
