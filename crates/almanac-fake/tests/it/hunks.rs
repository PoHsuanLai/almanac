//! Consolidation applies every kind of hunk and a revert puts the files back; every test seeds
//! the Space's files first, the way a person's editor would have left them.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::{ConsolidateError, Draft, MemoryService};
use jiff::tz::TimeZone;
use memfiles::{Block, TopicFile, Vault, VaultPath, render_topic};

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn topic(name: &str) -> TopicPath {
    TopicPath::parse(name).expect("topic")
}

fn fact_id(n: u8) -> FactId {
    FactId::mint(u64::from(n), [n; 10])
}

fn fact(n: u8, text: &str) -> Fact {
    Fact {
        id: fact_id(n),
        text: FactText::parse(text).expect("text"),
        recorded: NOW,
        by: Actor::User {
            via: AppName::parse("org.quire.Shell").expect("app"),
        },
        label: trusted_label(),
        links: vec![],
        supersedes: vec![],
        valid: Validity::Unstated,
    }
}

fn render(file: &TopicFile) -> String {
    render_topic(file, &TimeZone::UTC)
}

fn file_of(name: &str, blocks: Vec<Block>) -> TopicFile {
    TopicFile {
        topic: topic(name),
        title: name.rsplit('/').next().unwrap_or(name).to_owned(),
        blocks,
    }
}

fn path(name: &str) -> VaultPath {
    VaultPath::topic(&topic(name))
}

/// Puts `text` at the topic's path in the Space's vault, behind the service's back.
fn put(service: &MemoryService<FakeBackend>, name: &str, text: &str) {
    service
        .backend()
        .vault_of(&work())
        .write_atomic(&path(name), text.as_bytes())
        .expect("write");
}

fn text_of(service: &MemoryService<FakeBackend>, name: &str) -> String {
    let bytes = service
        .backend()
        .vault_of(&work())
        .read(&path(name))
        .expect("file");
    String::from_utf8(bytes).expect("utf8")
}

fn service_drafting(hunks: Vec<Hunk>) -> MemoryService<FakeBackend> {
    fake_service(ScriptedConsolidator::answering([
        Ok::<_, ConsolidateError>(Draft { hunks }),
    ]))
}

async fn shell(service: &MemoryService<FakeBackend>, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::ShellUi, request).await
}

async fn run(service: &MemoryService<FakeBackend>) -> DraftView {
    match shell(service, MemoryRequest::RunConsolidation(work())).await {
        MemoryReply::Consolidation(view) => view,
        other => panic!("{other:?}"),
    }
}

async fn found(service: &MemoryService<FakeBackend>, text: &str) -> bool {
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
        // The vector half returns its nearest whatever they say: a hit counts when it reads so.
        MemoryReply::Hits(hits) => hits.iter().any(|h| h.text.as_str().contains(text)),
        other => panic!("{other:?}"),
    }
}

/// Whether the lexical index matches `text` (what the index says, not what the file says: hits
/// read their text from the files).
async fn lexically(service: &MemoryService<FakeBackend>, text: &str) -> bool {
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

async fn facts(service: &MemoryService<FakeBackend>) -> Vec<FactView> {
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

fn tidy(name: &str, before: &str, after: &str) -> Hunk {
    Hunk::Tidy(TidyHunk {
        topic: topic(name),
        before: before.into(),
        after: after.into(),
    })
}

fn reworded(file: &TopicFile, text: &str) -> TopicFile {
    let mut out = file.clone();
    for block in &mut out.blocks {
        if let Block::Fact(f) = block {
            f.text = FactText::parse(text).expect("text");
        }
    }
    out
}

#[tokio::test]
async fn tidy_rewords_a_topic_and_a_revert_restores_it() {
    let before_file = file_of("people/ana", vec![Block::Fact(fact(1, "Ana is the CFO."))]);
    let (before, after) = (
        render(&before_file),
        render(&reworded(
            &before_file,
            "Ana is the chief financial officer.",
        )),
    );
    let service = service_drafting(vec![tidy("people/ana", &before, &after)]);
    put(&service, "people/ana", &before);
    assert!(
        found(&service, "CFO").await,
        "the Space indexes what it finds on open"
    );

    let view = run(&service).await;
    assert_eq!(
        text_of(&service, "people/ana"),
        after,
        "the tidy was applied"
    );
    assert!(!found(&service, "CFO").await, "the index follows the file");
    assert!(found(&service, "financial").await);

    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run)).await,
        MemoryReply::Ok
    );
    assert_eq!(
        text_of(&service, "people/ana"),
        before,
        "the pre-image is back"
    );
    assert!(found(&service, "CFO").await);
    assert!(!found(&service, "financial").await);
}

#[tokio::test]
async fn a_tidy_that_is_not_a_tidy_is_not_applied() {
    let file = file_of("people/ana", vec![Block::Fact(fact(1, "Ana is the CFO."))]);
    let before = render(&file);
    let mut laundered = reworded(&file, "Ana is the CFO.");
    for block in &mut laundered.blocks {
        if let Block::Fact(f) = block {
            f.label = mail_label();
        }
    }
    let mut dropped = file.clone();
    dropped.blocks.clear();
    dropped.blocks.push(Block::Verbatim("nothing here".into()));
    let mut renamed = file.clone();
    renamed.topic = topic("people/bob");
    let service = service_drafting(vec![
        tidy("people/ana", &before, &render(&laundered)),
        tidy("people/ana", &before, &render(&dropped)),
        tidy("people/ana", &before, &render(&renamed)),
        tidy(
            "people/ana",
            "# stale",
            &render(&reworded(&file, "Changed.")),
        ),
    ]);
    put(&service, "people/ana", &before);
    run(&service).await;
    assert_eq!(
        text_of(&service, "people/ana"),
        before,
        "a changed label, a dropped fact, another topic and a stale before all leave the file"
    );
}

