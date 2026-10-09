//! `SystemBackend`: the real seams. The key store is the Secret Service, the log is SQLCipher,
//! the vault is sealed or plain per Space, the models are inferd's.

use crate::clock::SystemClock;
use crate::infer::{InferdConsolidator, InferdEmbedder};
use almanac_core::{Dirs, ReplicaId, SpaceId, SpaceMeta, VaultKind};
use almanac_seal::{DbKey, KeyStore, Oo7Keys, Purpose, SpaceKey, derive};
use almanac_service::{Backend, BackendError, Consolidator};
use eventlog::SqliteLog;
use memfiles::{PlainDir, SealedDir, Vault, VaultError, VaultPath};
use porter_client::AnyTransport;
use recall::{Embedder, ExactScan, Fts5, Index, IndexError};
use rusqlite::Connection;
use std::path::Path;
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

/// memoryd's seams over the real system: SQLCipher files under `dirs`, sealed or plain vaults,
/// the OS's randomness and clock. The key store, the embedder and the consolidator are
/// parameters whose defaults are the real ones (the Secret Service, inferd); a test names its own
/// and keeps everything else real.
#[derive(Debug)]
pub struct SystemBackend<
    K = Oo7Keys,
    E = InferdEmbedder<AnyTransport>,
    C = InferdConsolidator<AnyTransport>,
> {
    dirs: Dirs,
    keys: K,
    embedder: E,
    consolidator: C,
    clock: SystemClock,
}

impl SystemBackend {
    /// Over `dirs` and one inferd connection shared by the embedder and the consolidator.
    pub fn new(dirs: Dirs, inferd: Arc<AnyTransport>, card: recall::EmbedderCard) -> Self {
        Self::with(
            dirs,
            Oo7Keys,
            InferdEmbedder::new(inferd.clone(), card),
            InferdConsolidator::new(inferd),
        )
    }
}

impl<K, E, C> SystemBackend<K, E, C> {
    /// Over `dirs` with the given seams.
    pub fn with(dirs: Dirs, keys: K, embedder: E, consolidator: C) -> Self {
        Self {
            dirs,
            keys,
            embedder,
            consolidator,
            clock: SystemClock,
        }
    }

    /// The directories.
    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }
}

/// A SQLCipher file opened with `key` (`PRAGMA key`, then `secure_delete` so removed rows do not
/// linger in free pages). A wrong key shows at the first query, which the caller makes.
fn open_keyed(path: &Path, key: &DbKey) -> Result<Connection, IndexError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", key.pragma())?;
    conn.pragma_update(None, "secure_delete", "ON")?;
    // No temporary files: the sandbox has no directory for them.
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(conn)
}

/// `index.db`: FTS5 and the vectors in one file, created with `recall::SCHEMA_V1` when it is
/// new. The two halves hold a connection each (the file is theirs alone and they never run at the
/// same time).
fn open_index_file<E: Embedder>(
    path: &Path,
    key: &DbKey,
    embedder: &E,
) -> Result<Index<ExactScan>, IndexError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| IndexError::Sqlite(e.to_string()))?;
    }
    let lexical = open_keyed(path, key)?;
    // SQLCipher reads nothing until the first query: a wrong key shows here.
    let version: u32 = lexical.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let fts = Fts5::new(lexical);
    if version == 0 {
        fts.create()?;
        fts.connection().pragma_update(None, "user_version", 1)?;
    }
    let vectors = ExactScan::new(open_keyed(path, key)?, embedder.card().clone());
    Ok(Index::new(fts, vectors))
}

/// Sixteen random bytes from the OS; if it cannot answer, a hash of the time and the process
/// (an id only needs to differ from the last run's).
fn random_bytes() -> [u8; 16] {
    let mut out = [0u8; 16];
    if getrandom::fill(&mut out).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let mut hasher = blake3::Hasher::new();
        hasher.update(&nanos.to_be_bytes());
        hasher.update(&std::process::id().to_be_bytes());
        out.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    }
    out
}

fn gone_is_fine(result: std::io::Result<()>) -> Result<(), BackendError> {
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(BackendError::Remove(e.to_string()))
        }
        _ => Ok(()),
    }
}

impl<K: KeyStore, E: Embedder, C: Consolidator> Backend for SystemBackend<K, E, C> {
    type Keys = K;
    type Log = SqliteLog;
    type Files = SpaceVault;
    type Index = Index<ExactScan>;
    type Embedder = E;
    type Consolidator = C;
    type Clock = SystemClock;

    fn keys(&self) -> &K {
        &self.keys
    }

    fn embedder(&self) -> &E {
        &self.embedder
    }

    fn consolidator(&self) -> &C {
        &self.consolidator
    }

    fn clock(&self) -> &SystemClock {
        &self.clock
    }

    fn random(&self) -> [u8; 16] {
        random_bytes()
    }

    fn remove_space(&self, space: &SpaceId) -> Result<(), BackendError> {
        for dir in [
            self.dirs.space(space),
            self.dirs.index_dir(space),
            self.dirs.edit(space),
        ] {
            gone_is_fine(std::fs::remove_dir_all(dir))?;
        }
        Ok(())
    }

    fn open_log(
        &self,
        space: &SpaceId,
        replica: ReplicaId,
        key: &SpaceKey,
    ) -> Result<SqliteLog, BackendError> {
        let db_key = DbKey::of(&derive(key, space, Purpose::Eventlog));
        let digest = derive(key, space, Purpose::Digest);
        Ok(SqliteLog::open_for(
            &self.dirs.events_db(space),
            &db_key,
            space,
            replica,
            &digest,
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
        let db_key = DbKey::of(&derive(key, space, Purpose::Index));
        open_index_file(&self.dirs.index_db(space), &db_key, &self.embedder)
            .map_err(|e| BackendError::Index(e.into()))
    }
}
