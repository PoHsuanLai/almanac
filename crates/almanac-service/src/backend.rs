//! The backend seam: everything the service needs that touches disk, keys, models or time.
//! memoryd supplies the real one; almanac-fake supplies in-memory ones.

use crate::clock::Clock;
use crate::consolidation::Consolidator;
use almanac_core::{ReplicaId, SpaceId, SpaceMeta};
use almanac_seal::{KeyError, KeyStore, SpaceKey};
use almanac_store::{Embedder, IndexFailure, LogError, LogWrite, SearchIndex, Vault, VaultError};

/// Why a backend could not open a Space's stores.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
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
    Index(#[from] IndexFailure),
    /// A Space's directories could not be removed.
    #[error("removing a Space: {0}")]
    Remove(String),
}

/// The service's seams, bundled: closed sets of implementations are types, never `dyn`.
pub trait Backend: Send + Sync {
    /// Where Space keys live.
    type Keys: KeyStore;
    /// One Space's event log.
    type Log: LogWrite + Send;
    /// One Space's file vault.
    type Files: Vault;
    /// One Space's search index.
    type Index: SearchIndex;
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
    /// Sixteen random bytes. The one place real randomness enters the service: it is mixed into
    /// every id the service mints (a fact, a consolidation run), so two ids minted in the same
    /// second by a restarted daemon do not collide. memoryd reads the OS; tests count.
    fn random(&self) -> [u8; 16];
    /// Removes everything a Space stored on disk (its log, index and files), after its key was
    /// destroyed. Called once, when the Space's deletion finishes; removing what is already gone
    /// is not an error.
    fn remove_space(&self, space: &SpaceId) -> Result<(), BackendError>;
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
    fn open_index(&self, space: &SpaceId, key: &SpaceKey) -> Result<Self::Index, BackendError>;
}
