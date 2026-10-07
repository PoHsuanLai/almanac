//! Forget wins over Revert: nothing the person forgot comes back by a revert, whatever the last
//! applied run kept in memory. Every test seeds the Space's files first.

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

const SECRET: &str = "Zebra Corp";

fn ana_file() -> TopicFile {
    file_of(
        "people/ana",
        vec![Block::Fact(fact(1, "Ana is the CFO of Zebra Corp."))],
    )
}

fn other_file() -> TopicFile {
    file_of("prefs/tea", vec![Block::Fact(fact(2, "Likes green tea."))])
}

/// Every file the Space keeps that could hold text: topics, run files, flags.
fn all_text(service: &MemoryService<FakeBackend>) -> String {
    let vault = service.backend().vault_of(&work());
    ["people", "prefs", "consolidation", "flags", "meta"]
        .iter()
        .filter_map(|d| vault.list(&VaultPath::parse(d).expect("dir")).ok())
        .flatten()
        .filter_map(|p| vault.read(&p).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect()
}

async fn forget_fact(service: &MemoryService<FakeBackend>, id: FactId) {
    let MemoryReply::Plan(plan) = shell(
        service,
        MemoryRequest::PlanForget(work(), ForgetScope::Fact(id)),
    )
    .await
    else {
        panic!("plan")
    };
    assert!(matches!(
        shell(service, MemoryRequest::Forget(plan.token)).await,
        MemoryReply::Forgot(_)
    ));
}

/// A service whose one run tidies `people/ana`; both topics are seeded and indexed.
async fn tidied() -> (MemoryService<FakeBackend>, DraftView, String) {
    let before = render(&ana_file());
    let after = render(&reworded(
        &ana_file(),
        "Ana is the finance chief of Zebra Corp.",
    ));
    let service = service_drafting(vec![tidy("people/ana", &before, &after)]);
    put(&service, "people/ana", &before);
    put(&service, "prefs/tea", &render(&other_file()));
    assert!(found(&service, "Zebra").await);
    let view = run(&service).await;
    assert_eq!(text_of(&service, "people/ana"), after, "the tidy applied");
    (service, view, before)
}

fn refused_reason() -> MemoryReply {
    MemoryReply::Refused(Refusal::Invalid(
        "a memory this run touched was forgotten since".into(),
    ))
}

#[tokio::test]
async fn forgetting_a_fact_the_run_touched_refuses_the_revert_and_nothing_comes_back() {
    let (service, view, _) = tidied().await;
    forget_fact(&service, fact_id(1)).await;
    assert!(
        !all_text(&service).contains(SECRET),
        "gone after the forget"
    );
    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run.clone())).await,
        refused_reason()
    );
    let text = all_text(&service);
    assert!(!text.contains(SECRET), "and still gone after: {text}");
    assert!(!text.contains("CFO"), "the pre-image text is not back");
    assert!(text.contains("green tea"), "the rest of the Space is kept");
    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run)).await,
        refused_reason(),
        "the refusal holds on a second try"
    );
}

#[tokio::test]
async fn any_forget_while_pre_images_are_held_refuses_the_revert() {
    let (service, view, _) = tidied().await;
    forget_fact(&service, fact_id(2)).await;
    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run)).await,
        refused_reason()
    );
    assert!(
        !all_text(&service).contains("CFO"),
        "no pre-image came back"
    );
}

#[tokio::test]
async fn a_revert_with_no_forget_in_between_still_works() {
    let (service, view, before) = tidied().await;
    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run)).await,
        MemoryReply::Ok
    );
    assert_eq!(text_of(&service, "people/ana"), before);
}

#[tokio::test]
async fn a_forgotten_event_whose_text_is_only_in_a_pre_image_stays_forgotten() {
    let mut file = other_file();
    file.blocks
        .push(Block::Unstamped("Zebra Corp quarterly".into()));
    let service = fake_service(ScriptedConsolidator::default());
    put(&service, "prefs/tea", &render(&file));
    let mut record = mail_thread_archived().expect("fixture");
    record.body = EventBody::Search {
        app: AppName::parse("org.quire.Mail").expect("app"),
        text: "Zebra Corp quarterly".into(),
        scope: ThingKind::parse("mail.thread").expect("kind"),
        results: Count(1),
    };
    let MemoryReply::Recorded(event) = service
        .handle(&Caller::Router, MemoryRequest::Record(record))
        .await
    else {
        panic!("recorded")
    };
    let view = run(&service).await;
    let MemoryReply::Plan(plan) = shell(
        &service,
        MemoryRequest::PlanForget(work(), ForgetScope::Event(event)),
    )
    .await
    else {
        panic!("plan")
    };
    assert!(matches!(
        shell(&service, MemoryRequest::Forget(plan.token)).await,
        MemoryReply::Forgot(_)
    ));
    let after_forget = text_of(&service, "prefs/tea");
    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run)).await,
        refused_reason()
    );
    assert_eq!(
        text_of(&service, "prefs/tea"),
        after_forget,
        "the pre-image holding the event's text was not written back"
    );
}

#[tokio::test]
async fn a_bullet_stamped_by_the_run_stays_forgotten_after_a_revert() {
    let mut file = other_file();
    file.blocks
        .push(Block::Unstamped("Owes Zebra Corp a call".into()));
    let service = fake_service(ScriptedConsolidator::default());
    put(&service, "prefs/tea", &render(&file));
    let view = run(&service).await;
    let stamped = facts(&service)
        .await
        .into_iter()
        .find(|v| v.fact.text.as_str() == "Owes Zebra Corp a call")
        .expect("stamped");
    forget_fact(&service, stamped.fact.id.clone()).await;
    assert!(!all_text(&service).contains(SECRET));
    assert_eq!(
        shell(&service, MemoryRequest::Revert(view.run)).await,
        refused_reason()
    );
    let text = all_text(&service);
    assert!(
        !text.contains(SECRET),
        "the unstamped bullet did not return: {text}"
    );
    assert!(text.contains("green tea"));
}

#[tokio::test]
async fn the_view_served_after_a_forget_holds_no_forgotten_text() {
    let (service, _, _) = tidied().await;
    forget_fact(&service, fact_id(1)).await;
    let MemoryReply::Consolidation(view) =
        shell(&service, MemoryRequest::Consolidation(work())).await
    else {
        panic!("view")
    };
    let wire = serde_json::to_string(&view).expect("json");
    assert!(!wire.contains("Zebra"), "{wire}");
    assert!(!wire.contains("CFO"), "{wire}");
}
