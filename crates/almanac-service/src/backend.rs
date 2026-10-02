//! The backend seam: everything the service needs that touches disk, keys, models or time.
//! memoryd supplies the real one; almanac-fake supplies in-memory ones.

use crate::clock::Clock;
use crate::consolidation::Consolidator;
use almanac_core::{ReplicaId, SpaceId, SpaceMeta};
use almanac_seal::{KeyError, KeyStore, SpaceKey};
use eventlog::{LogError, LogWrite};
use memfiles::{Vault, VaultError};
use recall::{Embedder, Index, IndexError, VectorIndex};

/// Why a backend could not open a Space's stores.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// The key store said no.
    #[error("keys: {0}")]
    Keys(#[from] KeyError),
    /// The event log could not be opened.
    #[error("event log: {0}")]
    Log(#[from] LogError),
    /// The vault could not be opened.
    #[error("vault: {0}")]
    Files(#[from] VaultError),
    /// The index could not be opened.
    #[error("index: {0}")]
    Index(#[from] IndexError),
}

/// The service's seams, bundled: closed sets of implementations are types, never `dyn`.
pub trait Backend: Send + Sync {
    /// Where Space keys live.
    type Keys: KeyStore;
    /// One Space's event log.
    type Log: LogWrite + Send;
    /// One Space's file vault.
    type Files: Vault;
    /// One Space's vector index.
    type Vectors: VectorIndex;
    /// The embedder (background priority for indexing).
    type Embedder: Embedder;
    /// The consolidation model.
    type Consolidator: Consolidator;
    /// The clock.
    type Clock: Clock;

    /// The key store.
    fn keys(&self) -> &Self::Keys;
    /// The embedder.
    fn embedder(&self) -> &Self::Embedder;
    /// The consolidator.
    fn consolidator(&self) -> &Self::Consolidator;
    /// The clock.
    fn clock(&self) -> &Self::Clock;
    /// Opens (or creates) the Space's event log, keyed from `key`.
    fn open_log(
        &self,
        space: &SpaceId,
        replica: ReplicaId,
        key: &SpaceKey,
    ) -> Result<Self::Log, BackendError>;
    /// Opens the Space's vault, sealed or plain as `meta.vault` says.
    fn open_files(&self, meta: &SpaceMeta, key: &SpaceKey) -> Result<Self::Files, BackendError>;
    /// Opens the Space's recall index.
    fn open_index(
        &self,
        space: &SpaceId,
        key: &SpaceKey,
    ) -> Result<Index<Self::Vectors>, BackendError>;
}
