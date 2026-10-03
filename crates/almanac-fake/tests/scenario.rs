//! Record -> Search -> Forget cascade through the service, and Inject into a budget, over the
//! fakes.

use almanac_core::*;
use almanac_fake::*;

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn titled(title: &str, key: &str, label: Label) -> Record {
    let mut record = mail_thread_archived().expect("fixture");
    record.label = label;
    if let EventBody::Thing { thing, .. } = &mut record.body {
        thing.title = title.into();
        thing.thing = self::thing("mail.thread", key).expect("thing");
    }
    record
}

async fn ask(
    service: &almanac_service::MemoryService<FakeBackend>,
    caller: &Caller,
    request: MemoryRequest,
) -> MemoryReply {
    service.handle(caller, request).await
}

fn search(text: &str, over: RecallOver) -> MemoryRequest {
    MemoryRequest::Search(RecallQuery {
        space: work(),
        text: text.into(),
        limit: Count(10),
        over,
    })
}

#[tokio::test]
async fn record_search_forget_cascade() {
    let service = fake_service(ScriptedConsolidator::default());
    let thing = thing("mail.thread", "7f3a").expect("thing");
    let recorded = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Record(mail_thread_archived().expect("fixture")),
    )
    .await;
    assert!(matches!(recorded, MemoryReply::Recorded(_)));
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("t"),
        text: FactText::parse("Ana sent the budget report.").expect("t"),
        links: vec![Link::Thing(thing.clone())],
        supersedes: vec![],
    };
    let proposed = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Propose(work(), draft),
    )
    .await;
    assert!(matches!(
        proposed,
        MemoryReply::Proposed(_, FactState::Active)
    ));

    let MemoryReply::Hits(hits) = ask(
        &service,
        &Caller::Router,
        search("budget", RecallOver::Both),
    )
    .await
    else {
        panic!("hits")
    };
    assert_eq!(hits.len(), 2, "the event and the fact: {hits:?}");
    assert!(hits.iter().any(|h| matches!(h.doc, MemoryItem::Fact(_))));
    assert!(hits.iter().any(|h| matches!(h.doc, MemoryItem::Event(_))));

    let MemoryReply::Plan(plan) = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::PlanForget(work(), ForgetScope::Thing(thing)),
    )
    .await
    else {
        panic!("plan")
    };
    assert_eq!((plan.events, plan.index_docs), (Count(1), Count(2)));
    assert_eq!(plan.facts.len(), 1);
    let MemoryReply::Forgot(report) = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Forget(plan.token.clone()),
    )
    .await
    else {
        panic!("forgot")
    };
    assert_eq!(report.counts.index_docs, plan.index_docs);

    let MemoryReply::Hits(after) = ask(
        &service,
        &Caller::Router,
        search("budget", RecallOver::Both),
    )
    .await
    else {
        panic!("hits")
    };
    assert!(after.is_empty(), "nothing is left to find: {after:?}");
    let again = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Forget(plan.token),
    )
    .await;
    assert!(
        matches!(again, MemoryReply::Refused(_)),
        "a plan applies once"
    );
}

#[tokio::test]
async fn inject_fits_the_budget_and_the_trust_filter() {
    let service = fake_service(ScriptedConsolidator::default());
    let long = "budget ".repeat(20);
    for (title, key, label) in [
        ("budget review", "a1", trusted_label()),
        (long.as_str(), "a2", trusted_label()),
        ("budget forecast", "a3", trusted_label()),
        ("budget from a stranger", "a4", mail_label()),
    ] {
        ask(
            &service,
            &Caller::Router,
            MemoryRequest::Record(titled(title, key, label)),
        )
        .await;
    }
    let inject = |budget: u32, k: u32, trust| {
        MemoryRequest::Inject(InjectQuery {
            space: work(),
            text: "budget".into(),
            budget: Tokens(budget),
            k: Count(k),
            over: RecallOver::Both,
            trust,
        })
    };
    let MemoryReply::Hits(hits) = ask(
        &service,
        &Caller::Router,
        inject(12, 10, TrustFilter::TrustedOnly),
    )
    .await
    else {
        panic!("hits")
    };
    let cost: u32 = hits
        .iter()
        .map(|h| estimate_tokens(h.text.as_str()).0)
        .sum();
    assert!(cost <= 12, "cost {cost}");
    assert_eq!(
        hits.len(),
        2,
        "the long one is skipped, the small ones fit: {hits:?}"
    );
    assert!(hits.iter().all(|h| h.label.integrity == Integrity::Trusted));

    let MemoryReply::Hits(one) =
        ask(&service, &Caller::Router, inject(1000, 1, TrustFilter::Any)).await
    else {
        panic!("hits")
    };
    assert_eq!(one.len(), 1, "k caps the count");
}

