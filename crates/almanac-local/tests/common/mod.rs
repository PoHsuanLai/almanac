//! Shared setup: a root in a temp dir, a service over `LocalBackend`, the clients over it.
//! No daemon, no bus, no wall clock (the stepped clock stands at a fixed instant).
#![allow(dead_code)]

use almanac_client::{InProcess, Memory};
use almanac_core::*;
use almanac_fake::{ScriptedConsolidator, SteppedClock};
use almanac_local::{LocalBackend, Root, create_space, open};
use almanac_seal::{ProvidedKeys, SpaceKey};
use almanac_service::MemoryService;
use std::path::Path;
use std::sync::Arc;

pub type Backend = LocalBackend<SteppedClock, almanac_local::NoEmbedder, ScriptedConsolidator>;
pub type Service = MemoryService<Backend>;
pub type Client = Memory<InProcess<Backend>>;

pub fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

pub fn topic(name: &str) -> TopicPath {
    TopicPath::parse(name).expect("topic")
}

pub fn backend(root: &Path, master: u8, consolidator: ScriptedConsolidator) -> Backend {
    LocalBackend::new(
        Root::new(root),
        ProvidedKeys::new(SpaceKey::from_bytes([master; 32])),
        SteppedClock::default(),
    )
    .with_consolidator(consolidator)
}

/// The service over `root` as a fresh process would open it: `spaces.toml` read, nothing open.
pub fn service(root: &Path, master: u8, consolidator: ScriptedConsolidator) -> Arc<Service> {
    Arc::new(open(backend(root, master, consolidator), RuleSet::standard()).expect("open"))
}

/// A service over `root` with the Space `work` created (sealed) in it.
pub fn fresh(root: &Path, master: u8, consolidator: ScriptedConsolidator) -> Arc<Service> {
    let service = service(root, master, consolidator);
    create_space(&service, work(), VaultKind::Sealed).expect("create");
    service
}

pub fn client(service: &Arc<Service>, caller: Caller) -> Client {
    Memory::over(InProcess::new(service.clone(), caller))
}

pub fn shell(service: &Arc<Service>) -> Client {
    client(service, Caller::ShellUi)
}

pub fn query(text: &str) -> RecallQuery {
    RecallQuery {
        space: work(),
        text: text.into(),
        limit: Count(10),
        over: RecallOver::Both,
    }
}

pub fn draft(topic_name: &str, text: &str) -> FactDraft {
    FactDraft {
        topic: topic(topic_name),
        text: FactText::parse(text).expect("text"),
        links: vec![],
        supersedes: vec![],
    }
}

pub fn all_facts() -> FactQuery {
    FactQuery {
        space: work(),
        topic: None,
        about: None,
        state: FactFilter::All,
        limit: Count(50),
    }
}
