//! A removed desktop-wide Space: its memories move to the App Space of the app that wrote them
//! (pending ones stay pending) and its event history to that of the app that recorded it, or
//! either is deleted when the person chose that; a move that stopped half way finishes when
//! asked again. Scratch Spaces, no clock, no bus.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::MemoryService;
use jiff::tz::TimeZone;
use memfiles::Store;
use std::collections::BTreeSet;

type Service = MemoryService<FakeBackend>;

const FILES: &str = "org.quire.Files";
const SHELL: &str = "org.quire.Shell";

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn app(name: &str) -> AppName {
    AppName::parse(name).expect("app")
}

fn home_of(name: &str) -> SpaceId {
    SpaceId::for_app(&app(name), LocalSpace(0), None).expect("app space")
}

fn store(service: &Service, space: &SpaceId) -> Store<SharedVault> {
    Store::new(
        service.backend().vault_of(space),
        space.clone(),
        TimeZone::UTC,
    )
}

fn fact(n: u8, by: Actor, label: Label) -> Fact {
    Fact {
        id: FactId::mint(u64::from(n), [n; 10]),
        text: FactText::parse(&format!("fact {n}")).expect("text"),
        recorded: UnixSeconds(NOW.0 + i64::from(n)),
        by,
        label,
        links: vec![],
        supersedes: vec![],
        valid: Validity::Unstated,
    }
}

fn private_to_work() -> Label {
    Label {
        integrity: Integrity::Untrusted,
        confidentiality: Confidentiality::Private(BTreeSet::from([work()])),
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::Mail]),
    }
}

fn planner() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-1").expect("s"),
        role: AgentRole::Planner,
    }
}

fn topic(name: &str) -> TopicPath {
    TopicPath::parse(name).expect("topic")
}

/// "work" holds a mail fact, a Files fact still pending, and a planner's fact no app wrote.
fn service_with_work() -> Service {
    let service = fake_service(ScriptedConsolidator::default());
    service.set_fallback_owner(app(SHELL));
    service.register(SpaceMeta {
        id: work(),
        created: NOW,
        replica: ReplicaId([9; 16]),
        vault: VaultKind::Plain,
        format: 1,
    });
    let held = store(&service, &work());
    let by_mail = Actor::User { via: mail() };
    held.append(&topic("people/ana"), fact(1, by_mail, trusted_label()))
        .expect("append");
    held.stage(
        fact(2, Actor::App { app: app(FILES) }, private_to_work()),
        topic("files/notes"),
    )
    .expect("stage");
    held.append(&topic("people/bo"), fact(3, planner(), trusted_label()))
        .expect("append");
    service
}

async fn remove(service: &Service, removal: Removal) -> MemoryReply {
    service
        .handle(
            &Caller::ShellUi,
            MemoryRequest::RemoveSpace(work(), removal),
        )
        .await
}

fn erase_all() -> Removal {
    Removal {
        memories: MemoryFate::Delete,
        history: HistoryFate::Delete,
    }
}

/// Three events in "work": Mail's archive, a Files save and the companion's forward.
fn history() -> Vec<Record> {
    let mut saved = file_saved_from_attachment().expect("fixture");
    saved.actor = Actor::App { app: app(FILES) };
    vec![
        mail_thread_archived().expect("fixture"),
        saved,
        companion_forwarded().expect("fixture"),
    ]
}

async fn record_history(service: &Service) -> Vec<EventRef> {
    let mut refs = Vec::new();
    for record in history() {
        let reply = service
            .handle(&Caller::Router, MemoryRequest::Record(record))
            .await;
        let MemoryReply::Recorded(event) = reply else {
            panic!("{reply:?}")
        };
        refs.push(event);
    }
    refs
}

/// The events of `space` that are not the service's own audit, oldest first.
async fn events_in(service: &Service, space: &SpaceId) -> Vec<TimelineEntry> {
    let query = TimelineQuery {
        before: None,
        limit: Count(50),
        filter: TimelineFilter {
            actors: ActorFilter::Everyone,
            apps: vec![],
            kinds: vec![],
            trust: TrustFilter::Any,
            range: None,
        },
    };
    let reply = service
        .handle(
            &Caller::ShellUi,
            MemoryRequest::Timeline(space.clone(), query),
        )
        .await;
    let MemoryReply::Timeline(page) = reply else {
        panic!("{reply:?}")
    };
    let mut kept: Vec<TimelineEntry> = page
        .entries
        .into_iter()
        .filter(|e| !e.kind.as_str().starts_with("memory."))
        .collect();
    kept.reverse();
    kept
}

fn kinds(events: &[TimelineEntry]) -> Vec<&str> {
    events.iter().map(|e| e.kind.as_str()).collect()
}

