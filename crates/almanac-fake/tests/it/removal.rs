//! A removed desktop-wide Space: its memories move to the App Space of the app that wrote them
//! (pending ones stay pending), or are deleted when the person chose that; a move that stopped
//! half way finishes when asked again. Scratch Spaces, no clock, no bus.

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

async fn remove(service: &Service, fate: MemoryFate) -> MemoryReply {
    service
        .handle(&Caller::ShellUi, MemoryRequest::RemoveSpace(work(), fate))
        .await
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
    let reply = remove(&service, MemoryFate::MoveToApps).await;
    assert_eq!(
        reply,
        MemoryReply::Relocated(Relocation {
            moved: Count(3),
            kept_pending: Count(1),
            deleted: Count(0),
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
    let reply = remove(&service, MemoryFate::Delete).await;
    assert_eq!(
        reply,
        MemoryReply::Relocated(Relocation {
            moved: Count(0),
            kept_pending: Count(0),
            deleted: Count(3),
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
    let reply = remove(&service, MemoryFate::MoveToApps).await;
    assert!(matches!(reply, MemoryReply::Relocated(_)), "{reply:?}");
    assert_eq!(
        fact_ids(&store(&service, &home_of("org.quire.Mail"))).len(),
        1
    );
    assert_eq!(service.backend().removed_spaces(), vec![work()]);
    // Asked again once it is done, it has nothing left to do.
    assert_eq!(
        remove(&service, MemoryFate::MoveToApps).await,
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
    let reply = remove(&bare, MemoryFate::MoveToApps).await;
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
            MemoryRequest::RemoveSpace(work(), MemoryFate::Delete),
        )
        .await;
    assert_eq!(by_router, MemoryReply::Refused(Refusal::NotAllowed));
    let own = home_of(FILES);
    let reply = service
        .handle(
            &Caller::ShellUi,
            MemoryRequest::RemoveSpace(own, MemoryFate::Delete),
        )
        .await;
    assert!(
        matches!(reply, MemoryReply::Refused(Refusal::Invalid(_))),
        "{reply:?}"
    );
}