fn receipt() -> ConfirmReceipt {
    ConfirmReceipt {
        id: ConfirmId::parse("c-1").expect("id"),
        input: InputProof::ShellCaller,
        at: NOW,
    }
}

#[tokio::test]
async fn pending_facts_settle_and_keep_endorses() {
    let service = fake_service(ScriptedConsolidator::default());
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("t"),
        text: FactText::parse("Ana is the CFO.").expect("t"),
        links: vec![],
        supersedes: vec![],
    };
    let MemoryReply::Proposed(id, FactState::Pending) = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Propose(work(), draft),
    )
    .await
    else {
        panic!("a router proposal waits for the person")
    };
    let kept = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Settle(id.clone(), Settlement::Keep(receipt())),
    )
    .await;
    assert_eq!(kept, MemoryReply::Ok);
    let MemoryReply::Facts(facts) = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Facts(FactQuery {
            space: work(),
            topic: None,
            about: None,
            state: FactFilter::Active,
            limit: Count(10),
        }),
    )
    .await
    else {
        panic!("facts")
    };
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].fact.label.integrity, Integrity::Trusted);
    let again = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Settle(id, Settlement::Discard),
    )
    .await;
    assert_eq!(again, MemoryReply::Refused(Refusal::NotPending));
}

#[tokio::test]
async fn the_desktop_scope_refuses_what_the_person_did_not_state() {
    let service = fake_service(ScriptedConsolidator::default());
    let draft = |text: &str| FactDraft {
        topic: TopicPath::parse("prefs/meetings").expect("t"),
        text: FactText::parse(text).expect("t"),
        links: vec![],
        supersedes: vec![],
    };
    let desktop = SpaceId::desktop();
    let from_router = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Propose(desktop.clone(), draft("Prefers mornings.")),
    )
    .await;
    assert!(matches!(
        from_router,
        MemoryReply::Refused(Refusal::Invalid(_))
    ));
    let from_person = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Propose(desktop, draft("Prefers mornings.")),
    )
    .await;
    assert!(matches!(
        from_person,
        MemoryReply::Proposed(_, FactState::Active)
    ));
}

#[tokio::test]
async fn the_chain_verifies_and_the_export_carries_it() {
    let service = fake_service(ScriptedConsolidator::default());
    ask(
        &service,
        &Caller::Router,
        MemoryRequest::Record(mail_thread_archived().expect("fixture")),
    )
    .await;
    let verified = ask(&service, &Caller::ShellUi, MemoryRequest::Verify(work())).await;
    assert!(matches!(
        verified,
        MemoryReply::Verified(ChainReport::Intact { .. })
    ));
    let mut tar = Vec::new();
    let options = ExportOptions {
        spaces: vec![],
        verification_key: VerificationKey::Omit,
    };
    let MemoryReply::Exported(manifest) =
        service.export(&Caller::ShellUi, &options, &mut tar).await
    else {
        panic!("export")
    };
    assert_eq!(manifest.counts.events, Count(1));
    let names: Vec<String> = tar::Archive::new(tar.as_slice())
        .entries()
        .expect("tar")
        .filter_map(|e| Some(e.ok()?.path().ok()?.to_string_lossy().into_owned()))
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("work/events.jsonl")),
        "{names:?}"
    );
    assert!(!names.iter().any(|n| n.ends_with("digest.key")));
    let refused = service
        .export(&Caller::Router, &options, &mut Vec::new())
        .await;
    assert_eq!(refused, MemoryReply::Refused(Refusal::NotAllowed));
}

#[tokio::test]
async fn a_paused_space_keeps_headers_only_for_audit_records_and_drops_the_rest() {
    let service = fake_service(ScriptedConsolidator::default());
    let until = UnixSeconds(NOW.0 + 3600);
    let paused = ask(
        &service,
        &Caller::ShellUi,
        MemoryRequest::Pause(work(), until),
    )
    .await;
    assert_eq!(paused, MemoryReply::Ok);
    let dropped = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Record(mail_thread_archived().expect("fixture")),
    )
    .await;
    assert_eq!(dropped, MemoryReply::Ok, "nothing was kept");
    let kept = ask(
        &service,
        &Caller::Router,
        MemoryRequest::Record(policy_ask().expect("fixture")),
    )
    .await;
    assert!(
        matches!(kept, MemoryReply::Recorded(_)),
        "audit class keeps its header"
    );
}
