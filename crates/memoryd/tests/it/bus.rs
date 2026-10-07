//! The daemon on a private bus, called through almanac-client's D-Bus transport: the whole path
//! from an app's `record` to the SQLCipher file and back, and the refusals, the signals and the
//! export that cross the bus. Every bus, directory and key here is the test's own.

use crate::support::bus::PrivateBus;
use crate::support::{SharedKeys, World, connect, dirs_in, space};
use almanac_client::{ClientError, DbusTransport, Memory, Recorded, Transport, TransportError};
use almanac_core::*;
use almanac_dbus::{ControlProxy, MEMORY_BUS};
use almanac_fake::{mail, mail_thread_archived, session_entry, session_thing, thing};
use std::pin::Pin;
use std::time::Duration;
use zbus::export::futures_core::Stream;

async fn next_item<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

fn app() -> Caller {
    Caller::App(AppId {
        name: mail(),
        isolation: Isolation::Unsandboxed,
    })
}

struct Rig {
    _scratch: tempfile::TempDir,
    _bus: PrivateBus,
    world: World,
}

async fn rig() -> Rig {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let dirs = dirs_in(&scratch.path().join("home"));
    let world = World::start(&bus, &dirs, &SharedKeys::default()).await;
    Rig {
        _scratch: scratch,
        _bus: bus,
        world,
    }
}

impl Rig {
    async fn memory(&self, caller: Caller) -> Memory<DbusTransport> {
        Memory::over(DbusTransport::new(self.world.client(caller).await))
    }
}

fn budget_search() -> RecallQuery {
    RecallQuery {
        space: space("work"),
        text: "budget".into(),
        limit: Count(10),
        over: RecallOver::Both,
    }
}

#[tokio::test]
async fn record_search_inject_forget_over_the_bus() {
    let rig = rig().await;
    let (app, router, shell) = (
        rig.memory(app()).await,
        rig.memory(Caller::Router).await,
        rig.memory(Caller::ShellUi).await,
    );

    // An app records what happened in it.
    let recorded = app
        .record(mail_thread_archived().expect("fixture"))
        .await
        .expect("record");
    let Recorded::Stored(event) = recorded else {
        panic!("{recorded:?}")
    };
    assert_eq!(event.space, space("work"));

    // The router searches and injects.
    let hits = router.search(budget_search()).await.expect("search");
    assert!(
        hits.iter()
            .any(|h| h.doc == MemoryItem::Event(event.clone())),
        "{hits:?}"
    );
    let injected = router
        .inject(InjectQuery {
            space: space("work"),
            text: "budget".into(),
            budget: Tokens(1000),
            k: Count(5),
            over: RecallOver::Both,
            trust: TrustFilter::Any,
        })
        .await
        .expect("inject");
    assert!(!injected.is_empty());

    // The person forgets the thing: plan, then apply exactly the plan.
    let plan = shell
        .plan_forget(
            space("work"),
            ForgetScope::Thing(thing("mail.thread", "7f3a").expect("thing")),
        )
        .await
        .expect("plan");
    assert_eq!(plan.events, Count(1));
    let report = shell.forget(plan.token).await.expect("forget");
    assert_eq!(report.counts.events, Count(1));
    let after = router.search(budget_search()).await.expect("search");
    assert!(
        after
            .iter()
            .all(|h| h.doc != MemoryItem::Event(event.clone())),
        "nothing of it is left to find: {after:?}"
    );
    let page = shell
        .timeline(
            space("work"),
            TimelineQuery {
                before: None,
                limit: Count(20),
                filter: TimelineFilter {
                    actors: ActorFilter::Everyone,
                    apps: vec![],
                    kinds: vec![],
                    trust: TrustFilter::Any,
                    range: None,
                },
            },
        )
        .await
        .expect("timeline");
    assert!(
        page.entries
            .iter()
            .any(|e| e.kind.as_str() == "memory.forgot"),
        "the forget is in the audit"
    );
}

