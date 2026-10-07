//! memoryd's inferd link over a private session bus: with a fake `org.quire.Inference1` serving
//! (documents of each data class embedded in their own session, a consolidation drafted), and
//! with none (the degraded path: lexical-only recall, consolidation off). Nothing here reaches
//! the real session bus, the real inferd or the real Secret Service.

use crate::support::inferd::{FakeInferd, Seen};
use crate::support::{SharedKeys, bus::PrivateBus, connect, dirs_in, space};
use almanac_core::*;
use almanac_fake::{mail_label, mail_thread_archived};
use almanac_service::{ConsolidateError, ConsolidationInput, Consolidator, MemoryService};
use memoryd::{InferdConsolidator, InferdEmbedder, SystemBackend, inferd_link};
use recall::{ClassTag, Classed, EmbedError, EmbedRole, Embedder, FakeEmbedder, Urgency};
use std::sync::{Arc, Mutex};

type LiveBackend = SystemBackend<
    SharedKeys,
    InferdEmbedder<porter_client::AnyTransport>,
    InferdConsolidator<porter_client::AnyTransport>,
>;

struct Rig {
    _scratch: tempfile::TempDir,
    _bus: PrivateBus,
    _inferd: Option<zbus::Connection>,
    seen: Arc<Mutex<Seen>>,
    link: Arc<porter_client::AnyTransport>,
    service: MemoryService<LiveBackend>,
}

/// A bus with a fake inferd on it when `draft` is given, and a memoryd service over the link
/// `inferd_link` builds from a connection to it (as `main` does).
async fn rig(draft: Option<&str>) -> Rig {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let (seen, daemon) = match draft {
        Some(draft) => {
            let (fake, seen) = FakeInferd::new(draft);
            let daemon = connect(&bus.address).await;
            fake.serve(&daemon).await;
            (seen, Some(daemon))
        }
        None => (Arc::default(), None),
    };
    let memoryd = connect(&bus.address).await;
    let link = inferd_link(&memoryd);
    let card = FakeEmbedder::new().card().clone();
    let backend = SystemBackend::with(
        dirs_in(&scratch.path().join("home")),
        SharedKeys::default(),
        InferdEmbedder::new(link.clone(), card),
        InferdConsolidator::new(link.clone()),
    );
    Rig {
        service: MemoryService::new(backend, RuleSet::standard()),
        _scratch: scratch,
        _bus: bus,
        _inferd: daemon,
        seen,
        link,
    }
}

fn mail_event(title: &str, key: &str) -> Record {
    let mut record = mail_thread_archived().expect("fixture");
    record.label = mail_label();
    if let EventBody::Thing { thing, .. } = &mut record.body {
        thing.title = title.into();
        thing.thing.key = ThingKey::parse(key).expect("key");
    }
    record
}

fn search(text: &str) -> MemoryRequest {
    MemoryRequest::Search(RecallQuery {
        space: space("work"),
        text: text.into(),
        limit: Count(10),
        over: RecallOver::Both,
    })
}

async fn record_and_find(rig: &Rig) -> Vec<MemoryItem> {
    let recorded = rig
        .service
        .handle(
            &Caller::Router,
            MemoryRequest::Record(mail_event("Lisbon receipts", "7f3a")),
        )
        .await;
    assert!(matches!(recorded, MemoryReply::Recorded(_)), "{recorded:?}");
    let reply = rig
        .service
        .handle(&Caller::Router, search("receipts"))
        .await;
    let MemoryReply::Hits(hits) = reply else {
        panic!("{reply:?}")
    };
    hits.into_iter().map(|h| h.doc).collect()
}

fn input() -> ConsolidationInput {
    ConsolidationInput {
        space: space("work"),
        run: RunId::parse("c-0123456789abcdef0123").expect("run"),
        now: UnixSeconds(1_790_000_000),
        facts: vec![],
        events: vec![],
        topics: vec![],
    }
}

