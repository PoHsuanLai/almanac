//! A Space whose stores will not open answers by what is wrong: a key that does not open the
//! log (or a locked key store) is `SpaceLocked`, never `Invalid`.

use almanac_core::{
    Caller, MemoryReply, MemoryRequest, Refusal, ReplicaId, RuleSet, SpaceId, SpaceMeta,
};
use almanac_fake::{FakeBackend, ScriptedConsolidator, SharedVault, SteppedClock};
use almanac_seal::{MemoryKeys, SpaceKey};
use almanac_service::{Backend, BackendError, MemoryService, ServiceEvent};
use eventlog::{LogError, MemoryLog};
use recall::{ExactScan, FakeEmbedder, Index};

/// The fake backend, except the log refuses to open with `error`.
struct Refusing {
    inner: FakeBackend,
    error: LogError,
}

impl Backend for Refusing {
    type Keys = MemoryKeys;
    type Log = MemoryLog;
    type Files = SharedVault;
    type Vectors = ExactScan;
    type Embedder = FakeEmbedder;
    type Consolidator = ScriptedConsolidator;
    type Clock = SteppedClock;

    fn keys(&self) -> &MemoryKeys {
        self.inner.keys()
    }
    fn embedder(&self) -> &FakeEmbedder {
        self.inner.embedder()
    }
    fn consolidator(&self) -> &ScriptedConsolidator {
        self.inner.consolidator()
    }
    fn clock(&self) -> &SteppedClock {
        self.inner.clock()
    }
    fn random(&self) -> [u8; 16] {
        self.inner.random()
    }
    fn remove_space(&self, space: &SpaceId) -> Result<(), BackendError> {
        self.inner.remove_space(space)
    }
    fn open_log(&self, _: &SpaceId, _: ReplicaId, _: &SpaceKey) -> Result<MemoryLog, BackendError> {
        Err(self.error.clone().into())
    }
    fn open_files(&self, meta: &SpaceMeta, key: &SpaceKey) -> Result<SharedVault, BackendError> {
        self.inner.open_files(meta, key)
    }
    fn open_index(&self, id: &SpaceId, key: &SpaceKey) -> Result<Index<ExactScan>, BackendError> {
        self.inner.open_index(id, key)
    }
}

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

async fn status(service: &MemoryService<Refusing>) -> MemoryReply {
    service
        .handle(&Caller::ShellUi, MemoryRequest::Status(work()))
        .await
}

fn refusing(error: LogError) -> MemoryService<Refusing> {
    let inner = FakeBackend::new(ScriptedConsolidator::default());
    MemoryService::new(Refusing { inner, error }, RuleSet::standard())
}

#[tokio::test]
async fn a_log_the_key_does_not_open_is_a_locked_space() {
    let service = refusing(LogError::Locked);
    assert_eq!(
        status(&service).await,
        MemoryReply::Refused(Refusal::SpaceLocked)
    );
    assert_eq!(service.take_events(), vec![ServiceEvent::Locked(work())]);
}

#[tokio::test]
async fn a_full_disk_is_still_a_failure_not_a_lock() {
    let service = refusing(LogError::Full);
    assert!(matches!(
        status(&service).await,
        MemoryReply::Refused(Refusal::Invalid(_))
    ));
    assert!(service.take_events().is_empty());
}

#[tokio::test]
async fn a_locked_key_store_is_a_locked_space() {
    let service = refusing(LogError::Full);
    service.backend().keys().lock();
    assert_eq!(
        status(&service).await,
        MemoryReply::Refused(Refusal::SpaceLocked)
    );
}