#[tokio::test]
async fn a_refusal_crosses_the_bus_as_the_error_of_its_name() {
    let rig = rig().await;
    let router = rig.memory(Caller::Router).await;
    let shell = rig.memory(Caller::ShellUi).await;
    let stranger = Memory::over(DbusTransport::new(connect(&rig.world.address).await));

    let token = PlanToken::parse("p-1").expect("token");
    assert_eq!(
        router
            .forget(token)
            .await
            .expect_err("the router may not forget"),
        ClientError::Refused(Refusal::NotAllowed)
    );
    assert_eq!(
        stranger
            .search(budget_search())
            .await
            .expect_err("nobody introduced this connection"),
        ClientError::Refused(Refusal::NotAllowed)
    );
    assert_eq!(
        shell
            .settle(FactId::mint(1, [9; 10]), Settlement::Discard)
            .await
            .expect_err("no such fact"),
        ClientError::Refused(Refusal::NoSuchFact)
    );
    let invalid = shell
        .forget(PlanToken::parse("p-nope").expect("token"))
        .await
        .expect_err("no such plan");
    assert!(matches!(invalid, ClientError::Refused(Refusal::Invalid(_))));
}

#[tokio::test]
async fn a_record_admission_dropped_is_no_memory_not_an_error() {
    let rig = rig().await;
    let app = rig.memory(app()).await;
    let shell = rig.memory(Caller::ShellUi).await;
    shell
        .ask(MemoryRequest::Mark(MarkRequest {
            space: space("work"),
            thing: thing("mail.thread", "7f3a").expect("thing"),
            mark: MarkKind::DoNotRemember,
        }))
        .await
        .expect("mark");
    assert_eq!(
        app.record(mail_thread_archived().expect("fixture")).await,
        Ok(Recorded::NoMemory),
        "the empty first output is `nothing was kept`"
    );
    assert_eq!(
        app.record_batch(vec![mail_thread_archived().expect("fixture")])
            .await,
        Ok(Recorded::NoMemory)
    );
}

#[tokio::test]
async fn a_batch_the_person_spaces_and_rules_travel_too() {
    let rig = rig().await;
    let app = rig.memory(app()).await;
    let shell = rig.memory(Caller::ShellUi).await;
    let one = mail_thread_archived().expect("fixture");
    let mut two = one.clone();
    if let EventBody::Thing { verb, .. } = &mut two.body {
        *verb = Verb::Viewed;
    }
    assert!(matches!(
        app.record_batch(vec![one, two]).await,
        Ok(Recorded::Stored(_))
    ));
    let spaces = shell.spaces().await.expect("spaces");
    assert!(spaces.iter().any(|s| s.id == space("work")));
    let MemoryReply::Rules(rules) = shell.ask(MemoryRequest::Rules).await.expect("rules") else {
        panic!("rules")
    };
    assert_eq!(rules, RuleSet::standard());
    let status = shell.status(space("work")).await.expect("status");
    assert_eq!(status.events, Count(2));
}

#[tokio::test]
async fn without_a_daemon_the_transport_says_there_is_no_memory() {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let lonely = Memory::over(DbusTransport::new(connect(&bus.address).await));
    assert_eq!(
        lonely.search(budget_search()).await,
        Err(ClientError::Transport(TransportError::Absent))
    );
    assert_eq!(
        lonely
            .record(mail_thread_archived().expect("fixture"))
            .await,
        Ok(Recorded::NoMemory),
        "writers carry on"
    );
}

