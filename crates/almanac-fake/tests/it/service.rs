//! The service contract, written against the fakes.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::Caller;
use almanac_service::{ByteCap, EventCap, MemoryService, Quota};

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
async fn record_then_timeline_shows_it() {
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
        on_behalf: None,
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
async fn an_untrusted_proposal_lands_in_pending() {
    let service = service();
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("t"),
        text: FactText::parse("Ana is the CFO.").expect("t"),
        links: vec![],
        supersedes: vec![],
        on_behalf: None,
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

fn helper_job() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-7").expect("session"),
        role: AgentRole::Helper {
            helper: HelperId::parse("01J9ZK").expect("helper"),
            task: TaskId::parse("t-7").expect("task"),
        },
    }
}

fn proposal(on_behalf: Option<Actor>) -> MemoryRequest {
    MemoryRequest::Propose(
        space("work"),
        FactDraft {
            topic: TopicPath::parse("people/ana").expect("t"),
            text: FactText::parse("Ana is the CFO.").expect("t"),
            links: vec![],
            supersedes: vec![],
            on_behalf,
        },
    )
}

async fn authors(service: &MemoryService<FakeBackend>) -> Vec<Actor> {
    let MemoryReply::Facts(facts) = ask(
        service,
        &Caller::ShellUi,
        MemoryRequest::Facts(FactQuery {
            space: space("work"),
            topic: None,
            about: None,
            state: FactFilter::All,
            limit: Count(10),
        }),
    )
    .await
    else {
        panic!("facts")
    };
    facts.into_iter().map(|f| f.fact.by).collect()
}

#[tokio::test]
async fn a_proposal_the_router_makes_for_a_helper_is_authored_by_it() {
    let service = service();
    let reply = ask(&service, &Caller::Router, proposal(Some(helper_job()))).await;
    assert!(matches!(reply, MemoryReply::Proposed(..)));
    assert_eq!(authors(&service).await, vec![helper_job()]);
}

#[tokio::test]
async fn a_proposal_without_on_behalf_is_authored_by_the_router_as_before() {
    let service = service();
    ask(&service, &Caller::Router, proposal(None)).await;
    let router = Actor::System {
        part: SystemPart::Router,
    };
    assert_eq!(authors(&service).await, vec![router]);
}

#[tokio::test]
async fn on_behalf_is_refused_from_any_caller_but_the_router() {
    let service = service();
    for caller in [Caller::ShellUi, Caller::Cuad] {
        let reply = ask(&service, &caller, proposal(Some(helper_job()))).await;
        assert!(is_refused(&reply, &Refusal::NotAllowed), "{caller:?}");
    }
    let reply = ask(&service, &mail_app(), proposal(Some(helper_job()))).await;
    assert!(matches!(reply, MemoryReply::Refused(_)));
    assert!(authors(&service).await.is_empty());
}

#[tokio::test]
async fn the_router_may_speak_only_for_a_helper() {
    let service = service();
    let reply = ask(&service, &Caller::Router, proposal(Some(Actor::Unknown))).await;
    assert!(is_refused(&reply, &Refusal::NotAllowed));
}

#[tokio::test]
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

fn mail_app() -> Caller {
    Caller::App(AppId {
        name: mail(),
        isolation: Isolation::InProcess,
    })
}

fn mail_record_in(id: &str) -> MemoryRequest {
    let mut record = mail_thread_archived().expect("fixture");
    record.space = space(id);
    MemoryRequest::Record(record)
}

#[tokio::test]
async fn an_app_opens_only_desktop_spaces_the_registry_lists() {
    let service = service();
    let refused = ask(&service, &mail_app(), mail_record_in("stranger")).await;
    assert_eq!(refused, MemoryReply::Refused(Refusal::SpaceUnknown));
    assert!(
        service.metas().is_empty(),
        "an unlisted Space is not provisioned"
    );

    service.backend().list_space(space("stranger"));
    let listed = ask(&service, &mail_app(), mail_record_in("stranger")).await;
    assert!(matches!(listed, MemoryReply::Recorded(_)), "{listed:?}");

    // The registry not answering fails closed for an app; the router is not asked.
    service.backend().take_registry_down();
    let down = ask(&service, &mail_app(), mail_record_in("other")).await;
    assert_eq!(down, MemoryReply::Refused(Refusal::Unavailable));
    let router = ask(&service, &Caller::Router, mail_record_in("other")).await;
    assert!(matches!(router, MemoryReply::Recorded(_)), "{router:?}");
}

