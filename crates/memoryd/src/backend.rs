//! `SystemBackend`: the real seams. The key store is the Secret Service, the log is SQLCipher,
//! the vault is sealed or plain per Space, the models are inferd's.

use crate::clock::SystemClock;
use crate::infer::{InferdConsolidator, InferdEmbedder};
use almanac_core::{Dirs, ReplicaId, SpaceId, SpaceMeta, VaultKind};
use almanac_seal::{DbKey, Oo7Keys, Purpose, SpaceKey, derive};
use almanac_service::{Backend, BackendError};
use eventlog::SqliteLog;
use memfiles::{PlainDir, SealedDir, Vault, VaultError, VaultPath};
use porter_client::AnyTransport;
use recall::{ExactScan, Index};
use std::sync::Arc;

/// A Space's vault: sealed or plain, by the Space's choice. A closed set, so an enum.
#[derive(Debug)]
pub enum SpaceVault {
    /// Sealed per file (the default).
    Sealed(SealedDir),
    /// Plain markdown.
    Plain(PlainDir),
}

impl Vault for SpaceVault {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.list(dir),
            SpaceVault::Plain(v) => v.list(dir),
        }
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.read(p),
            SpaceVault::Plain(v) => v.read(p),
        }
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.write_atomic(p, bytes),
            SpaceVault::Plain(v) => v.write_atomic(p, bytes),
        }
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.remove(p),
            SpaceVault::Plain(v) => v.remove(p),
        }
    }
}

/// memoryd's seams over the real system.
#[derive(Debug)]
pub struct SystemBackend {
    dirs: Dirs,
    keys: Oo7Keys,
    embedder: InferdEmbedder<AnyTransport>,
    consolidator: InferdConsolidator<AnyTransport>,
    clock: SystemClock,
}

impl SystemBackend {
    /// Over `dirs` and one inferd connection shared by the embedder and the consolidator.
    pub fn new(dirs: Dirs, inferd: Arc<AnyTransport>, card: recall::EmbedderCard) -> Self {
        Self {
            dirs,
            keys: Oo7Keys,
            embedder: InferdEmbedder::new(inferd.clone(), card),
            consolidator: InferdConsolidator::new(inferd),
            clock: SystemClock,
        }
    }
}

impl Backend for SystemBackend {
    type Keys = Oo7Keys;
    type Log = SqliteLog;
    type Files = SpaceVault;
    type Vectors = ExactScan;
    type Embedder = InferdEmbedder<AnyTransport>;
    type Consolidator = InferdConsolidator<AnyTransport>;
    type Clock = SystemClock;

    fn keys(&self) -> &Oo7Keys {
        &self.keys
    }

    fn embedder(&self) -> &Self::Embedder {
        &self.embedder
    }

    fn consolidator(&self) -> &Self::Consolidator {
        &self.consolidator
    }

    fn clock(&self) -> &SystemClock {
        &self.clock
    }

    fn open_log(
        &self,
        space: &SpaceId,
        replica: ReplicaId,
        key: &SpaceKey,
    ) -> Result<SqliteLog, BackendError> {
        let db_key = DbKey::of(&derive(key, space, Purpose::Eventlog));
        Ok(SqliteLog::open_for(
            &self.dirs.events_db(space),
            &db_key,
            space,
            replica,
        )?)
    }

    fn open_files(&self, meta: &SpaceMeta, key: &SpaceKey) -> Result<SpaceVault, BackendError> {
        let plain = PlainDir::new(self.dirs.space(&meta.id));
        Ok(match meta.vault {
            VaultKind::Plain => SpaceVault::Plain(plain),
            VaultKind::Sealed => SpaceVault::Sealed(SealedDir::new(
                plain,
                meta.id.clone(),
                derive(key, &meta.id, Purpose::Files),
            )),
        })
    }

    fn open_index(
        &self,
        space: &SpaceId,
        key: &SpaceKey,
    ) -> Result<Index<ExactScan>, BackendError> {
        let _ = (
            self.dirs.index_db(space),
            DbKey::of(&derive(key, space, Purpose::Index)),
        );
        todo!(
            "open index.db with PRAGMA key, create SCHEMA_V1 on a new file, wrap Fts5 and ExactScan over it"
        )
    }
}
