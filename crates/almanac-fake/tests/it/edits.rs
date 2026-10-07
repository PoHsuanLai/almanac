//! What the service notices by itself: topic files edited outside it (the baseline), the topic
//! text a consolidation reads, the notes a run flags, and the events the bus hears of (a pending
//! fact settled or aged out, a key lost and back).

use almanac_core::*;
use almanac_fake::*;
use almanac_service::{Backend, Draft, MemoryService, ServiceEvent};
use memfiles::{Vault, VaultPath};

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn topic(name: &str) -> TopicPath {
    TopicPath::parse(name).expect("topic")
}

fn path(name: &str) -> VaultPath {
    VaultPath::topic(&topic(name))
}

type Service = MemoryService<FakeBackend>;

async fn shell(service: &Service, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::ShellUi, request).await
}

async fn propose(service: &Service, topic_name: &str, text: &str) -> FactId {
    let draft = FactDraft {
        topic: topic(topic_name),
        text: FactText::parse(text).expect("text"),
        links: vec![],
        supersedes: vec![],
    };
    match shell(service, MemoryRequest::Propose(work(), draft)).await {
        MemoryReply::Proposed(id, FactState::Active) => id,
        other => panic!("{other:?}"),
    }
}

async fn run(service: &Service) -> DraftView {
    match shell(service, MemoryRequest::RunConsolidation(work())).await {
        MemoryReply::Consolidation(view) => view,
        other => panic!("{other:?}"),
    }
}

fn text_of(service: &Service, name: &str) -> String {
    let bytes = service
        .backend()
        .vault_of(&work())
        .read(&path(name))
        .expect("file");
    String::from_utf8(bytes).expect("utf8")
}

/// The person edits the file in their editor.
fn edit(service: &Service, name: &str, from: &str, to: &str) -> String {
    let edited = text_of(service, name).replace(from, to);
    service
        .backend()
        .vault_of(&work())
        .write_atomic(&path(name), edited.as_bytes())
        .expect("write");
    edited
}

async fn lexically(service: &Service, text: &str) -> bool {
    let query = RecallQuery {
        space: work(),
        text: text.into(),
        limit: Count(10),
        over: RecallOver::Facts,
    };
    match service
        .handle(&Caller::Router, MemoryRequest::Search(query))
        .await
    {
        MemoryReply::Hits(hits) => hits
            .iter()
            .any(|h| matches!(h.why, RecallWhy::Lexical { .. } | RecallWhy::Both { .. })),
        other => panic!("{other:?}"),
    }
}

async fn facts(service: &Service) -> Vec<FactView> {
    let query = FactQuery {
        space: work(),
        topic: None,
        about: None,
        state: FactFilter::All,
        limit: Count(50),
    };
    match shell(service, MemoryRequest::Facts(query)).await {
        MemoryReply::Facts(v) => v,
        other => panic!("{other:?}"),
    }
}

fn external_edits(view: &DraftView) -> Vec<(&TopicPath, &UserText, &UserText)> {
    view.hunks
        .iter()
        .filter_map(|h| match h {
            Hunk::ExternalEdit {
                topic,
                before,
                after,
            } => Some((topic, before, after)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_run_reports_a_file_edited_since_the_service_last_wrote_it() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    let written = text_of(&service, "people/ana");
    let edited = edit(
        &service,
        "people/ana",
        "Ana is the CFO.",
        "Ana left the company.",
    );
    assert!(
        !lexically(&service, "company").await,
        "the index has not heard of the edit yet"
    );

    let view = run(&service).await;
    let [(name, before, after)] = external_edits(&view)[..] else {
        panic!("{:?}", view.hunks)
    };
    assert_eq!(name, &topic("people/ana"));
    assert_eq!(before.as_str(), written, "the text the service left");
    assert_eq!(after.as_str(), edited, "the person's edit");
    assert!(lexically(&service, "company").await, "the index follows");
    assert!(!lexically(&service, "CFO").await);
    assert_eq!(text_of(&service, "people/ana"), edited, "the edit stands");

    let again = run(&service).await;
    assert!(
        external_edits(&again).is_empty(),
        "reported once: {:?}",
        again.hunks
    );
}

#[tokio::test]
async fn the_services_own_writes_are_not_edits() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    propose(&service, "people/ana", "Ana lives in Lisbon.").await;
    propose(&service, "people/bo", "Bo is the CTO.").await;
    let view = run(&service).await;
    assert!(external_edits(&view).is_empty(), "{:?}", view.hunks);
}

#[tokio::test]
async fn a_service_write_does_not_absorb_an_edit_made_before_it() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    let written = text_of(&service, "people/ana");
    edit(
        &service,
        "people/ana",
        "Ana is the CFO.",
        "Ana left the company.",
    );
    // The service appends to the same file after the person's edit.
    propose(&service, "people/ana", "Ana lives in Lisbon.").await;

    let view = run(&service).await;
    let [(_, before, after)] = external_edits(&view)[..] else {
        panic!("{:?}", view.hunks)
    };
    assert_eq!(
        before.as_str(),
        written,
        "the baseline is still the old text"
    );
    assert!(after.as_str().contains("left the company"));
    assert!(
        after.as_str().contains("Lisbon"),
        "and now holds the new fact"
    );
}

#[tokio::test]
async fn a_file_the_person_deleted_leaves_the_index() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    assert!(lexically(&service, "CFO").await);
    service
        .backend()
        .vault_of(&work())
        .remove(&path("people/ana"))
        .expect("remove");
    // The next request that may write topic files looks.
    propose(&service, "people/bo", "Bo is the CTO.").await;
    assert!(!lexically(&service, "CFO").await);
    assert!(lexically(&service, "CTO").await);
}

