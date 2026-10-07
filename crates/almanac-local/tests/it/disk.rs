//! A Space on disk through `Memory::over(InProcess)` with almanac-local's backend: every
//! operation, then a restart over the same root and master key.

use crate::support::*;
use almanac_client::{ClientError, Recorded};
use almanac_core::*;
use almanac_fake::{
    ScriptedConsolidator, mail_thread_archived, session_entry, session_thing, thing,
};
use almanac_local::{LocalError, create_space};

fn nothing() -> ScriptedConsolidator {
    ScriptedConsolidator::default()
}

#[tokio::test]
async fn record_propose_and_search_through_the_in_process_client() {
    let dir = tempfile::tempdir().expect("dir");
    let service = fresh(dir.path(), 7, nothing());
    let app = client(
        &service,
        Caller::App(AppId {
            name: AppName::parse("org.quire.Mail").expect("app"),
            isolation: Isolation::InProcess,
        }),
    );
    let shell = shell(&service);
    assert!(matches!(
        app.record(mail_thread_archived().expect("fixture")).await,
        Ok(Recorded::Stored(_))
    ));
    let (_, state) = shell
        .propose(work(), draft("people/ana", "Ana sent the budget report."))
        .await
        .expect("propose");
    assert_eq!(state, FactState::Active);
    let hits = shell.search(query("budget")).await.expect("search");
    assert_eq!(hits.len(), 2, "the event and the fact: {hits:?}");
}

#[tokio::test]
async fn everything_is_there_after_a_restart_and_forget_persists_too() {
    let dir = tempfile::tempdir().expect("dir");
    let service = fresh(dir.path(), 7, nothing());
    let shell1 = shell(&service);
    client(&service, Caller::Router)
        .record(mail_thread_archived().expect("fixture"))
        .await
        .expect("record");
    shell1
        .propose(work(), draft("people/ana", "Ana sent the budget report."))
        .await
        .expect("propose");
    drop(shell1);
    drop(service);

    let service = service_again(dir.path());
    let shell2 = shell(&service);
    let spaces = shell2.spaces().await.expect("spaces");
    assert_eq!(spaces.len(), 1, "spaces.toml was read: {spaces:?}");
    assert_eq!(shell2.search(query("budget")).await.expect("s").len(), 2);
    let facts = shell2.facts(all_facts()).await.expect("facts");
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].topic, topic("people/ana"));

    let thing = thing("mail.thread", "7f3a").expect("thing");
    let plan = shell2
        .plan_forget(work(), ForgetScope::Thing(thing))
        .await
        .expect("plan");
    shell2.forget(plan.token).await.expect("forget");
    drop(shell2);
    drop(service);

    let service = service_again(dir.path());
    let hits = shell(&service).search(query("budget")).await.expect("s");
    assert_eq!(
        hits.len(),
        1,
        "only the fact is left; the forgotten event stays gone after a restart: {hits:?}"
    );
}

fn service_again(root: &std::path::Path) -> std::sync::Arc<Service> {
    service(root, 7, nothing())
}

#[tokio::test]
async fn a_wrong_master_key_is_refused_as_locked_not_garbled() {
    let dir = tempfile::tempdir().expect("dir");
    let service = fresh(dir.path(), 7, nothing());
    shell(&service)
        .propose(work(), draft("people/ana", "Ana sent the budget report."))
        .await
        .expect("propose");
    drop(service);

    let wrong = crate::support::service(dir.path(), 8, nothing());
    for refused in [
        shell(&wrong).search(query("budget")).await.map(drop),
        shell(&wrong).facts(all_facts()).await.map(drop),
    ] {
        assert!(
            matches!(&refused, Err(ClientError::Refused(Refusal::SpaceLocked))),
            "{refused:?}"
        );
    }

    let right = service_again(dir.path());
    assert_eq!(shell(&right).facts(all_facts()).await.expect("f").len(), 1);
}

#[tokio::test]
async fn a_space_is_created_once_and_listed_after_a_restart() {
    let dir = tempfile::tempdir().expect("dir");
    let service = fresh(dir.path(), 7, nothing());
    assert_eq!(
        create_space(&service, work(), VaultKind::Sealed),
        Err(LocalError::Exists)
    );
    let other = SpaceId::parse("home").expect("space");
    create_space(&service, other, VaultKind::Plain).expect("second");
    drop(service);
    let service = service_again(dir.path());
    assert_eq!(service.metas().len(), 2);
}

#[tokio::test]
async fn recent_with_bodies_returns_an_area_payload_in_the_owners_form() {
    let dir = tempfile::tempdir().expect("dir");
    let service = fresh(dir.path(), 7, nothing());
    let router = client(&service, Caller::Router);
    router
        .record(almanac_fake::policy_ask().expect("fixture"))
        .await
        .expect("record");
    let entries = router
        .recent(
            work(),
            RecentQuery {
                since: UnixSeconds(0),
                kinds: vec![],
                trust: TrustFilter::Any,
                limit: Count(10),
                bodies: BodyMode::Json,
            },
        )
        .await
        .expect("recent");
    assert_eq!(entries.len(), 1);
    let body = entries[0].body.as_ref().expect("body");
    assert_eq!(body.as_str(), r#"{"ruling":"ask","rule":"untrusted-sink"}"#);
}

#[tokio::test]
async fn an_acked_durable_append_survives_a_reopen_and_pages_back_in_order() {
    let dir = tempfile::tempdir().expect("dir");
    let service = fresh(dir.path(), 7, nothing());
    let router = client(&service, Caller::Router);
    let mut acked = Vec::new();
    for slug in ["opened", "turn", "taint"] {
        let record = session_entry("s-1", slug, "kept verbatim").expect("fixture");
        acked.push(router.record_durable(record).await.expect("ack").event.seq);
        router
            .record(mail_thread_archived().expect("fixture"))
            .await
            .expect("record");
    }
    assert!(acked.windows(2).all(|w| w[0] < w[1]), "{acked:?}");
    drop(router);
    drop(service);

    let service = service_again(dir.path());
    let router = client(&service, Caller::Router);
    let query = |after| EntriesQuery {
        kinds: vec![KindPattern::parse("companion.session.*").expect("pattern")],
        about: session_thing("s-1"),
        after,
        limit: Count(2),
        bodies: BodyMode::Json,
    };
    let first = router.entries(work(), query(None)).await.expect("page");
    let second = router
        .entries(work(), query(first.next))
        .await
        .expect("page");
    let seqs: Vec<Seq> = first
        .entries
        .iter()
        .chain(&second.entries)
        .map(|e| e.summary.event.seq)
        .collect();
    assert_eq!(seqs, acked, "every acked entry is there after the restart");
    assert_eq!(second.next, None);
    let body = second.entries[0].body.as_ref().expect("body");
    assert!(body.as_str().contains("kept verbatim"));
}