fn fact_ids(store: &Store<SharedVault>) -> Vec<FactId> {
    let mut found: Vec<FactId> = store
        .topics()
        .expect("topics")
        .iter()
        .flat_map(|t| store.read(t).expect("read").blocks)
        .filter_map(|b| match b {
            memfiles::Block::Fact(f) => Some(f.id),
            _ => None,
        })
        .chain(
            store
                .pending()
                .expect("pending")
                .into_iter()
                .map(|(_, f)| f.id),
        )
        .collect();
    found.sort();
    found
}

#[tokio::test]
async fn memories_move_to_the_app_that_wrote_them_and_pending_stays_pending() {
    let service = service_with_work();
    let reply = remove(&service, Removal::KEEP_ALL).await;
    assert_eq!(
        reply,
        MemoryReply::Relocated(Relocation {
            moved: Count(3),
            kept_pending: Count(1),
            ..Relocation::NONE
        })
    );
    let (mail_home, files_home, shell_home) =
        (home_of("org.quire.Mail"), home_of(FILES), home_of(SHELL));
    assert_eq!(
        store(&service, &mail_home).topics().expect("t"),
        vec![topic("people/ana")]
    );
    assert_eq!(
        store(&service, &shell_home).topics().expect("t"),
        vec![topic("people/bo")]
    );
    let files = store(&service, &files_home);
    assert!(
        files.topics().expect("t").is_empty(),
        "pending is not active"
    );
    let pending = files.pending().expect("pending");
    assert_eq!(pending.len(), 1);
    let (staged_topic, staged) = &pending[0];
    assert_eq!(staged_topic, &topic("files/notes"));
    assert_eq!(staged.label.integrity, Integrity::Untrusted);
    assert_eq!(
        staged.label.confidentiality,
        Confidentiality::Private(BTreeSet::from([files_home])),
        "private to the Space it now lives in"
    );
    assert_eq!(service.backend().removed_spaces(), vec![work()]);
    assert!(service.metas().iter().all(|m| m.id != work()));
}

#[tokio::test]
async fn deleting_instead_moves_nothing() {
    let service = service_with_work();
    let reply = remove(&service, erase_all()).await;
    assert_eq!(
        reply,
        MemoryReply::Relocated(Relocation {
            deleted: Count(3),
            ..Relocation::NONE
        })
    );
    for name in ["org.quire.Mail", FILES, SHELL] {
        let home = store(&service, &home_of(name));
        assert!(home.topics().expect("t").is_empty());
        assert!(home.pending().expect("p").is_empty());
    }
    assert_eq!(service.backend().removed_spaces(), vec![work()]);
}

#[tokio::test]
async fn a_move_that_stopped_half_way_finishes_without_duplicates() {
    let service = service_with_work();
    // The daemon died after copying the mail fact and before deleting the Space.
    let copied = fact(1, Actor::User { via: mail() }, trusted_label());
    store(&service, &home_of("org.quire.Mail"))
        .append(&topic("people/ana"), copied)
        .expect("append");
    let reply = remove(&service, Removal::KEEP_ALL).await;
    assert!(matches!(reply, MemoryReply::Relocated(_)), "{reply:?}");
    assert_eq!(
        fact_ids(&store(&service, &home_of("org.quire.Mail"))).len(),
        1
    );
    assert_eq!(service.backend().removed_spaces(), vec![work()]);
    // Asked again once it is done, it has nothing left to do.
    assert_eq!(
        remove(&service, Removal::KEEP_ALL).await,
        MemoryReply::Relocated(Relocation::NONE)
    );
}

#[tokio::test]
async fn memories_no_app_wrote_need_an_app_to_take_them() {
    let bare = fake_service(ScriptedConsolidator::default());
    bare.register(SpaceMeta {
        id: work(),
        created: NOW,
        replica: ReplicaId([9; 16]),
        vault: VaultKind::Plain,
        format: 1,
    });
    store(&bare, &work())
        .append(&topic("people/bo"), fact(3, planner(), trusted_label()))
        .expect("append");
    let reply = remove(&bare, Removal::KEEP_ALL).await;
    assert!(
        matches!(reply, MemoryReply::Refused(Refusal::Invalid(_))),
        "{reply:?}"
    );
    assert!(
        bare.backend().removed_spaces().is_empty(),
        "nothing was lost"
    );
}

