//! A proposed consolidation run is a file (`consolidation/<run>.toml`): it survives a restart,
//! can be discarded or superseded, records its outcome instead of vanishing, and the applied
//! view lists what was applied and what was skipped. Scratch Space, scripted drafts, no clock.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::{
    ConsolidateApply, ConsolidateError, Draft, MemoryService, MemorySettings, ServiceEvent,
};
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

fn render_of(name: &str, n: u8, text: &str) -> String {
    render_topic(
        &TopicFile {
            topic: topic(name),
            title: name.to_owned(),
            blocks: vec![Block::Fact(fact(n, text, vec![]))],
        },
        &TimeZone::UTC,
    )
}

fn ana(text: &str) -> String {
    render_of("people/ana", 1, text)
}

fn put(service: &Service, name: &str, text: &str) {
    service
        .backend()
        .vault_of(&work())
        .write_atomic(&VaultPath::topic(&topic(name)), text.as_bytes())
        .expect("write");
}

fn text_of(service: &Service, name: &str) -> String {
    let bytes = service
        .backend()
        .vault_of(&work())
        .read(&VaultPath::topic(&topic(name)))
        .expect("file");
    String::from_utf8(bytes).expect("utf8")
}

fn drafts(hunks: Vec<Vec<Hunk>>) -> ScriptedConsolidator {
    ScriptedConsolidator::answering(
        hunks
            .into_iter()
            .map(|hunks| Ok::<_, ConsolidateError>(Draft { hunks })),
    )
}

fn reviewing(hunks: Vec<Vec<Hunk>>) -> Service {
    let service = fake_service(drafts(hunks));
    service.apply_settings(MemorySettings {
        apply: ConsolidateApply::Review,
        ..MemorySettings::default()
    });
    service
}