#[tokio::test]
async fn the_signals_tell_the_bus_what_changed() {
    let rig = rig().await;
    let watcher = rig.world.client(Caller::ShellUi).await;
    let control = ControlProxy::new(&watcher).await.expect("proxy");
    assert_eq!(
        control.version().await.expect("version"),
        MEMORY_WIRE_VERSION
    );
    let mut recorded = control.receive_recorded().await.expect("subscribe");
    let mut forgotten = control.receive_forgotten().await.expect("subscribe");
    let mut pending = control.receive_pending_changed().await.expect("subscribe");
    let mut status = control.receive_status_changed().await.expect("subscribe");

    let app = rig.memory(app()).await;
    let router = rig.memory(Caller::Router).await;
    let shell = rig.memory(Caller::ShellUi).await;
    let wait = Duration::from_secs(10);

    app.record(mail_thread_archived().expect("fixture"))
        .await
        .expect("record");
    let signal = tokio::time::timeout(wait, next_item(&mut recorded))
        .await
        .expect("Recorded arrives")
        .expect("a signal");
    let args = signal.args().expect("args");
    assert_eq!(args.space, "work");
    assert_eq!(args.kind, "thing.archived");
    let event: EventRef = serde_json::from_str(args.event_ref).expect("an EventRef");
    assert_eq!(event.seq, Seq(1));

    // The router's proposal waits for the person: the pending count changes.
    router
        .propose(
            space("work"),
            FactDraft {
                topic: TopicPath::parse("people/ana").expect("topic"),
                text: FactText::parse("Ana is the CFO.").expect("text"),
                links: vec![],
                supersedes: vec![],
            },
        )
        .await
        .expect("propose");
    let signal = tokio::time::timeout(wait, next_item(&mut pending))
        .await
        .expect("PendingChanged arrives")
        .expect("a signal");
    assert_eq!(signal.args().expect("args").count, 1);

    shell
        .ask(MemoryRequest::Pause(
            space("work"),
            UnixSeconds(4_000_000_000),
        ))
        .await
        .expect("pause");
    let signal = tokio::time::timeout(wait, next_item(&mut status))
        .await
        .expect("StatusChanged arrives")
        .expect("a signal");
    let paused: SpaceStatus =
        serde_json::from_str(signal.args().expect("args").status).expect("a SpaceStatus");
    assert!(matches!(paused.state, SpaceState::Paused { .. }));

    let plan = shell
        .plan_forget(
            space("work"),
            ForgetScope::Thing(thing("mail.thread", "7f3a").expect("thing")),
        )
        .await
        .expect("plan");
    shell.forget(plan.token).await.expect("forget");
    let signal = tokio::time::timeout(wait, next_item(&mut forgotten))
        .await
        .expect("Forgotten arrives")
        .expect("a signal");
    assert_eq!(signal.args().expect("args").space, "work");
}

#[tokio::test]
async fn an_export_is_written_to_the_stream_the_caller_passes() {
    let rig = rig().await;
    let app = rig.memory(app()).await;
    app.record(mail_thread_archived().expect("fixture"))
        .await
        .expect("record");
    let shell = DbusTransport::new(rig.world.client(Caller::ShellUi).await);
    let target = tempfile::tempfile().expect("target");
    let reader = target.try_clone().expect("clone");
    let reply = shell
        .export(
            ExportOptions {
                spaces: vec![],
                verification_key: VerificationKey::Omit,
            },
            target.into(),
        )
        .await
        .expect("export");
    let MemoryReply::Exported(manifest) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(manifest.counts.events, Count(1));
    let mut reader = reader;
    std::io::Seek::rewind(&mut reader).expect("rewind");
    let names: Vec<String> = tar::Archive::new(reader)
        .entries()
        .expect("a tar stream")
        .filter_map(|e| Some(e.ok()?.path().ok()?.to_string_lossy().into_owned()))
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("work/events.jsonl")),
        "{names:?}"
    );

    // Over `call` an export has no stream to write to.
    let refused = shell
        .call(MemoryRequest::Export(ExportOptions {
            spaces: vec![],
            verification_key: VerificationKey::Omit,
        }))
        .await;
    assert!(matches!(refused, Err(TransportError::Bus(_))));
}

#[tokio::test]
async fn the_daemon_owns_its_well_known_name() {
    let rig = rig().await;
    let bus = zbus::fdo::DBusProxy::new(&rig.world.server)
        .await
        .expect("bus");
    let name = zbus::names::BusName::try_from(MEMORY_BUS).expect("name");
    assert!(bus.name_has_owner(name).await.expect("owner"));
}

fn receipt() -> ConfirmReceipt {
    ConfirmReceipt {
        id: ConfirmId::parse("c-1").expect("id"),
        input: InputProof::ShellCaller,
        at: UnixSeconds(1_790_000_000),
        covers: Confidentiality::Secret,
    }
}