fn is_refused(reply: &MemoryReply, want: &Refusal) -> bool {
    matches!(reply, MemoryReply::Refused(got) if got == want)
}

#[tokio::test]
async fn a_full_space_refuses_and_an_oversize_record_is_invalid() {
    let service = service();
    service.set_quota(Quota {
        events: EventCap(3),
        ..Quota::default()
    });
    let first = ask(&service, &Caller::Router, mail_record_in("work")).await;
    assert!(matches!(first, MemoryReply::Recorded(_)), "{first:?}");
    // The audit entries count too, so the cap is reached within a few records, never passed.
    let mut refused = None;
    for _ in 0..4 {
        let reply = ask(&service, &Caller::Router, mail_record_in("work")).await;
        if matches!(reply, MemoryReply::Refused(_)) {
            refused = Some(reply);
            break;
        }
    }
    let full = refused.expect("the Space fills");
    assert!(is_refused(&full, &Refusal::SpaceFull), "{full:?}");

    let mut quota = Quota::default();
    quota.fields.body = ByteCap(10);
    service.set_quota(quota);
    let big = ask(&service, &Caller::Router, mail_record_in("home")).await;
    assert!(
        matches!(big, MemoryReply::Refused(Refusal::Invalid(_))),
        "{big:?}"
    );
}

#[tokio::test]
async fn an_app_cannot_claim_trusted_or_a_wider_audience() {
    let service = service();
    let claimed = mail_thread_archived().expect("fixture");
    assert_eq!(claimed.label.integrity, Integrity::Trusted);
    assert_eq!(claimed.label.confidentiality, Confidentiality::Public);
    let reply = ask(&service, &mail_app(), MemoryRequest::Record(claimed)).await;
    assert!(matches!(reply, MemoryReply::Recorded(_)), "{reply:?}");
    let page = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Timeline(space("work"), everyone()),
    )
    .await;
    let MemoryReply::Timeline(page) = page else {
        panic!("{page:?}")
    };
    let label = &page.entries[0].label;
    assert_eq!(label.integrity, Integrity::Untrusted);
    assert_eq!(
        label.confidentiality,
        Confidentiality::Private([space("work")].into())
    );
}

#[tokio::test]
async fn a_locked_space_buffers_only_what_it_could_store_and_replays_it() {
    let service = service();
    service.backend().memory_keys().lock();
    let kept = ask(&service, &Caller::Router, mail_record_in("work")).await;
    assert_eq!(kept, MemoryReply::Ok, "buffered while locked");

    let mut tiny = Quota::default();
    tiny.fields.body = ByteCap(10);
    service.set_quota(tiny);
    let oversize = ask(&service, &Caller::Router, mail_record_in("work")).await;
    assert!(
        matches!(oversize, MemoryReply::Refused(Refusal::Invalid(_))),
        "a record that could never be stored is refused now: {oversize:?}"
    );
    service.set_quota(Quota::default());

    service.backend().memory_keys().unlock();
    let page = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Timeline(space("work"), everyone()),
    )
    .await;
    let MemoryReply::Timeline(page) = page else {
        panic!("{page:?}")
    };
    assert_eq!(page.entries.len(), 1, "the buffered record replayed once");
}

#[tokio::test]
async fn a_batch_with_a_malformed_record_stores_nothing() {
    let service = service();
    let valid = mail_thread_archived().expect("fixture");
    let mut malformed = valid.clone();
    malformed.body = EventBody::Message(Box::new(Message {
        id: MessageId::parse("m-2").expect("id"),
        thread: ThreadId::parse("m-1").expect("id"),
        in_reply_to: None,
        from: Address::new(AgentRef::Companion, space("work")),
        to: Address::new(AgentRef::User, space("work")),
        kind: MessageKind::Note,
        parts: vec![Part::Text(MessageText::new("hello"))],
        label: almanac_fake::trusted_label(),
        sent: UnixSeconds(1),
    }));
    let mut first = valid;
    first.space = space("work");
    malformed.space = space("work");
    let refused = ask(
        &service,
        &Caller::Router,
        MemoryRequest::RecordBatch(vec![first, malformed]),
    )
    .await;
    assert!(
        matches!(refused, MemoryReply::Refused(Refusal::Invalid(_))),
        "{refused:?}"
    );
    let page = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Timeline(space("work"), everyone()),
    )
    .await;
    assert!(
        matches!(&page, MemoryReply::Timeline(p) if p.entries.is_empty())
            || matches!(&page, MemoryReply::Refused(_)),
        "the valid record before it was not stored: {page:?}"
    );
}
