//! `FakeBackend`: every seam in memory, and the service built over it.

use crate::clock::SteppedClock;
use crate::consolidator::ScriptedConsolidator;
use almanac_core::{ReplicaId, RuleSet, SpaceId, SpaceMeta, UnixSeconds};
use almanac_seal::{MemoryKeys, Purpose, SpaceKey, derive};
use almanac_service::{Backend, BackendError, MemoryService};
use eventlog::MemoryLog;
use memfiles::{MemoryVault, Vault, VaultError, VaultPath};
use recall::{ExactScan, FakeEmbedder, Fts5, Index};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// The three Spaces every fake service knows.
pub const FAKE_SPACES: [&str; 3] = ["work", "home", "desktop"];

/// A Space's in-memory vault that the backend also keeps, so a test can edit files the way a
/// person with an editor would, behind the service's back.
#[derive(Debug, Clone, Default)]
pub struct SharedVault(Arc<MemoryVault>);

impl Vault for SharedVault {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        self.0.list(dir)
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        self.0.read(p)
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        self.0.write_atomic(p, bytes)
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        self.0.remove(p)
    }
}

/// In-memory keys, log, vault, index (an in-memory SQLite), embedder, a scripted
/// consolidator and a stepped clock (it stands at `NOW` until a test advances it).
#[derive(Debug)]
pub struct FakeBackend {
    keys: MemoryKeys,
    embedder: FakeEmbedder,
    consolidator: ScriptedConsolidator,
    clock: SteppedClock,
    draws: AtomicU64,
    removed: Mutex<Vec<SpaceId>>,
    vaults: Mutex<BTreeMap<SpaceId, SharedVault>>,
}

impl FakeBackend {
    /// A backend whose consolidator answers from `consolidator`.
    pub fn new(consolidator: ScriptedConsolidator) -> Self {
        Self {
            keys: MemoryKeys::new(),
            embedder: FakeEmbedder::new(),
            consolidator,
            clock: SteppedClock::default(),
            draws: AtomicU64::new(0),
            removed: Mutex::new(Vec::new()),
            vaults: Mutex::new(BTreeMap::new()),
        }
    }

    /// Moves the clock forward (retention, expiry and plan lapse tests).
    pub fn advance_clock(&self, seconds: i64) {
        self.clock.advance(seconds);
    }

    /// A Space's vault, the same files the service reads and writes.
    pub fn vault_of(&self, space: &SpaceId) -> SharedVault {
        self.vaults
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(space.clone())
            .or_default()
            .clone()
    }

    /// The Spaces whose directories the service asked to remove, in order.
    pub fn removed_spaces(&self) -> Vec<SpaceId> {
        self.removed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The key store, for tests that lock it.
    pub fn memory_keys(&self) -> &MemoryKeys {
        &self.keys
    }
}

impl Backend for FakeBackend {
    type Keys = MemoryKeys;
    type Log = MemoryLog;
    type Files = SharedVault;
    type Vectors = ExactScan;
    type Embedder = FakeEmbedder;
    type Consolidator = ScriptedConsolidator;
    type Clock = SteppedClock;

    fn keys(&self) -> &MemoryKeys {
        &self.keys
    }

    fn embedder(&self) -> &FakeEmbedder {
        &self.embedder
    }

    fn consolidator(&self) -> &ScriptedConsolidator {
        &self.consolidator
    }

    fn clock(&self) -> &SteppedClock {
        &self.clock
    }

    /// A counter, so the fake's ids are reproducible.
    fn random(&self) -> [u8; 16] {
        let n = self.draws.fetch_add(1, Ordering::Relaxed);
        let mut out = [0u8; 16];
        out[8..].copy_from_slice(&n.to_be_bytes());
        out
    }

    fn remove_space(&self, space: &SpaceId) -> Result<(), BackendError> {
        self.vaults
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(space);
        self.removed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(space.clone());
        Ok(())
    }

    fn open_log(
        &self,
        space: &SpaceId,
        replica: ReplicaId,
        key: &SpaceKey,
    ) -> Result<MemoryLog, BackendError> {
        Ok(MemoryLog::new(
            space,
            replica,
            derive(key, space, Purpose::Digest),
        ))
    }

    fn open_files(&self, meta: &SpaceMeta, _key: &SpaceKey) -> Result<SharedVault, BackendError> {
        Ok(self.vault_of(&meta.id))
    }

    fn open_index(
        &self,
        _space: &SpaceId,
        _key: &SpaceKey,
    ) -> Result<Index<ExactScan>, BackendError> {
        let card = recall::Embedder::card(&self.embedder).clone();
        let fts =
            Fts5::new(rusqlite::Connection::open_in_memory().map_err(recall::IndexError::from)?);
        fts.create()?;
        let vectors = ExactScan::in_memory(card)?;
        Ok(Index::new(fts, vectors))
    }
}

/// A service over a [`FakeBackend`] with the standard rules and the stepped clock (it stands at `NOW` until a test advances it).
pub fn fake_service(consolidator: ScriptedConsolidator) -> MemoryService<FakeBackend> {
    MemoryService::new(FakeBackend::new(consolidator), RuleSet::standard())
}

/// `spaces.toml` entries for [`FAKE_SPACES`], created at the fixed instant, sealed.
pub fn fake_space_metas(created: UnixSeconds) -> Vec<SpaceMeta> {
    FAKE_SPACES
        .iter()
        .zip(1u8..)
        .filter_map(|(id, n)| {
            Some(SpaceMeta {
                id: SpaceId::parse(id).ok()?,
                created,
                replica: ReplicaId([n; 16]),
                vault: almanac_core::VaultKind::Sealed,
                format: 1,
            })
        })
        .collect()
}