#[tokio::test]
async fn settling_a_pending_fact_and_losing_a_key_are_signalled_too() {
    let rig = rig().await;
    let watcher = rig.world.client(Caller::ShellUi).await;
    let control = ControlProxy::new(&watcher).await.expect("proxy");
    let mut pending = control.receive_pending_changed().await.expect("subscribe");
    let mut status = control.receive_status_changed().await.expect("subscribe");
    let router = rig.memory(Caller::Router).await;
    let shell = rig.memory(Caller::ShellUi).await;
    let wait = Duration::from_secs(10);

    let (id, _) = router
        .propose(
            space("work"),
            FactDraft {
                topic: TopicPath::parse("people/ana").expect("topic"),
                text: FactText::parse("Ana is the CFO.").expect("text"),
                links: vec![],
                supersedes: vec![],
            },
        )
        .await
        .expect("propose");
    let signal = tokio::time::timeout(wait, next_item(&mut pending))
        .await
        .expect("PendingChanged arrives for the proposal")
        .expect("a signal");
    assert_eq!(signal.args().expect("args").count, 1);

    shell
        .settle(id, Settlement::Keep(receipt()))
        .await
        .expect("settle");
    let signal = tokio::time::timeout(wait, next_item(&mut pending))
        .await
        .expect("PendingChanged arrives for the settlement")
        .expect("a signal");
    let args = signal.args().expect("args");
    assert_eq!((args.space, args.count), ("work", 0));

    // The keyring locks: the daemon's key check closes the Space and says so, once.
    rig.world.keys.0.lock();
    rig.world.daemon.check_keys().await;
    let signal = tokio::time::timeout(wait, next_item(&mut status))
        .await
        .expect("StatusChanged arrives for the lost key")
        .expect("a signal");
    let args = signal.args().expect("args");
    assert_eq!(args.space, "work");
    let locked: SpaceStatus = serde_json::from_str(args.status).expect("a SpaceStatus");
    assert_eq!(locked.state, SpaceState::Locked);

    // It comes back: the next check opens the Space again and the status is read from it.
    rig.world.keys.0.unlock();
    rig.world.daemon.check_keys().await;
    let signal = tokio::time::timeout(wait, next_item(&mut status))
        .await
        .expect("StatusChanged arrives for the returned key")
        .expect("a signal");
    let open: SpaceStatus =
        serde_json::from_str(signal.args().expect("args").status).expect("a SpaceStatus");
    assert_eq!(open.state, SpaceState::Open);
    assert_eq!(open.facts, Count(1));
}

fn session_query(after: Option<Cursor>, limit: u32) -> EntriesQuery {
    EntriesQuery {
        kinds: vec![KindPattern::parse("companion.session.*").expect("pattern")],
        about: session_thing("s-1"),
        after,
        limit: Count(limit),
        bodies: BodyMode::Json,
    }
}

#[tokio::test]
async fn a_durable_append_and_its_pages_cross_the_bus() {
    let rig = rig().await;
    let router = rig.memory(Caller::Router).await;
    let mut acks = Vec::new();
    for slug in ["turn", "tool", "done"] {
        let record = session_entry("s-1", slug, slug).expect("fixture");
        acks.push(router.record_durable(record).await.expect("durable ack"));
    }
    assert!(
        acks.windows(2).all(|w| w[0].event.seq < w[1].event.seq),
        "the sequence only grows: {acks:?}"
    );

    let first = router
        .entries(space("work"), session_query(None, 2))
        .await
        .expect("first page");
    assert_eq!(first.entries.len(), 2);
    let cursor = first.next.expect("more to read");
    let second = router
        .entries(space("work"), session_query(Some(cursor), 2))
        .await
        .expect("second page");
    assert_eq!(second.entries.len(), 1, "resumes after the cursor");
    let seqs: Vec<_> = first
        .entries
        .iter()
        .chain(&second.entries)
        .map(|e| e.summary.event.seq)
        .collect();
    assert_eq!(seqs, acks.iter().map(|a| a.event.seq).collect::<Vec<_>>());
}

#[tokio::test]
async fn the_durable_members_refuse_as_the_error_of_their_name() {
    let rig = rig().await;
    let router = rig.memory(Caller::Router).await;
    let shell = rig.memory(Caller::ShellUi).await;
    let app = rig.memory(app()).await;
    let record = || session_entry("s-1", "turn", "x").expect("fixture");

    assert_eq!(
        app.record_durable(record()).await,
        Err(ClientError::Refused(Refusal::NotAllowed))
    );
    assert_eq!(
        app.entries(space("work"), session_query(None, 5)).await,
        Err(ClientError::Refused(Refusal::NotAllowed))
    );
    assert_eq!(
        shell
            .ask(MemoryRequest::Pause(
                space("work"),
                UnixSeconds(4_000_000_000)
            ))
            .await,
        Ok(MemoryReply::Ok)
    );
    assert_eq!(
        router.record_durable(record()).await,
        Err(ClientError::Refused(Refusal::NotKept(DropReason::Paused)))
    );
}
