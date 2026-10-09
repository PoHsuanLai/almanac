//! The seams almanac's service stands on, as traits and types only.
//!
//! * the event log: [`LogRead`], [`LogWrite`], [`Entry`], [`Header`] and the chain check,
//! * the file vault: [`Vault`] and [`VaultPath`],
//! * the search index: [`SearchIndex`], with the documents, vectors and [`IndexState`] it speaks
//!   in, and the [`Embedder`] it is fed by.
//!
//! Each store crate implements them: `eventlog` (`SqliteLog`), `memfiles` (`PlainDir`,
//! `SealedDir`), `recall` (`Index`), plus the in-memory ones behind their `testing` features.
//! The service depends on this crate and on none of them, so a build of the service links no
//! SQLite, SQLCipher or OpenSSL; whoever builds a service picks the concrete stores
//! (`almanac-local`, `almanac-fake`, `memoryd`). Every name here is re-exported from the crate
//! it used to live in, so old paths keep working.
//!
//! ```
//! use almanac_fake::{FAKE_SPACES, FakeBackend, NOW, ScriptedConsolidator, fake_space_metas};
//! use almanac_seal::SpaceKey;
//! use almanac_service::Backend;
//! use almanac_store::{LogRead, Vault, VaultPath};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = FakeBackend::new(ScriptedConsolidator::default());
//! let meta = fake_space_metas(NOW).into_iter().next().ok_or("a fake Space")?;
//! let key = SpaceKey::from_bytes([7; 32]);
//!
//! // The service sees the stores only through the seams.
//! let log = backend.open_log(&meta.id, meta.replica, &key)?;
//! assert_eq!(log.head()?.seq.0, 0);
//! let files = backend.open_files(&meta, &key)?;
//! assert!(files.list(&VaultPath::facts_dir())?.is_empty());
//! assert_eq!(FAKE_SPACES.len(), 3);
//! # Ok(())
//! # }
//! ```

mod chain;
mod doc;
mod embed;
mod fuse;
mod header;
mod log;
mod search;
mod state;
mod vault;
mod vector;

pub use chain::{BodyState, Entry, verify_chain};
pub use doc::{
    Allow, Chunk, ClassTag, Count, Doc, DocId, Facets, Ranked, StoredDoc, TopK, TrustTier,
};
pub use embed::{Classed, EmbedError, Embedder, RetryClass};
pub use fuse::{CHARS_PER_TOKEN, Fused, HitWhy, RrfK, chunk, fuse_rrf};
pub use header::{
    GENESIS_CONTEXT, HEADER_MAGIC, Header, NewHeader, body_digest, genesis_link, header_bytes, link,
};
pub use log::{LogError, LogRead, LogWrite, PageQuery, RoleFilter};
pub use search::{IndexFailure, SearchIndex, SearchQuery};
pub use state::{DegradedWhy, IndexEvent, IndexState, StaleWhy, step};
pub use vault::{FACTS_DIR, PENDING_DIR, PROCEDURES_DIR, Vault, VaultError, VaultPath};
pub use vector::{
    EmbedRole, EmbedderCard, MaxBatch, Metric, PromptPrefixes, SpaceCheck, Urgency, Vector,
    nearest_exact,
};
