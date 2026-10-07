//! `memory.consolidation.apply = review`: a run stops at `Proposed`, changes nothing, and the
//! person's `ApplyConsolidation` runs the machine's `Proceed` step over the kept hunks, each
//! checked again against the Space as it is then. Scratch Space, scripted drafts, no clock.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::{ConsolidateApply, ConsolidateError, Draft, MemoryService, MemorySettings};
use jiff::tz::TimeZone;
use memfiles::{Block, TopicFile, Vault, VaultPath, render_topic};

type Service = MemoryService<FakeBackend>;

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn topic(name: &str) -> TopicPath {
    TopicPath::parse(name).expect("topic")
}

fn fact_id(n: u8) -> FactId {
    FactId::mint(u64::from(n), [n; 10])
}

fn fact(n: u8, text: &str, links: Vec<Link>) -> Fact {
    Fact {
        id: fact_id(n),
        text: FactText::parse(text).expect("text"),
        recorded: NOW,
        by: Actor::User {
            via: AppName::parse("org.quire.Shell").expect("app"),
        },
        label: trusted_label(),
        links,
        supersedes: vec![],
        valid: Validity::Unstated,
    }
}

fn render(file: &TopicFile) -> String {
    render_topic(file, &TimeZone::UTC)
}

fn ana(text: &str) -> TopicFile {
    TopicFile {
        topic: topic("people/ana"),
        title: "ana".to_owned(),
        blocks: vec![Block::Fact(fact(1, text, vec![]))],
    }
}

fn path() -> VaultPath {
    VaultPath::topic(&topic("people/ana"))
}

fn put(service: &Service, text: &str) {
    service
        .backend()
        .vault_of(&work())
        .write_atomic(&path(), text.as_bytes())
        .expect("write");
}

fn text_of(service: &Service) -> String {
    let bytes = service
        .backend()
        .vault_of(&work())
        .read(&path())
        .expect("file");
    String::from_utf8(bytes).expect("utf8")
}

fn reviewing(hunks: Vec<Hunk>) -> Service {
    let service = fake_service(ScriptedConsolidator::answering([
        Ok::<_, ConsolidateError>(Draft { hunks }),
    ]));
    service.apply_settings(MemorySettings {
        apply: ConsolidateApply::Review,
        ..MemorySettings::default()
    });
    service
}

async fn shell(service: &Service, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::ShellUi, request).await
}

async fn run(service: &Service) -> DraftView {
    match shell(service, MemoryRequest::RunConsolidation(work())).await {
        MemoryReply::Consolidation(view) => view,
        other => panic!("{other:?}"),
    }
}

async fn apply(service: &Service, run: &RunId) -> MemoryReply {
    shell(service, MemoryRequest::ApplyConsolidation(run.clone())).await
}

async fn ids(service: &Service) -> Vec<FactId> {
    let query = FactQuery {
        space: work(),
        topic: None,
        about: None,
        state: FactFilter::Active,
        limit: Count(50),
    };
    match shell(service, MemoryRequest::Facts(query)).await {
        MemoryReply::Facts(v) => v.into_iter().map(|f| f.fact.id).collect(),
        other => panic!("{other:?}"),
    }
}

fn tidy(before: &str, after: &str) -> Hunk {
    Hunk::Tidy(TidyHunk {
        topic: topic("people/ana"),
        before: before.into(),
        after: after.into(),
    })
}

fn supersede() -> Hunk {
    Hunk::Supersede {
        old: fact_id(1),
        new: Fact {
            supersedes: vec![fact_id(1)],
            ..fact(2, "Ana is the CFO of Acme.", vec![Link::Fact(fact_id(1))])
        },
    }
}

#[tokio::test]
async fn a_run_under_review_stops_at_proposed_and_apply_then_changes_the_files() {
    let (before, after) = (
        render(&ana("Ana is the CFO.")),
        render(&ana("Ana is the chief financial officer.")),
    );
    let service = reviewing(vec![tidy(&before, &after)]);
    put(&service, &before);

    let proposed = run(&service).await;
    assert_eq!(proposed.state, RunState::Proposed);
    assert_eq!(proposed.hunks.len(), 1, "the kept hunks are the proposal");
    assert_eq!(text_of(&service), before, "nothing changed yet");
    assert_eq!(
        shell(&service, MemoryRequest::Consolidation(work())).await,
        MemoryReply::Consolidation(proposed.clone()),
        "the proposal can be read back"
    );

    let MemoryReply::Consolidation(done) = apply(&service, &proposed.run).await else {
        panic!("apply answers with the applied view");
    };
    assert_eq!(done.state, RunState::Applied);
    assert_eq!(done.hunks, proposed.hunks);
    assert_eq!(text_of(&service), after);

    // The applied run is an ordinary applied run: its pre-image brings the file back.
    assert_eq!(
        shell(&service, MemoryRequest::Revert(proposed.run)).await,
        MemoryReply::Ok
    );
    assert_eq!(text_of(&service), before);
}

