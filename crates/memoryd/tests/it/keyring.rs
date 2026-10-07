//! The keyring's lock signal drives the key check, and an index that changes state is announced.
//! A fake Secret Service on a private bus emits the signal; nothing here waits on a timer.

use crate::support::bus::PrivateBus;
use crate::support::{SharedKeys, World, connect, dirs_in, space};
use almanac_client::{DbusTransport, Memory};
use almanac_core::*;
use almanac_dbus::ControlProxy;
use almanac_fake::{ScriptedConsolidator, mail_thread_archived};
use almanac_service::{Backend, MemoryService, ServiceEvent};
use memoryd::{LockChanges, SystemBackend, is_lock_change};
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, FakeEmbedder, Urgency, Vector};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use zbus::export::futures_core::Stream;
use zbus::zvariant::Value;

const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const ITEM: &str = "org.freedesktop.Secret.Item";
const PATH: &str = "/org/freedesktop/secrets/collection/login";

async fn next_item<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// What a Secret Service says when `property` of an `interface` object changed.
async fn announce(service: &zbus::Connection, interface: &str, property: &str) {
    let changed: HashMap<&str, Value<'_>> = HashMap::from([(property, Value::from(true))]);
    service
        .emit_signal(
            None::<&str>,
            PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(interface, changed, Vec::<&str>::new()),
        )
        .await
        .expect("emit the signal");
}

fn signal_of(interface: &str, property: &str, invalidated: &[&str]) -> zbus::Message {
    let changed: HashMap<&str, Value<'_>> = HashMap::from([(property, Value::from(true))]);
    zbus::Message::signal(PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged")
        .expect("builder")
        .build(&(interface, changed, invalidated.to_vec()))
        .expect("message")
}

#[test]
fn only_a_collections_locked_property_is_a_lock_change() {
    assert!(is_lock_change(&signal_of(COLLECTION, "Locked", &[])));
    assert!(!is_lock_change(&signal_of(COLLECTION, "Label", &[])));
    assert!(!is_lock_change(&signal_of(ITEM, "Locked", &[])));
    // A property named only as invalidated changed too.
    let plain = zbus::Message::signal(PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged")
        .expect("builder")
        .build(&(
            COLLECTION,
            HashMap::<&str, Value<'_>>::new(),
            vec!["Locked"],
        ))
        .expect("message");
    assert!(is_lock_change(&plain));
    // Not a PropertiesChanged body at all.
    let other = zbus::Message::signal(PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged")
        .expect("builder")
        .build(&"nothing")
        .expect("message");
    assert!(!is_lock_change(&other));
}

#[tokio::test]
async fn the_keyrings_lock_signal_closes_and_reopens_the_space() {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let dirs = dirs_in(&scratch.path().join("home"));
    let world = World::start(&bus, &dirs, &SharedKeys::default()).await;
    let fake_service = connect(&bus.address).await;

    let watcher = world.client(Caller::ShellUi).await;
    let control = ControlProxy::new(&watcher).await.expect("proxy");
    let mut status = control.receive_status_changed().await.expect("subscribe");

    // The daemon listens (the match rule is in place once this returns).
    let changes = LockChanges::on(&world.server).await.expect("subscribe");
    let daemon = world.daemon.clone();
    let task = tokio::spawn(async move { daemon.follow_keyring(changes).await });

    // A Space is open with a fact in it.
    let app = Caller::App(AppId {
        name: almanac_fake::mail(),
        isolation: Isolation::Unsandboxed,
    });
    let memory = Memory::over(DbusTransport::new(world.client(app).await));
    memory
        .record(mail_thread_archived().expect("fixture"))
        .await
        .expect("record");

    let wait = Duration::from_secs(10);
    // Something else about the collection changing is not a lock; then the keyring locks.
    announce(&fake_service, COLLECTION, "Label").await;
    world.keys.0.lock();
    announce(&fake_service, COLLECTION, "Locked").await;
    let signal = tokio::time::timeout(wait, next_item(&mut status))
        .await
        .expect("StatusChanged arrives for the lock signal")
        .expect("a signal");
    let args = signal.args().expect("args");
    assert_eq!(args.space, space("work").to_string());
    let locked: SpaceStatus = serde_json::from_str(args.status).expect("a SpaceStatus");
    assert_eq!(locked.state, SpaceState::Locked);

    // It unlocks: the signal opens the Space again.
    world.keys.0.unlock();
    announce(&fake_service, COLLECTION, "Locked").await;
    let signal = tokio::time::timeout(wait, next_item(&mut status))
        .await
        .expect("StatusChanged arrives for the unlock signal")
        .expect("a signal");
    let open: SpaceStatus =
        serde_json::from_str(signal.args().expect("args").status).expect("a SpaceStatus");
    assert_eq!(open.state, SpaceState::Open);
    task.abort();
}

/// An embedder a test can take away and give back.
#[derive(Debug, Clone)]
struct Switch {
    inner: FakeEmbedder,
    up: Arc<AtomicBool>,
}

impl Embedder for Switch {
    fn card(&self) -> &EmbedderCard {
        self.inner.card()
    }

    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        if self.up.load(Ordering::SeqCst) {
            self.inner.embed(texts, role, urgency).await
        } else {
            Err(EmbedError::Unavailable)
        }
    }
}

async fn index_of<B: Backend>(service: &MemoryService<B>) -> IndexView {
    match service
        .handle(&Caller::ShellUi, MemoryRequest::Status(space("work")))
        .await
    {
        MemoryReply::Status(status) => status.index,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn an_index_that_changes_state_is_announced_once() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let up = Arc::new(AtomicBool::new(true));
    let embedder = Switch {
        inner: FakeEmbedder::new(),
        up: up.clone(),
    };
    let backend = SystemBackend::with(
        dirs,
        SharedKeys::default(),
        embedder,
        ScriptedConsolidator::default(),
    );
    let service = MemoryService::new(backend, RuleSet::standard());
    let app = Caller::App(AppId {
        name: almanac_fake::mail(),
        isolation: Isolation::Unsandboxed,
    });
    let record = || MemoryRequest::Record(mail_thread_archived().expect("fixture"));
    assert!(matches!(
        service.handle(&app, record()).await,
        MemoryReply::Recorded(_)
    ));
    assert_eq!(index_of(&service).await, IndexView::Ready);
    assert!(service.take_events().is_empty(), "opening is not news");

    // The embedder goes away: the next write leaves the index lexical only, and the bus hears.
    up.store(false, Ordering::SeqCst);
    service.handle(&app, record()).await;
    assert_eq!(
        service.take_events(),
        vec![ServiceEvent::StatusChanged(space("work"))]
    );
    service.handle(&app, record()).await;
    assert!(
        service.take_events().is_empty(),
        "the same state is not said twice"
    );

    // It comes back and the person rebuilds: the state moves again, and the bus hears.
    up.store(true, Ordering::SeqCst);
    let rebuilt = service
        .handle(&Caller::ShellUi, MemoryRequest::Rebuild(space("work")))
        .await;
    assert_eq!(rebuilt, MemoryReply::Ok);
    assert_eq!(
        service.take_events(),
        vec![ServiceEvent::StatusChanged(space("work"))]
    );
    assert_eq!(index_of(&service).await, IndexView::Ready);
}
