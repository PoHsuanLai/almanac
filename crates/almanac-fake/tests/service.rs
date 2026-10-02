//! The service contract, written against the fakes. Each test is `#[ignore]`d until
//! `MemoryService::handle` is built (fill wave 2; FINDINGS.md): the bodies are the acceptance
//! the fill must meet, and they compile now so the frozen API is exercised.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::MemoryService;

const WHY: &str = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)";

fn service() -> MemoryService<FakeBackend> {
    fake_service(ScriptedConsolidator::default())
}

fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space")
}

async fn ask(
    service: &MemoryService<FakeBackend>,
    caller: &Caller,
    request: MemoryRequest,
) -> MemoryReply {
    service.handle(caller, request).await
}

fn everyone() -> TimelineQuery {
    TimelineQuery {
        before: None,
        limit: Count(50),
        filter: TimelineFilter {
            actors: ActorFilter::Everyone,
            apps: vec![],
            kinds: vec![],
            trust: TrustFilter::Any,
            range: None,
        },
    }
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn record_then_timeline_shows_it() {
    let _ = WHY;
    let service = service();
    let record = mail_thread_archived().expect("fixture");
    let reply = ask(&service, &Caller::Router, MemoryRequest::Record(record)).await;
    assert!(matches!(reply, MemoryReply::Recorded(_)));
    let page = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Timeline(space("work"), everyone()),
    )
    .await;
    let MemoryReply::Timeline(page) = page else {
        panic!("{page:?}")
    };
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].kind.as_str(), "thing.archived");
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn forget_thing_cascades() {
    let service = service();
    let thing = thing("mail.thread", "7f3a").expect("thing");
    for record in [mail_thread_archived(), companion_forwarded(), policy_ask()] {
        ask(
            &service,
            &Caller::Router,
            MemoryRequest::Record(record.expect("fixture")),
        )
        .await;
    }
    let plan = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::PlanForget(space("work"), ForgetScope::Thing(thing)),
    )
    .await;
    let MemoryReply::Plan(plan) = plan else {
        panic!("{plan:?}")
    };
    assert_eq!(
        plan.events,
        Count(3),
        "area payloads are in the closure by their `things`"
    );
    let report = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Forget(plan.token),
    )
    .await;
    let MemoryReply::Forgot(report) = report else {
        panic!("{report:?}")
    };
    assert_eq!(
        report.counts.events, plan.events,
        "counts equal the plan preview exactly"
    );
    let after = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Timeline(space("work"), everyone()),
    )
    .await;
    let MemoryReply::Timeline(page) = after else {
        panic!("{after:?}")
    };
    assert!(
        page.entries
            .iter()
            .all(|e| e.body != EntryBody::Present || e.things.is_empty()),
        "bodies are gone, headers stay"
    );
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn plan_goes_stale_when_new_derivation_appears() {
    let service = service();
    let thing = thing("mail.thread", "7f3a").expect("thing");
    ask(
        &service,
        &Caller::Router,
        MemoryRequest::Record(mail_thread_archived().expect("fixture")),
    )
    .await;
    let plan = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::PlanForget(space("work"), ForgetScope::Thing(thing.clone())),
    )
    .await;
    let MemoryReply::Plan(plan) = plan else {
        panic!("{plan:?}")
    };
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("t"),
        text: FactText::parse("Ana sent the budget.").expect("t"),
        links: vec![Link::Thing(thing)],
        supersedes: vec![],
    };
    ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Propose(space("work"), draft),
    )
    .await;
    let reply = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Forget(plan.token),
    )
    .await;
    assert_eq!(reply, MemoryReply::Refused(Refusal::PlanStale));
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn the_router_cannot_forget_or_settle() {
    let service = service();
    let forget = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Forget(PlanToken::parse("p-1").expect("t")),
    )
    .await;
    assert_eq!(forget, MemoryReply::Refused(Refusal::NotAllowed));
    let settle = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Settle(FactId::mint(1, [0; 10]), Settlement::Discard),
    )
    .await;
    assert_eq!(settle, MemoryReply::Refused(Refusal::NotAllowed));
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn router_reads_are_audited() {
    let service = service();
    let query = RecallQuery {
        space: space("work"),
        text: "budget".into(),
        limit: Count(5),
        over: RecallOver::Both,
    };
    ask(&service, &Caller::Router, MemoryRequest::Search(query)).await;
    let page = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Timeline(space("work"), everyone()),
    )
    .await;
    let MemoryReply::Timeline(page) = page else {
        panic!("{page:?}")
    };
    assert!(
        page.entries
            .iter()
            .any(|e| e.kind.as_str() == "memory.read"),
        "every read logs Memory.Read"
    );
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn an_untrusted_proposal_lands_in_pending() {
    let service = service();
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("t"),
        text: FactText::parse("Ana is the CFO.").expect("t"),
        links: vec![],
        supersedes: vec![],
    };
    let reply = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Propose(space("work"), draft),
    )
    .await;
    assert!(
        matches!(reply, MemoryReply::Proposed(_, FactState::Pending))
            || matches!(reply, MemoryReply::Proposed(_, FactState::Active))
    );
}

#[tokio::test]
#[ignore = "MemoryService::handle is a todo!() until fill wave 2 (FINDINGS.md)"]
async fn space_delete_destroys_key() {
    let service = service();
    ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Forget(PlanToken::parse("p-space").expect("t")),
    )
    .await;
    let key = almanac_seal::KeyStore::get(service.backend().memory_keys(), &space("home")).await;
    assert_eq!(key, Err(almanac_seal::KeyError::Missing));
}

#[test]
fn the_fakes_build_without_a_service_body() {
    let service = service();
    let metas = fake_space_metas(NOW);
    assert_eq!(metas.len(), 3);
    let _ = service.backend().memory_keys();
}
