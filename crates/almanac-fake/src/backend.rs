//! `FakeBackend`: every seam in memory, and the service built over it.

use crate::clock::FixedClock;
use crate::consolidator::ScriptedConsolidator;
use almanac_core::{ReplicaId, RuleSet, SpaceId, SpaceMeta, UnixSeconds};
use almanac_seal::{MemoryKeys, Purpose, SpaceKey, derive};
use almanac_service::{Backend, BackendError, MemoryService};
use eventlog::MemoryLog;
use memfiles::MemoryVault;
use recall::{ExactScan, FakeEmbedder, Fts5, Index};

/// The three Spaces every fake service knows.
pub const FAKE_SPACES: [&str; 3] = ["work", "home", "desktop"];

/// In-memory keys, log, vault, index (an in-memory SQLite), embedder, a scripted
/// consolidator and a fixed clock.
#[derive(Debug)]
pub struct FakeBackend {
    keys: MemoryKeys,
    embedder: FakeEmbedder,
    consolidator: ScriptedConsolidator,
    clock: FixedClock,
}

impl FakeBackend {
    /// A backend whose consolidator answers from `consolidator`.
    pub fn new(consolidator: ScriptedConsolidator) -> Self {
        Self {
            keys: MemoryKeys::new(),
            embedder: FakeEmbedder::new(),
            consolidator,
            clock: FixedClock::default(),
        }
    }

    /// The key store, for tests that lock it.
    pub fn memory_keys(&self) -> &MemoryKeys {
        &self.keys
    }
}

impl Backend for FakeBackend {
    type Keys = MemoryKeys;
    type Log = MemoryLog;
    type Files = MemoryVault;
    type Vectors = ExactScan;
    type Embedder = FakeEmbedder;
    type Consolidator = ScriptedConsolidator;
    type Clock = FixedClock;

    fn keys(&self) -> &MemoryKeys {
        &self.keys
    }

    fn embedder(&self) -> &FakeEmbedder {
        &self.embedder
    }

    fn consolidator(&self) -> &ScriptedConsolidator {
        &self.consolidator
    }

    fn clock(&self) -> &FixedClock {
        &self.clock
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

    fn open_files(&self, _meta: &SpaceMeta, _key: &SpaceKey) -> Result<MemoryVault, BackendError> {
        Ok(MemoryVault::new())
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

/// A service over a [`FakeBackend`] with the standard rules and the fixed clock.
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
