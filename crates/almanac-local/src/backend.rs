//! `LocalBackend`: the service's seams over files below one root.

use crate::index::open_index_file;
use crate::none::{NoConsolidator, NoEmbedder};
use crate::root::Root;
use crate::vault::LocalVault;
use almanac_core::{Dirs, ReplicaId, SpaceId, SpaceMeta, VaultKind};
use almanac_seal::{DbKey, ProvidedKeys, Purpose, SpaceKey, derive};
use almanac_service::{Backend, BackendError, Clock, Consolidator};
use eventlog::SqliteLog;
use memfiles::{PlainDir, SealedDir};
use recall::{Embedder, ExactScan, Index};
use std::sync::atomic::{AtomicU64, Ordering};

/// An app's own memory on disk: SQLCipher log and index, sealed or plain files, keys derived from
/// the master key the app provides. The embedder, consolidator and clock are the app's.
///
/// ```
/// use almanac_client::{InProcess, Memory, Recorded};
/// use almanac_core::{AppId, Caller, Isolation, RuleSet, SpaceId, VaultKind};
/// use almanac_fake::{mail, mail_thread_archived};
/// use almanac_local::{LocalBackend, Root, WallClock, create_space, open};
/// use almanac_seal::{ProvidedKeys, SpaceKey};
/// use std::sync::Arc;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
/// let dir = tempfile::tempdir()?;
/// let keys = ProvidedKeys::new(SpaceKey::from_bytes([7; 32]));
/// let backend = LocalBackend::new(Root::new(dir.path()), keys, WallClock);
/// let service = Arc::new(open(backend, RuleSet::standard())?);
/// create_space(&service, SpaceId::parse("work")?, VaultKind::Sealed)?;
///
/// let caller = Caller::App(AppId { name: mail(), isolation: Isolation::InProcess });
/// let app = Memory::over(InProcess::new(service, caller));
/// let record = mail_thread_archived().ok_or("fixture")?;
/// assert!(matches!(app.record(record).await?, Recorded::Stored(_)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # })
/// # }
/// ```
#[derive(Debug)]
pub struct LocalBackend<Clk, E = NoEmbedder, C = NoConsolidator> {
    root: Root,
    dirs: Dirs,
    keys: ProvidedKeys,
    embedder: E,
    consolidator: C,
    clock: Clk,
    draws: AtomicU64,
}

impl<Clk: Clock> LocalBackend<Clk> {
    /// Over `root` with `master` (as `ProvidedKeys`) and `clock`; recall is lexical-only and
    /// consolidation unavailable until the app adds a model.
    pub fn new(root: Root, keys: ProvidedKeys, clock: Clk) -> Self {
        Self {
            dirs: root.dirs(),
            root,
            keys,
            embedder: NoEmbedder::default(),
            consolidator: NoConsolidator,
            clock,
            draws: AtomicU64::new(0),
        }
    }
}

impl<Clk, E, C> LocalBackend<Clk, E, C> {
    /// The same backend with the app's embedder.
    pub fn with_embedder<E2: Embedder>(self, embedder: E2) -> LocalBackend<Clk, E2, C> {
        LocalBackend {
            root: self.root,
            dirs: self.dirs,
            keys: self.keys,
            embedder,
            consolidator: self.consolidator,
            clock: self.clock,
            draws: self.draws,
        }
    }

    /// The same backend with the app's consolidation model.
    pub fn with_consolidator<C2: Consolidator>(self, consolidator: C2) -> LocalBackend<Clk, E, C2> {
        LocalBackend {
            root: self.root,
            dirs: self.dirs,
            keys: self.keys,
            embedder: self.embedder,
            consolidator,
            clock: self.clock,
            draws: self.draws,
        }
    }

    /// The root the app gave.
    pub fn root(&self) -> &Root {
        &self.root
    }

    /// The layout below the root.
    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }
}

fn gone_is_fine(result: std::io::Result<()>) -> Result<(), BackendError> {
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(BackendError::Remove(e.to_string()))
        }
        _ => Ok(()),
    }
}

impl<Clk: Clock, E: Embedder, C: Consolidator> LocalBackend<Clk, E, C> {
    /// Sixteen bytes that differ from the last run's: the OS's randomness, or, if it cannot
    /// answer, the clock's seconds and a counter (an id only needs to differ).
    fn draw(&self) -> [u8; 16] {
        let n = self.draws.fetch_add(1, Ordering::Relaxed);
        let mut out = [0u8; 16];
        if getrandom::fill(&mut out).is_err() {
            out[..8].copy_from_slice(&self.clock.now().0.to_be_bytes());
            out[8..].copy_from_slice(&n.to_be_bytes());
        }
        out
    }
}

impl<Clk: Clock, E: Embedder, C: Consolidator> Backend for LocalBackend<Clk, E, C> {
    type Keys = ProvidedKeys;
    type Log = SqliteLog;
    type Files = LocalVault;
    type Vectors = ExactScan;
    type Embedder = E;
    type Consolidator = C;
    type Clock = Clk;

    fn keys(&self) -> &ProvidedKeys {
        &self.keys
    }

    fn embedder(&self) -> &E {
        &self.embedder
    }

    fn consolidator(&self) -> &C {
        &self.consolidator
    }

    fn clock(&self) -> &Clk {
        &self.clock
    }

    fn random(&self) -> [u8; 16] {
        self.draw()
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

    fn open_files(&self, meta: &SpaceMeta, key: &SpaceKey) -> Result<LocalVault, BackendError> {
        let plain = PlainDir::new(self.dirs.space(&meta.id));
        Ok(match meta.vault {
            VaultKind::Plain => LocalVault::Plain(plain),
            VaultKind::Sealed => LocalVault::Sealed(SealedDir::new(
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
        Ok(open_index_file(
            &self.dirs.index_db(space),
            &db_key,
            &self.embedder,
        )?)
    }
}