#[tokio::test]
async fn forgetting_a_fact_forgets_it_in_the_baseline_too() {
    let service = fake_service(ScriptedConsolidator::default());
    let id = propose(&service, "people/ana", "Ana is the CFO of Zebra Corp.").await;
    propose(&service, "people/ana", "Ana lives in Lisbon.").await;
    // An edit is pending when the person forgets the first fact: the baseline is the pre-edit text.
    edit(&service, "people/ana", "Lisbon", "Porto");
    let MemoryReply::Plan(plan) = shell(
        &service,
        MemoryRequest::PlanForget(work(), ForgetScope::Fact(id)),
    )
    .await
    else {
        panic!("plan")
    };
    assert!(matches!(
        shell(&service, MemoryRequest::Forget(plan.token)).await,
        MemoryReply::Forgot(_)
    ));
    let vault = service.backend().vault_of(&work());
    let baseline = vault
        .read(&VaultPath::parse("meta/baseline.json").expect("path"))
        .expect("baseline");
    let baseline = String::from_utf8(baseline).expect("utf8");
    assert!(
        !baseline.contains("Zebra"),
        "the forgotten text is gone from the baseline: {baseline}"
    );
    assert!(baseline.contains("Lisbon"), "the rest is the pre-edit text");
}

#[tokio::test]
async fn the_consolidator_reads_the_topic_files_as_text() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    propose(&service, "prefs/meetings", "Prefers mornings.").await;
    run(&service).await;
    let seen = service.backend().consolidator().inputs();
    let [input] = seen.as_slice() else {
        panic!("{seen:?}")
    };
    let topics: Vec<(String, String)> = input
        .topics
        .iter()
        .map(|t| (t.topic.to_string(), t.text.as_str().to_owned()))
        .collect();
    assert_eq!(
        topics,
        vec![
            ("people/ana".to_owned(), text_of(&service, "people/ana")),
            (
                "prefs/meetings".to_owned(),
                text_of(&service, "prefs/meetings")
            ),
        ]
    );
}

fn answer(service: &Service, hunks: Vec<Hunk>) {
    service.backend().consolidator().push(Ok(Draft { hunks }));
}

#[tokio::test]
async fn a_tidy_proposed_from_the_topic_text_applies_and_is_not_an_edit() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    let before = text_of(&service, "people/ana");
    let after = before.replace("Ana is the CFO.", "Ana is the chief financial officer.");
    answer(
        &service,
        vec![Hunk::Tidy(TidyHunk {
            topic: topic("people/ana"),
            before: UserText::new(before),
            after: UserText::new(after.clone()),
        })],
    );
    run(&service).await;
    assert_eq!(text_of(&service, "people/ana"), after);
    assert!(lexically(&service, "financial").await);
    let view = run(&service).await;
    assert!(
        external_edits(&view).is_empty(),
        "the service's own tidy is not an edit: {:?}",
        view.hunks
    );
}

#[tokio::test]
async fn a_flag_shows_on_the_fact_and_goes_when_the_fact_is_forgotten() {
    let service = fake_service(ScriptedConsolidator::default());
    let first = propose(&service, "people/ana", "Ana is the CFO.").await;
    let second = propose(&service, "people/ana", "Ana lives in Lisbon.").await;
    answer(
        &service,
        vec![Hunk::Flag {
            facts: vec![first.clone()],
            note: UserText::new("Ana's title may be out of date".to_owned()),
        }],
    );
    assert!(facts(&service).await.iter().all(|v| v.flagged.is_empty()));
    let view = run(&service).await;

    let all = facts(&service).await;
    let flagged = all.iter().find(|v| v.fact.id == first).expect("fact");
    assert_eq!(
        flagged.flagged,
        vec![FlagNote {
            run: view.run.clone(),
            note: UserText::new("Ana's title may be out of date".to_owned()),
        }]
    );
    let other = all.iter().find(|v| v.fact.id == second).expect("fact");
    assert!(other.flagged.is_empty(), "only the flagged fact shows it");

    let MemoryReply::Plan(plan) = shell(
        &service,
        MemoryRequest::PlanForget(work(), ForgetScope::Fact(first)),
    )
    .await
    else {
        panic!("plan")
    };
    assert!(matches!(
        shell(&service, MemoryRequest::Forget(plan.token)).await,
        MemoryReply::Forgot(_)
    ));
    let flags = VaultPath::parse(&format!("flags/{}.json", view.run)).expect("path");
    assert!(
        service.backend().vault_of(&work()).read(&flags).is_err(),
        "a note about nothing left is gone with its fact"
    );
}