#[tokio::test]
async fn only_the_shell_removes_a_space_and_only_a_desktop_wide_one() {
    let service = service_with_work();
    let by_router = service
        .handle(
            &Caller::Router,
            MemoryRequest::RemoveSpace(work(), erase_all()),
        )
        .await;
    assert_eq!(by_router, MemoryReply::Refused(Refusal::NotAllowed));
    let own = home_of(FILES);
    let reply = service
        .handle(
            &Caller::ShellUi,
            MemoryRequest::RemoveSpace(own, erase_all()),
        )
        .await;
    assert!(
        matches!(reply, MemoryReply::Refused(Refusal::Invalid(_))),
        "{reply:?}"
    );
}

/// "work" with its history recorded, and a mail fact that cites the archive event.
async fn service_with_history() -> (Service, Vec<EventRef>) {
    let service = service_with_work();
    let events = record_history(&service).await;
    let mut cited = fact(4, Actor::User { via: mail() }, trusted_label());
    cited.links = vec![Link::Event(events[0].clone())];
    store(&service, &work())
        .append(&topic("people/cy"), cited)
        .expect("append");
    (service, events)
}

fn cited_link(service: &Service) -> Vec<Link> {
    let mail_store = store(service, &home_of("org.quire.Mail"));
    mail_store
        .read(&topic("people/cy"))
        .expect("read")
        .blocks
        .into_iter()
        .find_map(|b| match b {
            memfiles::Block::Fact(f) => Some(f.links),
            _ => None,
        })
        .expect("the cited fact moved")
}

#[tokio::test]
async fn kept_history_follows_the_app_that_recorded_it() {
    let (service, _) = service_with_history().await;
    let reply = remove(&service, Removal::KEEP_ALL).await;
    let MemoryReply::Relocated(report) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(
        (report.events_moved, report.events_deleted),
        (Count(3), Count(0))
    );
    let mail_events = events_in(&service, &home_of("org.quire.Mail")).await;
    assert_eq!(kinds(&mail_events), vec!["thing.archived"]);
    assert_eq!(
        kinds(&events_in(&service, &home_of(FILES)).await),
        vec!["file.created"]
    );
    let forwarded = events_in(&service, &home_of(SHELL)).await;
    assert_eq!(kinds(&forwarded), vec!["thing.forwarded"]);
    assert_eq!(
        forwarded[0].label.confidentiality,
        Confidentiality::Private(BTreeSet::from([home_of(SHELL)])),
        "private to the log it now lives in"
    );
    assert_eq!(
        cited_link(&service),
        vec![Link::Event(mail_events[0].event.clone())],
        "a fact's link follows its event"
    );
    assert_eq!(service.backend().removed_spaces(), vec![work()]);
}

#[tokio::test]
async fn deleted_history_leaves_no_event_and_no_link() {
    let (service, _) = service_with_history().await;
    let removal = Removal {
        memories: MemoryFate::MoveToApps,
        history: HistoryFate::Delete,
    };
    let reply = remove(&service, removal).await;
    let MemoryReply::Relocated(report) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(
        (report.events_moved, report.events_deleted),
        (Count(0), Count(3))
    );
    assert_eq!(report.moved, Count(4));
    for name in ["org.quire.Mail", FILES, SHELL] {
        assert!(
            events_in(&service, &home_of(name)).await.is_empty(),
            "{name}"
        );
    }
    assert!(cited_link(&service).is_empty());
}

#[tokio::test]
async fn history_can_be_kept_while_memories_are_deleted() {
    let (service, _) = service_with_history().await;
    let removal = Removal {
        memories: MemoryFate::Delete,
        history: HistoryFate::Keep,
    };
    let reply = remove(&service, removal).await;
    let MemoryReply::Relocated(report) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(
        (report.moved, report.deleted, report.events_moved),
        (Count(0), Count(4), Count(3))
    );
    assert_eq!(
        kinds(&events_in(&service, &home_of("org.quire.Mail")).await),
        vec!["thing.archived"]
    );
}

#[tokio::test]
async fn a_history_move_that_stopped_half_way_finishes_without_duplicates() {
    let (service, _) = service_with_history().await;
    // The daemon died after the archive event reached Mail's log and before the Space went.
    let mut copied = mail_thread_archived().expect("fixture");
    copied.space = home_of("org.quire.Mail");
    let reply = service
        .handle(&Caller::Router, MemoryRequest::Record(copied))
        .await;
    assert!(matches!(reply, MemoryReply::Recorded(_)), "{reply:?}");
    let reply = remove(&service, Removal::KEEP_ALL).await;
    assert!(matches!(reply, MemoryReply::Relocated(_)), "{reply:?}");
    let mail_events = events_in(&service, &home_of("org.quire.Mail")).await;
    assert_eq!(kinds(&mail_events), vec!["thing.archived"], "not twice");
    assert_eq!(
        cited_link(&service),
        vec![Link::Event(mail_events[0].event.clone())]
    );
}
