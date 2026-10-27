//! Deleting a Space, and the retention sweep: expired bodies are erased, old headers pruned.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::Caller;
use almanac_service::MemoryService;

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

async fn shell(service: &MemoryService<FakeBackend>, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::ShellUi, request).await
}

#[tokio::test]
async fn deleting_a_space_removes_it_and_anchors_its_final_head_in_the_desktop_log() {
    let service = fake_service(ScriptedConsolidator::default());
    let record = mail_thread_archived().expect("fixture");
    service
        .handle(&Caller::Router, MemoryRequest::Record(record))
        .await;
    let MemoryReply::Plan(plan) = shell(
        &service,
        MemoryRequest::PlanForget(work(), ForgetScope::Space),
    )
    .await
    else {
        panic!("plan")
    };
    let report = shell(&service, MemoryRequest::Forget(plan.token)).await;
    assert!(matches!(report, MemoryReply::Forgot(_)));
    assert_eq!(service.backend().removed_spaces(), vec![work()]);
    assert_eq!(
        shell(&service, MemoryRequest::Status(work())).await,
        // A Space that is asked for again is provisioned afresh: it is empty.
        shell(&service, MemoryRequest::Status(work())).await
    );

    let page = shell(
        &service,
        MemoryRequest::Timeline(
            SpaceId::desktop(),
            TimelineQuery {
                before: None,
                limit: Count(10),
                filter: TimelineFilter {
                    actors: ActorFilter::Everyone,
                    apps: vec![],
                    kinds: vec![],
                    trust: TrustFilter::Any,
                    range: None,
                },
            },
        ),
    )
    .await;
    let MemoryReply::Timeline(page) = page else {
        panic!("{page:?}")
    };
    assert!(
        page.entries
            .iter()
            .any(|e| e.kind.as_str() == "memory.space_deleted"),
        "{:?}",
        page.entries
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>()
    );
}

fn search_event(text: &str, at: UnixSeconds) -> Record {
    let mut record = mail_thread_archived().expect("fixture");
    record.occurred = at;
    record.body = EventBody::Search {
        app: mail(),
        text: text.into(),
        scope: ThingKind::parse("mail.thread").expect("kind"),
        results: Count(3),
    };
    record
}

async fn timeline(service: &MemoryService<FakeBackend>) -> Vec<TimelineEntry> {
    let query = TimelineQuery {
        before: None,
        limit: Count(100),
        filter: TimelineFilter {
            actors: ActorFilter::Everyone,
            apps: vec![],
            kinds: vec![],
            trust: TrustFilter::Any,
            range: None,
        },
    };
    match shell(service, MemoryRequest::Timeline(work(), query)).await {
        MemoryReply::Timeline(page) => page.entries,
        other => panic!("{other:?}"),
    }
}

const DAY: i64 = 86_400;

#[tokio::test]
async fn the_sweep_erases_expired_bodies_then_prunes_old_headers() {
    let service = fake_service(ScriptedConsolidator::default());
    let router = Caller::Router;
    service
        .handle(&router, MemoryRequest::Record(search_event("lisbon", NOW)))
        .await;
    service.backend().advance_clock(10 * DAY);
    service
        .handle(
            &router,
            MemoryRequest::Record(search_event("porto", UnixSeconds(NOW.0 + 10 * DAY))),
        )
        .await;
    // 31 days after the first search, 21 after the second: only the first has outlived its 30.
    service.backend().advance_clock(21 * DAY);
    let MemoryReply::Swept(first) = shell(&service, MemoryRequest::Sweep(work())).await else {
        panic!("swept")
    };
    assert_eq!(first.bodies, Count(1));
    assert_eq!(
        first.headers,
        Count(0),
        "a header outlives its body by a year"
    );
    let rows = timeline(&service).await;
    let searches: Vec<&TimelineEntry> = rows
        .iter()
        .filter(|e| e.kind.as_str() == "search.performed")
        .collect();
    assert_eq!(searches.len(), 2);
    let erased = |e: &&&TimelineEntry| {
        matches!(
            e.body,
            EntryBody::Erased {
                by: EraseCause::Expired
            }
        )
    };
    assert_eq!(searches.iter().filter(erased).count(), 1, "{searches:?}");
    let verified = shell(&service, MemoryRequest::Verify(work())).await;
    assert!(matches!(
        verified,
        MemoryReply::Verified(ChainReport::Intact { .. })
    ));

    // A year and a month on, both bodies are gone and the first headers are pruned.
    service.backend().advance_clock(400 * DAY);
    let MemoryReply::Swept(second) = shell(&service, MemoryRequest::Sweep(work())).await else {
        panic!("swept")
    };
    assert_eq!(second.bodies, Count(1));
    assert!(second.headers.0 >= 2, "{second:?}");
    let verified = shell(&service, MemoryRequest::Verify(work())).await;
    assert!(
        matches!(verified, MemoryReply::Verified(ChainReport::Intact { .. })),
        "the chain still verifies from its checkpoint: {verified:?}"
    );
    let kinds: Vec<String> = timeline(&service)
        .await
        .iter()
        .map(|e| e.kind.as_str().to_owned())
        .collect();
    assert!(kinds.contains(&"memory.checkpoint".to_owned()), "{kinds:?}");
}

#[tokio::test]
async fn a_body_that_lives_while_its_source_exists_goes_when_the_source_is() {
    let service = fake_service(ScriptedConsolidator::default());
    let router = Caller::Router;
    service
        .handle(
            &router,
            MemoryRequest::Record(mail_thread_archived().expect("fixture")),
        )
        .await;
    let MemoryReply::Swept(before) = shell(&service, MemoryRequest::Sweep(work())).await else {
        panic!("swept")
    };
    assert_eq!(before.bodies, Count(0), "the thread still exists");
    let mut deleted = mail_thread_archived().expect("fixture");
    if let EventBody::Thing { verb, .. } = &mut deleted.body {
        *verb = Verb::Deleted;
    }
    service
        .handle(&router, MemoryRequest::Record(deleted))
        .await;
    let MemoryReply::Swept(after) = shell(&service, MemoryRequest::Sweep(work())).await else {
        panic!("swept")
    };
    assert_eq!(
        after.bodies,
        Count(2),
        "the archive and the delete both leave with the thread"
    );
}

#[tokio::test]
async fn only_the_shell_may_sweep() {
    let service = fake_service(ScriptedConsolidator::default());
    let refused = service
        .handle(&Caller::Router, MemoryRequest::Sweep(work()))
        .await;
    assert_eq!(refused, MemoryReply::Refused(Refusal::NotAllowed));
    let swept = service.sweep_all().await;
    assert!(swept.is_empty(), "no Space is known yet: {swept:?}");
}