#[tokio::test]
async fn a_proposed_run_applies_once_and_cannot_be_reverted_before_it_is_applied() {
    let (before, after) = (
        render(&ana("Ana is the CFO.")),
        render(&ana("Ana is the chief financial officer.")),
    );
    let service = reviewing(vec![tidy(&before, &after)]);
    put(&service, &before);
    let proposed = run(&service).await;

    assert!(matches!(
        shell(&service, MemoryRequest::Revert(proposed.run.clone())).await,
        MemoryReply::Refused(Refusal::Invalid(_))
    ));
    assert_eq!(text_of(&service), before);

    assert!(matches!(
        apply(&service, &proposed.run).await,
        MemoryReply::Consolidation(_)
    ));
    assert!(
        matches!(
            apply(&service, &proposed.run).await,
            MemoryReply::Refused(Refusal::Invalid(_))
        ),
        "a second apply is refused, not repeated"
    );
    let unknown = RunId::parse("c-nothing").expect("run");
    assert!(matches!(
        apply(&service, &unknown).await,
        MemoryReply::Refused(Refusal::Invalid(_))
    ));
}

#[tokio::test]
async fn only_the_shell_applies_a_proposal() {
    let (before, after) = (
        render(&ana("Ana is the CFO.")),
        render(&ana("Ana is the chief financial officer.")),
    );
    let service = reviewing(vec![tidy(&before, &after)]);
    put(&service, &before);
    let proposed = run(&service).await;
    let reply = service
        .handle(
            &Caller::Router,
            MemoryRequest::ApplyConsolidation(proposed.run),
        )
        .await;
    assert!(matches!(reply, MemoryReply::Refused(_)), "{reply:?}");
    assert_eq!(text_of(&service), before);
}

#[tokio::test]
async fn a_supersede_applies_with_the_old_fact_kept_in_its_file() {
    let file = render(&ana("Ana is the CFO."));
    let service = reviewing(vec![supersede()]);
    put(&service, &file);
    let proposed = run(&service).await;
    assert_eq!(ids(&service).await, vec![fact_id(1)], "proposed only");

    apply(&service, &proposed.run).await;
    assert_eq!(
        ids(&service).await,
        vec![fact_id(2)],
        "the new fact is the active one"
    );
    assert_eq!(
        text_of(&service),
        file,
        "the superseded fact's file is untouched"
    );

    shell(&service, MemoryRequest::Revert(proposed.run)).await;
    assert_eq!(ids(&service).await, vec![fact_id(1)]);
}

#[tokio::test]
async fn a_hunk_whose_fact_has_gone_since_the_proposal_is_skipped() {
    let file = render(&ana("Ana is the CFO."));
    let service = reviewing(vec![supersede()]);
    put(&service, &file);
    let proposed = run(&service).await;

    // The person deletes the file the proposal was about; the old fact leaves the Space.
    service
        .backend()
        .vault_of(&work())
        .remove(&path())
        .expect("remove");
    let MemoryReply::Consolidation(done) = apply(&service, &proposed.run).await else {
        panic!("apply answers");
    };
    assert_eq!(done.state, RunState::Applied);
    assert_eq!(ids(&service).await, vec![], "nothing was brought back");
}

#[tokio::test]
async fn a_newer_run_replaces_an_unapplied_proposal_and_the_old_one_cannot_apply() {
    let before = render(&ana("Ana is the CFO."));
    let service = fake_service(ScriptedConsolidator::answering([
        Ok::<_, ConsolidateError>(Draft {
            hunks: vec![supersede()],
        }),
        Ok(Draft { hunks: vec![] }),
    ]));
    service.apply_settings(MemorySettings {
        apply: ConsolidateApply::Review,
        ..MemorySettings::default()
    });
    put(&service, &before);
    let first = run(&service).await;
    let second = run(&service).await;
    assert_ne!(first.run, second.run);
    assert!(matches!(
        apply(&service, &first.run).await,
        MemoryReply::Refused(Refusal::Invalid(_))
    ));
    assert_eq!(
        ids(&service).await,
        vec![fact_id(1)],
        "the dropped proposal changed nothing"
    );
}

#[tokio::test]
async fn auto_still_applies_at_once() {
    let (before, after) = (
        render(&ana("Ana is the CFO.")),
        render(&ana("Ana is the chief financial officer.")),
    );
    let service = fake_service(ScriptedConsolidator::answering([
        Ok::<_, ConsolidateError>(Draft {
            hunks: vec![tidy(&before, &after)],
        }),
    ]));
    put(&service, &before);
    assert_eq!(run(&service).await.state, RunState::Applied);
    assert_eq!(text_of(&service), after);
}