#[tokio::test]
async fn with_inferd_up_the_documents_are_embedded_each_in_a_session_of_its_class() {
    let rig = rig(Some(r#"{"hunks":[]}"#)).await;
    let items = record_and_find(&rig).await;
    assert_eq!(items.len(), 1, "{items:?}");

    // The event's text carries the mail class: its session is a mail session, over the bus.
    let seen = rig.seen.lock().expect("lock").clone();
    assert!(
        seen.opens.iter().all(|c| c == "mail"),
        "the document, the query and the pin are all mail: {seen:?}"
    );
    let documents: Vec<_> = seen
        .embeds
        .iter()
        .flat_map(|(class, inputs)| inputs.iter().map(move |t| (class.as_str(), t.as_str())))
        .collect();
    assert!(
        documents
            .iter()
            .any(|(c, t)| *c == "mail" && t.contains("Lisbon receipts")),
        "{documents:?}"
    );
    assert!(
        documents
            .iter()
            .any(|(c, t)| *c == "mail" && *t == "receipts"),
        "the query goes as the pin: {documents:?}"
    );
}

#[tokio::test]
async fn with_inferd_up_consolidation_drafts_over_the_bus() {
    let rig = rig(Some(r#"{"hunks":[]}"#)).await;
    let consolidator = InferdConsolidator::new(rig.link.clone());
    let draft = consolidator.draft(input()).await.expect("a draft");
    assert!(draft.hunks.is_empty());
    let seen = rig.seen.lock().expect("lock").clone();
    // Nothing is classed in an empty input: the app's own data. (A mixed input takes the most
    // sensitive class present: `the_class_of_a_request_is_the_most_sensitive_of_its_labels`.)
    assert_eq!(seen.tasks, vec!["app_own".to_owned()]);
}

#[tokio::test]
async fn a_batch_of_mixed_classes_opens_one_session_per_class_over_the_bus() {
    let rig = rig(Some("{}")).await;
    let embedder = InferdEmbedder::new(rig.link.clone(), FakeEmbedder::new().card().clone());
    let classed = |class: &str, text: &str| Classed {
        class: ClassTag(class.to_owned()),
        text: text.to_owned(),
    };
    let texts = vec![
        classed("notes", "n1"),
        classed("mail", "m1"),
        classed("notes", "n2"),
        classed("", "untagged"),
        classed("from_the_future", "unknown"),
    ];
    let got = embedder
        .embed_classed(&texts, EmbedRole::Document, Urgency::Background)
        .await
        .expect("vectors");
    let want: Vec<_> = texts
        .iter()
        .map(|t| FakeEmbedder::vector(&t.text))
        .collect();
    assert_eq!(got, want, "vectors come back in input order");
    let seen = rig.seen.lock().expect("lock").clone();
    // Classes in their own order; the untagged and the unknown ride with the pin (mail).
    assert_eq!(
        seen.embeds,
        vec![
            (
                "mail".to_owned(),
                vec!["m1".to_owned(), "untagged".to_owned(), "unknown".to_owned()]
            ),
            ("notes".to_owned(), vec!["n1".to_owned(), "n2".to_owned()]),
        ]
    );
    assert_eq!(seen.opens, vec!["mail".to_owned(), "notes".to_owned()]);
}

#[tokio::test]
async fn with_inferd_down_recall_is_lexical_only_and_consolidation_is_off() {
    let rig = rig(None).await;
    // The record succeeds and the search still finds it by its words.
    let items = record_and_find(&rig).await;
    assert_eq!(items.len(), 1, "{items:?}");

    // The embedder says so, and the consolidator too.
    let embedder = InferdEmbedder::new(rig.link.clone(), FakeEmbedder::new().card().clone());
    assert_eq!(
        embedder
            .embed(&["x".to_owned()], EmbedRole::Query, Urgency::Interactive)
            .await,
        Err(EmbedError::Unavailable)
    );
    let consolidator = InferdConsolidator::new(rig.link.clone());
    assert_eq!(
        consolidator.draft(input()).await.err(),
        Some(ConsolidateError::Unavailable)
    );
    // Through the service, a run is refused as busy (retried next night), not a crash.
    let run = rig
        .service
        .handle(
            &Caller::ShellUi,
            MemoryRequest::RunConsolidation(space("work")),
        )
        .await;
    assert_eq!(run, MemoryReply::Refused(Refusal::Busy));
}

#[tokio::test]
async fn inferd_that_starts_later_is_found_at_the_next_call() {
    let rig = rig(None).await;
    let embedder = InferdEmbedder::new(rig.link.clone(), FakeEmbedder::new().card().clone());
    let texts = ["x".to_owned()];
    let ask = || embedder.embed(&texts, EmbedRole::Query, Urgency::Interactive);
    assert_eq!(ask().await, Err(EmbedError::Unavailable));

    let (fake, _seen) = FakeInferd::new("{}");
    let daemon = connect(&rig._bus.address).await;
    fake.serve(&daemon).await;
    assert_eq!(ask().await, Ok(vec![FakeEmbedder::vector("x")]));
}