fn receipt() -> ConfirmReceipt {
    ConfirmReceipt {
        id: ConfirmId::parse("c-1").expect("id"),
        input: InputProof::ShellCaller,
        at: NOW,
        covers: Confidentiality::Secret,
    }
}

async fn propose_pending(service: &Service, text: &str) -> FactId {
    let draft = FactDraft {
        topic: topic("people/ana"),
        text: FactText::parse(text).expect("text"),
        links: vec![],
        supersedes: vec![],
    };
    match service
        .handle(&Caller::Router, MemoryRequest::Propose(work(), draft))
        .await
    {
        MemoryReply::Proposed(id, FactState::Pending) => id,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn settling_a_pending_fact_raises_pending_changed() {
    let service = fake_service(ScriptedConsolidator::default());
    let id = propose_pending(&service, "Ana is the CFO.").await;
    assert!(
        service.take_events().is_empty(),
        "a proposal is the reply's"
    );
    for settlement in [Settlement::Keep(receipt()), Settlement::Discard] {
        let id = match settlement {
            Settlement::Keep(_) => id.clone(),
            Settlement::Discard => propose_pending(&service, "Ana lives in Lisbon.").await,
        };
        assert_eq!(
            shell(&service, MemoryRequest::Settle(id, settlement)).await,
            MemoryReply::Ok
        );
        assert_eq!(
            service.take_events(),
            vec![ServiceEvent::PendingChanged(work())]
        );
        assert!(service.take_events().is_empty(), "taken once");
    }
}

#[tokio::test]
async fn an_aged_out_pending_fact_raises_pending_changed_once() {
    let service = fake_service(ScriptedConsolidator::default());
    propose_pending(&service, "Ana is the CFO.").await;
    service.backend().advance_clock(13 * 86_400);
    let MemoryReply::Pending(waiting) = shell(&service, MemoryRequest::Pending(work())).await
    else {
        panic!("pending")
    };
    assert_eq!(waiting.len(), 1);
    assert!(service.take_events().is_empty(), "nothing aged yet");

    service.backend().advance_clock(2 * 86_400);
    let MemoryReply::Pending(waiting) = shell(&service, MemoryRequest::Pending(work())).await
    else {
        panic!("pending")
    };
    assert!(waiting.is_empty(), "past fourteen days");
    assert_eq!(
        service.take_events(),
        vec![ServiceEvent::PendingChanged(work())]
    );
    shell(&service, MemoryRequest::Pending(work())).await;
    assert!(service.take_events().is_empty(), "and not again");
}

#[tokio::test]
async fn the_daily_sweep_ages_pending_facts_nobody_asked_about() {
    let service = fake_service(ScriptedConsolidator::default());
    propose_pending(&service, "Ana is the CFO.").await;
    service.backend().advance_clock(15 * 86_400);
    let swept = service.sweep_all().await;
    assert!(swept.iter().all(|(_, r)| r.is_ok()), "{swept:?}");
    assert!(
        service
            .take_events()
            .contains(&ServiceEvent::PendingChanged(work()))
    );
}

#[tokio::test]
async fn a_lost_key_is_announced_once_and_so_is_its_return() {
    let service = fake_service(ScriptedConsolidator::default());
    propose(&service, "people/ana", "Ana is the CFO.").await;
    assert!(service.take_events().is_empty());

    service.backend().memory_keys().lock();
    service.check_keys().await;
    assert_eq!(service.take_events(), vec![ServiceEvent::Locked(work())]);
    assert_eq!(
        shell(&service, MemoryRequest::Status(work())).await,
        MemoryReply::Refused(Refusal::SpaceLocked)
    );
    assert!(
        service.take_events().is_empty(),
        "a request to a locked Space does not say it again"
    );
    service.check_keys().await;
    assert!(service.take_events().is_empty());
    let summaries = shell(&service, MemoryRequest::Spaces).await;
    let MemoryReply::Spaces(spaces) = summaries else {
        panic!("{summaries:?}")
    };
    assert!(
        spaces
            .iter()
            .any(|s| s.id == work() && s.state == SpaceState::Locked)
    );

    service.backend().memory_keys().unlock();
    service.check_keys().await;
    assert_eq!(
        service.take_events(),
        vec![ServiceEvent::StatusChanged(work())]
    );
    let MemoryReply::Status(status) = shell(&service, MemoryRequest::Status(work())).await else {
        panic!("status")
    };
    assert_eq!(status.state, SpaceState::Open);
    assert_eq!(status.facts, Count(1), "the files were there all along");
}

#[tokio::test]
async fn a_request_that_finds_the_key_gone_announces_it_too() {
    let service = fake_service(ScriptedConsolidator::default());
    service.backend().memory_keys().lock();
    assert_eq!(
        shell(&service, MemoryRequest::Status(work())).await,
        MemoryReply::Refused(Refusal::SpaceLocked)
    );
    assert_eq!(service.take_events(), vec![ServiceEvent::Locked(work())]);
}