#[tokio::test]
async fn a_bullet_the_person_wrote_is_stamped_and_the_revert_unstamps_it() {
    let mut file = file_of(
        "prefs/meetings",
        vec![Block::Fact(fact(1, "Prefers mornings."))],
    );
    file.blocks
        .push(Block::Unstamped("Dislikes Monday calls".into()));
    let before = render(&file);
    let service = fake_service(ScriptedConsolidator::default());
    put(&service, "prefs/meetings", &before);
    assert!(
        !found(&service, "Monday").await,
        "an unstamped bullet is not a fact yet"
    );

    let view = run(&service).await;
    assert!(
        view.hunks.iter().any(
            |h| matches!(h, Hunk::Stamp { text, .. } if text.as_str() == "Dislikes Monday calls")
        ),
        "the run proposes the stamp itself: {:?}",
        view.hunks
    );
    let all = facts(&service).await;
    assert_eq!(all.len(), 2);
    let stamped = all
        .iter()
        .find(|v| v.fact.text.as_str() == "Dislikes Monday calls")
        .expect("stamped");
    assert_eq!(stamped.fact.label.integrity, Integrity::Trusted);
    assert!(matches!(stamped.fact.by, Actor::User { .. }));
    assert!(found(&service, "Monday").await, "and it is searchable");
    assert!(!text_of(&service, "prefs/meetings").contains("\n- Dislikes Monday calls\n- "));

    shell(&service, MemoryRequest::Revert(view.run)).await;
    assert_eq!(text_of(&service, "prefs/meetings"), before);
    assert!(!found(&service, "Monday").await);
    assert_eq!(facts(&service).await.len(), 1);
}

#[tokio::test]
async fn an_external_edit_reaches_the_index() {
    let original = file_of("people/ana", vec![Block::Fact(fact(1, "Ana is the CFO."))]);
    let before = render(&original);
    let edited = render(&reworded(&original, "Ana left the company."));
    let service = service_drafting(vec![Hunk::ExternalEdit {
        topic: topic("people/ana"),
        before: before.clone().into(),
        after: edited.clone().into(),
    }]);
    put(&service, "people/ana", &before);
    assert!(lexically(&service, "CFO").await);
    put(&service, "people/ana", &edited);
    assert!(
        !lexically(&service, "company").await,
        "the index has not heard of the edit yet"
    );

    run(&service).await;
    assert!(!lexically(&service, "CFO").await);
    assert!(lexically(&service, "company").await);
    assert_eq!(
        text_of(&service, "people/ana"),
        edited,
        "the person's edit stands"
    );
}

#[tokio::test]
async fn a_flag_is_kept_for_the_person_and_changes_no_fact() {
    let file = file_of("people/ana", vec![Block::Fact(fact(1, "Ana is the CFO."))]);
    let before = render(&file);
    let service = service_drafting(vec![Hunk::Flag {
        facts: vec![fact_id(1)],
        note: "Ana's title may be out of date".into(),
    }]);
    put(&service, "people/ana", &before);
    let view = run(&service).await;
    assert_eq!(text_of(&service, "people/ana"), before);
    let flags = VaultPath::parse(&format!("flags/{}.json", view.run)).expect("path");
    let saved = service
        .backend()
        .vault_of(&work())
        .read(&flags)
        .expect("flags file");
    let text = String::from_utf8(saved).expect("utf8");
    assert!(text.contains("out of date") && text.contains(&fact_id(1).to_string()));
}

#[tokio::test]
async fn marks_are_kept_in_the_vault() {
    let service = fake_service(ScriptedConsolidator::default());
    let marked = thing("mail.thread", "7f3a").expect("thing");
    let reply = shell(
        &service,
        MemoryRequest::Mark(MarkRequest {
            space: work(),
            thing: marked.clone(),
            mark: MarkKind::DoNotRemember,
        }),
    )
    .await;
    assert_eq!(reply, MemoryReply::Ok);
    let path = VaultPath::parse("meta/marks.json").expect("path");
    let saved = service
        .backend()
        .vault_of(&work())
        .read(&path)
        .expect("marks file");
    let marks: Marks = serde_json::from_slice(&saved).expect("json");
    assert!(marks.things.contains(&marked));
    let dropped = service
        .handle(
            &Caller::Router,
            MemoryRequest::Record(mail_thread_archived().expect("fixture")),
        )
        .await;
    assert_eq!(dropped, MemoryReply::Ok, "a marked thing is not remembered");
    shell(
        &service,
        MemoryRequest::Mark(MarkRequest {
            space: work(),
            thing: marked,
            mark: MarkKind::Clear,
        }),
    )
    .await;
    let saved = service
        .backend()
        .vault_of(&work())
        .read(&path)
        .expect("marks file");
    let marks: Marks = serde_json::from_slice(&saved).expect("json");
    assert!(marks.things.is_empty());
}

#[tokio::test]
async fn use_counts_come_from_the_routers_audited_reads() {
    let file = file_of("people/ana", vec![Block::Fact(fact(1, "Ana is the CFO."))]);
    let service = fake_service(ScriptedConsolidator::default());
    put(&service, "people/ana", &render(&file));
    assert_eq!(facts(&service).await[0].used, UseCount(0));
    assert_eq!(facts(&service).await[0].last_used, None);
    // Two reads by the router, one by the shell (which is not a prompt and is not counted).
    assert!(found(&service, "CFO").await);
    service.backend().advance_clock(60);
    assert!(found(&service, "CFO").await);
    let view = facts(&service).await.remove(0);
    assert_eq!(view.used, UseCount(2));
    assert_eq!(view.last_used, Some(UnixSeconds(NOW.0 + 60)));
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
