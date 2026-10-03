//! What the daemon's tests share: a private session bus, scratch directories, seams that are
//! real where the test is about the real thing (SQLCipher, sealed files) and in memory where it
//! is about the system around it (keys, the embedder, the callers).

#![allow(dead_code)]

pub mod bus;
pub mod inferd;

use almanac_core::{Caller, Dirs, SpaceId};
use almanac_dbus::serve_on;
use almanac_fake::ScriptedConsolidator;
use almanac_seal::{KeyError, KeyStore, MemoryKeys, SpaceKey};
use almanac_service::MemoryService;
use memoryd::{Daemon, SystemBackend, TablePeers};
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, FakeEmbedder, Urgency, Vector};
use std::path::Path;
use std::sync::Arc;

pub fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space")
}

/// Scratch XDG roots under `root`: nothing here is the real home.
pub fn dirs_in(root: &Path) -> Dirs {
    Dirs::new(
        root.join("data"),
        root.join("cache"),
        root.join("config"),
        root.join("run"),
    )
}

/// A key store two daemons can share, as the Secret Service is shared by successive runs.
#[derive(Debug, Clone, Default)]
pub struct SharedKeys(pub Arc<MemoryKeys>);

impl KeyStore for SharedKeys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.0.get(space).await
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.0.create(space).await
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        self.0.destroy(space).await
    }
}

/// The fake embedder that gives the executor back before it answers, as a call to inferd does:
/// without it a second request could never overlap the first.
#[derive(Debug, Clone)]
pub struct SlowEmbedder(pub FakeEmbedder);

impl Embedder for SlowEmbedder {
    fn card(&self) -> &EmbedderCard {
        self.0.card()
    }

    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        self.0.embed(texts, role, urgency).await
    }
}

pub type TestBackend = SystemBackend<SharedKeys, SlowEmbedder, ScriptedConsolidator>;

pub fn backend(dirs: &Dirs, keys: &SharedKeys, consolidator: ScriptedConsolidator) -> TestBackend {
    SystemBackend::with(
        dirs.clone(),
        keys.clone(),
        SlowEmbedder(FakeEmbedder::new()),
        consolidator,
    )
}

pub fn service(dirs: &Dirs, keys: &SharedKeys) -> MemoryService<TestBackend> {
    MemoryService::new(
        backend(dirs, keys, ScriptedConsolidator::default()),
        almanac_core::RuleSet::standard(),
    )
}

/// A daemon on `bus` over scratch directories, with the clients that call it.
pub struct World {
    pub daemon: Arc<Daemon<TestBackend, TablePeers>>,
    pub server: zbus::Connection,
    pub address: String,
    pub dirs: Dirs,
    pub keys: SharedKeys,
}

impl World {
    /// The daemon, serving on `bus`'s address.
    pub async fn start(bus: &bus::PrivateBus, dirs: &Dirs, keys: &SharedKeys) -> World {
        let server = connect(&bus.address).await;
        let daemon = Arc::new(Daemon::new(
            service(dirs, keys),
            TablePeers::new(),
            dirs.clone(),
        ));
        serve_on(&server, daemon.clone()).await.expect("serve");
        daemon.attach(server.clone());
        World {
            daemon,
            server,
            address: bus.address.clone(),
            dirs: dirs.clone(),
            keys: keys.clone(),
        }
    }

    /// A new connection to the bus that the daemon takes for `caller`.
    pub async fn client(&self, caller: Caller) -> zbus::Connection {
        let connection = connect(&self.address).await;
        let unique = connection.unique_name().expect("unique name").to_string();
        self.daemon.peers().introduce(&unique, caller);
        connection
    }
}

pub async fn connect(address: &str) -> zbus::Connection {
    zbus::connection::Builder::address(address)
        .expect("address")
        .build()
        .await
        .expect("connect to the private bus")
}
