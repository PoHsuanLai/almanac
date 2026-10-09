//! The portable path: an app hosts the service itself over `InProcess`, with no daemon and no
//! bus. Writes, recalls, forgets and seals against a Space. Runs with
//! `cargo test -p almanac-client --no-default-features --features in_process`, which links
//! neither zbus nor the Secret Service (`scripts/check-portable.sh` checks that).
#![cfg(feature = "in_process")]

use almanac_client::{InProcess, Memory, Recorded};
use almanac_core::*;
use almanac_fake::*;
use almanac_seal::{Aad, KeyStore, Nonce, ProvidedKeys, Purpose, SpaceKey, derive, seal, unseal};
use std::sync::Arc;

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

fn query(text: &str) -> RecallQuery {
    RecallQuery {
        space: work(),
        text: text.into(),
        limit: Count(10),
        over: RecallOver::Both,
    }
}

#[tokio::test]
async fn a_space_is_written_recalled_and_forgotten_in_process() {
    let service = Arc::new(fake_service(ScriptedConsolidator::default()));
    let app = Memory::over(InProcess::new(
        service.clone(),
        Caller::App(AppId {
            name: mail(),
            isolation: Isolation::InProcess,
        }),
    ));
    let reader = Memory::over(InProcess::new(service.clone(), Caller::Router));
    let shell = Memory::over(InProcess::new(service, Caller::ShellUi));

    assert!(matches!(
        app.record(mail_thread_archived().expect("fixture")).await,
        Ok(Recorded::Stored(_))
    ));
    let page = shell
        .timeline(
            work(),
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
        )
        .await
        .expect("timeline");
    assert_eq!(
        page.entries.len(),
        1,
        "the shell's timeline shows the app's one record"
    );
    let thing = thing("mail.thread", "7f3a").expect("thing");
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("topic"),
        text: FactText::parse("Ana sent the budget report.").expect("text"),
        links: vec![Link::Thing(thing.clone())],
        supersedes: vec![],
    };
    let (_, state) = shell.propose(work(), draft).await.expect("propose");
    assert_eq!(state, FactState::Active);

    let hits = reader.search(query("budget")).await.expect("search");
    assert_eq!(hits.len(), 2, "the event and the fact: {hits:?}");

    let plan = shell
        .plan_forget(work(), ForgetScope::Thing(thing))
        .await
        .expect("plan");
    let report = shell.forget(plan.token.clone()).await.expect("forget");
    assert_eq!(report.counts.index_docs, plan.index_docs);
    assert!(
        reader
            .search(query("budget"))
            .await
            .expect("search")
            .is_empty()
    );
}

#[tokio::test]
async fn a_provided_key_seals_a_space_file_with_no_key_service() {
    let keys = ProvidedKeys::new(SpaceKey::from_bytes([9; 32]));
    let key = keys.get(&work()).await.expect("key");
    let sub = derive(&key, &work(), Purpose::Files);
    let aad = Aad::file(&work(), "facts/people/ana.md");
    let sealed = seal(&sub, &aad, b"Ana sent the report.", Nonce([5; 24])).expect("seal");
    assert!(
        !sealed.windows(3).any(|w| w == b"Ana"),
        "no plaintext in the sealed file"
    );
    assert_eq!(
        unseal(&sub, &aad, &sealed).expect("unseal"),
        b"Ana sent the report."
    );
    // The same master on a later run opens it again.
    let again = ProvidedKeys::new(SpaceKey::from_bytes([9; 32]));
    let key = again.get(&work()).await.expect("key");
    let sub = derive(&key, &work(), Purpose::Files);
    assert!(unseal(&sub, &aad, &sealed).is_ok());
}
