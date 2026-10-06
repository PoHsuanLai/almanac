//! The person's settings reach a service that is already serving: the next request reads them.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::{ConsolidateWhen, MemoryService, MemorySettings};

type Service = MemoryService<FakeBackend>;

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn settings(f: impl FnOnce(&mut MemorySettings)) -> MemorySettings {
    let mut settings = MemorySettings::default();
    f(&mut settings);
    settings
}

async fn shell(service: &Service, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::ShellUi, request).await
}

async fn propose_pending(service: &Service) {
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("topic"),
        text: FactText::parse("Ana is the CFO.").expect("text"),
        links: vec![],
        supersedes: vec![],
    };
    let reply = service
        .handle(&Caller::Router, MemoryRequest::Propose(work(), draft))
        .await;
    assert!(
        matches!(reply, MemoryReply::Proposed(_, FactState::Pending)),
        "{reply:?}"
    );
}

async fn waiting(service: &Service) -> usize {
    match shell(service, MemoryRequest::Pending(work())).await {
        MemoryReply::Pending(facts) => facts.len(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_pending_fact_waits_as_long_as_the_persons_settings_say_and_a_change_applies_at_once() {
    let service = fake_service(ScriptedConsolidator::default());
    service.apply_settings(settings(|s| s.pending_ttl = DayCount(3)));
    propose_pending(&service).await;
    service.backend().advance_clock(2 * 86_400);
    assert_eq!(waiting(&service).await, 1, "two days is inside three");

    // The person shortens the wait: the very next request ages the fact out.
    service.apply_settings(settings(|s| s.pending_ttl = DayCount(1)));
    assert_eq!(waiting(&service).await, 0, "two days is past one");
}

#[tokio::test]
async fn the_shipped_wait_is_fourteen_days_until_the_settings_say_otherwise() {
    let service = fake_service(ScriptedConsolidator::default());
    propose_pending(&service).await;
    service.backend().advance_clock(10 * 86_400);
    assert_eq!(waiting(&service).await, 1);
}

async fn record_in(service: &Service, space: &str) {
    let mut record = mail_thread_archived().expect("fixture");
    record.space = SpaceId::parse(space).expect("space");
    let reply = service
        .handle(&Caller::Router, MemoryRequest::Record(record))
        .await;
    assert!(matches!(reply, MemoryReply::Recorded(_)), "{reply:?}");
}

fn vault_of(service: &Service, space: &str) -> VaultKind {
    service
        .metas()
        .into_iter()
        .find(|m| m.id.as_str() == space)
        .map(|m| m.vault)
        .expect("the Space exists")
}

#[tokio::test]
async fn a_new_space_is_made_as_the_settings_say_and_an_old_one_keeps_its_way() {
    let service = fake_service(ScriptedConsolidator::default());
    service.apply_settings(settings(|s| s.at_rest = VaultKind::Plain));
    record_in(&service, "work").await;
    service.apply_settings(settings(|s| s.at_rest = VaultKind::Sealed));
    record_in(&service, "home").await;
    record_in(&service, "work").await;
    assert_eq!(vault_of(&service, "work"), VaultKind::Plain);
    assert_eq!(vault_of(&service, "home"), VaultKind::Sealed);
}

async fn sweep_bodies(service: &Service) -> u32 {
    let swept = service.sweep_all().await;
    swept
        .iter()
        .filter_map(|(_, r)| r.as_ref().ok())
        .map(|r| r.bodies.0)
        .sum()
}

#[tokio::test]
async fn the_retention_the_settings_give_is_what_the_sweep_keeps() {
    // An audit event (`policy.ruled`) keeps its body 90 days unless the person says less.
    let kept = fake_service(ScriptedConsolidator::default());
    kept.handle(
        &Caller::Router,
        MemoryRequest::Record(policy_ask().expect("fixture")),
    )
    .await;
    kept.backend().advance_clock(11 * 86_400);
    assert_eq!(sweep_bodies(&kept).await, 0);

    let shorter = fake_service(ScriptedConsolidator::default());
    shorter.apply_settings(settings(|s| s.retention.audit_body = DayCount(10)));
    shorter
        .handle(
            &Caller::Router,
            MemoryRequest::Record(policy_ask().expect("fixture")),
        )
        .await;
    shorter.backend().advance_clock(11 * 86_400);
    assert_eq!(sweep_bodies(&shorter).await, 1);
    // The rules the person's `SetRule` made are not changed by it: memoryd persists those.
    assert_eq!(shorter.rules(), RuleSet::standard());
}

#[tokio::test]
async fn never_refuses_a_consolidation_and_manual_lets_one_run() {
    let service = fake_service(ScriptedConsolidator::default());
    service.apply_settings(settings(|s| s.consolidate = ConsolidateWhen::Never));
    assert!(matches!(
        shell(&service, MemoryRequest::RunConsolidation(work())).await,
        MemoryReply::Refused(Refusal::Invalid(_))
    ));
    service.apply_settings(settings(|s| s.consolidate = ConsolidateWhen::Manual));
    assert!(matches!(
        shell(&service, MemoryRequest::RunConsolidation(work())).await,
        MemoryReply::Consolidation(_)
    ));
}
