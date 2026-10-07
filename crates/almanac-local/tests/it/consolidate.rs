//! Consolidation in review mode over the disk backend: propose, apply, revert, and a proposal
//! that survives a restart.

use crate::support::*;
use almanac_core::*;
use almanac_fake::{NOW, ScriptedConsolidator, trusted_label};
use almanac_service::{Backend, ConsolidateApply, ConsolidateError, Draft, MemorySettings};
use std::sync::Arc;

fn fact_id(n: u8) -> FactId {
    FactId::mint(u64::from(n), [n; 10])
}

fn supersede(old: FactId) -> Hunk {
    Hunk::Supersede {
        old: old.clone(),
        new: Fact {
            id: fact_id(2),
            text: FactText::parse("Ana is the CFO of Acme.").expect("text"),
            recorded: NOW,
            by: Actor::User {
                via: AppName::parse("org.quire.Shell").expect("app"),
            },
            label: trusted_label(),
            links: vec![Link::Fact(old.clone())],
            supersedes: vec![old],
            valid: Validity::Unstated,
        },
    }
}

fn reviewing(service: &Service) {
    service.apply_settings(MemorySettings {
        apply: ConsolidateApply::Review,
        ..MemorySettings::default()
    });
}

async fn ask(service: &Arc<Service>, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::ShellUi, request).await
}

async fn proposed(root: &std::path::Path) -> (Arc<Service>, FactId, DraftView) {
    let service = fresh(root, 7, ScriptedConsolidator::default());
    reviewing(&service);
    let (old, _) = shell(&service)
        .propose(work(), draft("people/ana", "Ana is the CFO."))
        .await
        .expect("propose");
    service
        .backend()
        .consolidator()
        .push(Ok::<_, ConsolidateError>(Draft {
            hunks: vec![supersede(old.clone())],
        }));
    let MemoryReply::Consolidation(view) =
        ask(&service, MemoryRequest::RunConsolidation(work())).await
    else {
        panic!("a proposed run");
    };
    (service, old, view)
}

async fn active_texts(service: &Arc<Service>) -> Vec<String> {
    let facts = shell(service).facts(all_facts()).await.expect("facts");
    facts
        .into_iter()
        .filter(|f| f.state == FactState::Active)
        .map(|f| f.fact.text.as_str().to_owned())
        .collect()
}

#[tokio::test]
async fn a_review_run_proposes_applies_and_reverts() {
    let dir = tempfile::tempdir().expect("dir");
    let (service, _, view) = proposed(dir.path()).await;
    assert_eq!(view.state, RunState::Proposed);
    assert_eq!(active_texts(&service).await, vec!["Ana is the CFO."]);

    let applied = ask(
        &service,
        MemoryRequest::ApplyConsolidation(view.run.clone()),
    )
    .await;
    assert!(matches!(applied, MemoryReply::Consolidation(ref v) if v.state == RunState::Applied));
    assert_eq!(
        active_texts(&service).await,
        vec!["Ana is the CFO of Acme."]
    );

    assert_eq!(
        ask(&service, MemoryRequest::Revert(view.run)).await,
        MemoryReply::Ok
    );
    assert_eq!(active_texts(&service).await, vec!["Ana is the CFO."]);
}

#[tokio::test]
async fn a_proposal_and_its_apply_survive_a_restart() {
    let dir = tempfile::tempdir().expect("dir");
    let (service, _, view) = proposed(dir.path()).await;
    drop(service);

    let service = service_again(dir.path());
    reviewing(&service);
    let applied = ask(
        &service,
        MemoryRequest::ApplyConsolidation(view.run.clone()),
    )
    .await;
    assert!(matches!(applied, MemoryReply::Consolidation(ref v) if v.state == RunState::Applied));
    drop(service);

    let service = service_again(dir.path());
    assert_eq!(
        active_texts(&service).await,
        vec!["Ana is the CFO of Acme."]
    );
}

fn service_again(root: &std::path::Path) -> Arc<Service> {
    service(root, 7, ScriptedConsolidator::default())
}