/// The same machine after a restart: same files, a new service with nothing in memory.
fn restarted(service: &Service) -> Service {
    let again = MemoryService::new(
        service.backend().restarted(drafts(vec![])),
        RuleSet::standard(),
    );
    // The daemon reads `spaces.toml` at start and registers what it finds.
    for meta in service.metas() {
        again.register(meta);
    }
    again
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

async fn read_view(service: &Service) -> DraftView {
    match shell(service, MemoryRequest::Consolidation(work())).await {
        MemoryReply::Consolidation(view) => view,
        other => panic!("{other:?}"),
    }
}

async fn apply(service: &Service, run: &RunId) -> MemoryReply {
    shell(service, MemoryRequest::ApplyConsolidation(run.clone())).await
}

async fn discard(service: &Service, run: &RunId) -> MemoryReply {
    shell(service, MemoryRequest::DiscardConsolidation(run.clone())).await
}

fn invalid(reply: &MemoryReply) -> bool {
    matches!(reply, MemoryReply::Refused(Refusal::Invalid(_)))
}

fn file_of(service: &Service, run: &RunId) -> String {
    let bytes = service
        .backend()
        .vault_of(&work())
        .read(&VaultPath::parse(&format!("consolidation/{run}.toml")).expect("path"))
        .expect("the run file");
    String::from_utf8(bytes).expect("utf8")
}

fn state_in(text: &str) -> String {
    let table: toml::Table = text.parse().expect("toml");
    table["state"].as_str().expect("state").to_owned()
}

fn tidy(name: &str, before: &str, after: &str) -> Hunk {
    Hunk::Tidy(TidyHunk {
        topic: topic(name),
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

/// A service with a tidy of `people/ana` proposed; its before text and after text.
async fn proposed_tidy() -> (Service, DraftView, String, String) {
    let (before, after) = (
        ana("Ana is the CFO."),
        ana("Ana is the chief financial officer."),
    );
    let service = reviewing(vec![vec![tidy("people/ana", &before, &after)]]);
    put(&service, "people/ana", &before);
    let view = run(&service).await;
    (service, view, before, after)
}

#[tokio::test]
async fn a_proposal_is_written_as_its_file_in_the_pinned_form() {
    let (service, view, ..) = proposed_tidy().await;
    let text = file_of(&service, &view.run).replace(view.run.as_str(), "RUN");
    assert_eq!(text, include_str!("golden/run_proposed.toml"));
}

#[tokio::test]
async fn a_proposal_survives_a_restart_and_applies() {
    let (service, view, _, after) = proposed_tidy().await;
    let service = restarted(&service);

    // The first request after the restart is the apply: no Space is open yet.
    let MemoryReply::Consolidation(done) = apply(&service, &view.run).await else {
        panic!("apply answers with the applied view");
    };
    assert_eq!(done.state, RunState::Applied);
    assert_eq!(done.hunks, view.hunks);
    assert_eq!(text_of(&service, "people/ana"), after);
    assert_eq!(state_in(&file_of(&service, &view.run)), "applied");
}

#[tokio::test]
async fn a_restarted_service_reads_the_proposal_back() {
    let (service, view, ..) = proposed_tidy().await;
    let service = restarted(&service);
    assert_eq!(read_view(&service).await, view);
}

#[tokio::test]
async fn a_restart_applies_the_hunks_only_if_they_still_stand() {
    let (service, view, before, _) = proposed_tidy().await;
    put(&service, "people/ana", &ana("Ana is CFO now."));
    let service = restarted(&service);
    let reply = apply(&service, &view.run).await;
    let MemoryReply::Consolidation(done) = reply else {
        panic!("apply answers: {reply:?}");
    };
    assert!(done.hunks.is_empty());
    assert_eq!(
        done.skipped,
        vec![SkippedHunk {
            hunk: view.hunks[0].clone(),
            reason: SkipReason::FileChanged
        }]
    );
    assert_ne!(text_of(&service, "people/ana"), before);
}

#[tokio::test]
async fn a_discarded_proposal_stays_on_disk_and_cannot_be_applied() {
    let (service, view, before, _) = proposed_tidy().await;
    let MemoryReply::Consolidation(gone) = discard(&service, &view.run).await else {
        panic!("discard answers with the view");
    };
    assert_eq!(gone.state, RunState::Discarded);
    assert_eq!(gone.hunks, view.hunks, "the hunks are kept for the record");
    assert_eq!(state_in(&file_of(&service, &view.run)), "discarded");
    assert!(invalid(&apply(&service, &view.run).await));
    assert!(invalid(&discard(&service, &view.run).await), "not twice");
    assert_eq!(text_of(&service, "people/ana"), before);
    assert_eq!(read_view(&service).await.state, RunState::Discarded);

    // A restart does not bring a discarded proposal back.
    let service = restarted(&service);
    assert!(invalid(&apply(&service, &view.run).await));
    assert_eq!(state_in(&file_of(&service, &view.run)), "discarded");
}

#[tokio::test]
async fn discard_is_refused_for_every_state_but_proposed_and_for_everyone_but_the_shell() {
    let (service, view, ..) = proposed_tidy().await;
    let unknown = RunId::parse("c-nothing").expect("run");
    assert!(invalid(&discard(&service, &unknown).await));
    let reply = service
        .handle(
            &Caller::Router,
            MemoryRequest::DiscardConsolidation(view.run.clone()),
        )
        .await;
    assert!(matches!(reply, MemoryReply::Refused(_)));
    assert_eq!(state_in(&file_of(&service, &view.run)), "proposed");

    apply(&service, &view.run).await;
    assert!(invalid(&discard(&service, &view.run).await), "applied");
    assert_eq!(state_in(&file_of(&service, &view.run)), "applied");
    shell(&service, MemoryRequest::Revert(view.run.clone())).await;
    assert!(invalid(&discard(&service, &view.run).await), "reverted");
    assert_eq!(state_in(&file_of(&service, &view.run)), "reverted");
}

#[tokio::test]
async fn a_newer_run_marks_the_older_proposal_superseded() {
    let (before, after) = (
        ana("Ana is the CFO."),
        ana("Ana is the chief financial officer."),
    );
    let tidy = tidy("people/ana", &before, &after);
    let service = reviewing(vec![vec![tidy.clone()], vec![tidy]]);
    put(&service, "people/ana", &before);
    let first = run(&service).await;
    let second = run(&service).await;
    assert_ne!(first.run, second.run);

    assert_eq!(state_in(&file_of(&service, &first.run)), "superseded");
    assert_eq!(state_in(&file_of(&service, &second.run)), "proposed");
    assert!(invalid(&apply(&service, &first.run).await));
    assert!(invalid(&discard(&service, &first.run).await));
    assert_eq!(text_of(&service, "people/ana"), before);
    assert!(matches!(
        apply(&service, &second.run).await,
        MemoryReply::Consolidation(_)
    ));
    assert_eq!(text_of(&service, "people/ana"), after);
}

#[tokio::test]
async fn two_proposals_left_by_a_crash_resolve_to_the_newer_at_start() {
    let (service, view, ..) = proposed_tidy().await;
    let text = file_of(&service, &view.run);
    let newer = RunId::parse("c-newer").expect("run");
    let at = text
        .lines()
        .find(|l| l.starts_with("drafted = "))
        .expect("drafted");
    let later = format!("drafted = {}", NOW.0 + 60);
    let copy = text
        .replace(view.run.as_str(), newer.as_str())
        .replace(at, &later);
    service
        .backend()
        .vault_of(&work())
        .write_atomic(
            &VaultPath::parse("consolidation/c-newer.toml").expect("path"),
            copy.as_bytes(),
        )
        .expect("write");

    let service = restarted(&service);
    assert_eq!(read_view(&service).await.run, newer);
    assert_eq!(state_in(&file_of(&service, &view.run)), "superseded");
    assert_eq!(state_in(&file_of(&service, &newer)), "proposed");
}

#[tokio::test]
async fn a_torn_write_is_ignored_when_proposals_load() {
    let (service, view, ..) = proposed_tidy().await;
    let text = file_of(&service, &view.run);
    let torn = &text.as_bytes()[..text.len() / 2];
    let vault = service.backend().vault_of(&work());
    for name in [
        "consolidation/.almanac-tmp-c-torn",
        "consolidation/c-torn.toml",
        "consolidation/c-junk.toml",
    ] {
        let bytes = if name.ends_with("junk.toml") {
            b"\xff\xfe not toml"
        } else {
            torn
        };
        vault
            .write_atomic(&VaultPath::parse(name).expect("path"), bytes)
            .expect("write");
    }

    let service = restarted(&service);
    assert_eq!(
        read_view(&service).await,
        view,
        "the whole proposal is found, the torn files are not"
    );
    assert!(matches!(
        apply(&service, &view.run).await,
        MemoryReply::Consolidation(_)
    ));
}

#[tokio::test]
async fn the_applied_view_lists_what_was_applied_and_what_was_skipped() {
    let ben_before = render_of("people/ben", 3, "Ben is in Lisbon.");
    let ben_after = render_of("people/ben", 3, "Ben lives in Lisbon.");
    let tidy = tidy("people/ben", &ben_before, &ben_after);
    let service = reviewing(vec![vec![tidy.clone(), supersede()]]);
    put(&service, "people/ben", &ben_before);
    put(&service, "people/ana", &ana("Ana is the CFO."));
    let proposed = run(&service).await;
    assert_eq!(proposed.hunks.len(), 2);

    // The fact the supersede replaces goes before the person applies.
    service
        .backend()
        .vault_of(&work())
        .remove(&VaultPath::topic(&topic("people/ana")))
        .expect("remove");
    let MemoryReply::Consolidation(done) = apply(&service, &proposed.run).await else {
        panic!("apply answers");
    };
    assert_eq!(done.hunks, vec![tidy]);
    assert_eq!(
        done.skipped,
        vec![SkippedHunk {
            hunk: supersede(),
            reason: SkipReason::ReplacedFactGone
        }]
    );
    assert_eq!(read_view(&service).await, done);
    let recorded: toml::Table = file_of(&service, &proposed.run).parse().expect("toml");
    assert_eq!(recorded["hunks"].as_array().map(Vec::len), Some(1));
    assert_eq!(recorded["skipped"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn applying_and_discarding_tell_the_bus() {
    let (service, view, ..) = proposed_tidy().await;
    service.take_events();
    apply(&service, &view.run).await;
    assert!(
        service
            .take_events()
            .contains(&ServiceEvent::ConsolidationChanged(
                work(),
                view.run.clone()
            ))
    );

    let (service, view, ..) = proposed_tidy().await;
    service.take_events();
    discard(&service, &view.run).await;
    assert!(
        service
            .take_events()
            .contains(&ServiceEvent::ConsolidationChanged(work(), view.run))
    );
}

#[tokio::test]
async fn forgetting_a_fact_takes_its_text_out_of_the_run_files_and_counts_it() {
    let service = reviewing(vec![vec![supersede()]]);
    put(&service, "people/ana", &ana("Ana is the CFO."));
    let proposed = run(&service).await;
    apply(&service, &proposed.run).await;
    assert!(file_of(&service, &proposed.run).contains("Acme"));

    let plan = shell(
        &service,
        MemoryRequest::PlanForget(work(), ForgetScope::Fact(fact_id(2))),
    )
    .await;
    let MemoryReply::Plan(plan) = plan else {
        panic!("{plan:?}")
    };
    assert!(matches!(
        shell(&service, MemoryRequest::Forget(plan.token)).await,
        MemoryReply::Forgot(_)
    ));
    let text = file_of(&service, &proposed.run);
    assert!(!text.contains("Acme"), "{text}");
    let table: toml::Table = text.parse().expect("toml");
    assert_eq!(table["erased"].as_integer(), Some(1));
    assert_eq!(table["state"].as_str(), Some("applied"));
}
