//! A companion session's durable log: paged by session in append order, acknowledged on append,
//! and kept out of recall.

use almanac_core::*;
use almanac_fake::*;
use almanac_service::{Backend, MemoryService};
use std::io::Read;

fn service() -> MemoryService<FakeBackend> {
    fake_service(ScriptedConsolidator::default())
}

fn work() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

async fn ask(service: &MemoryService<FakeBackend>, request: MemoryRequest) -> MemoryReply {
    service.handle(&Caller::Router, request).await
}

async fn append(
    service: &MemoryService<FakeBackend>,
    session: &str,
    slug: &str,
    text: &str,
) -> Ack {
    let record = session_entry(session, slug, text).expect("fixture");
    match ask(service, MemoryRequest::RecordDurable(record)).await {
        MemoryReply::Durable(ack) => ack,
        other => panic!("{other:?}"),
    }
}

fn session_query(session: &str, after: Option<Cursor>, limit: u32) -> MemoryRequest {
    MemoryRequest::Entries(
        work(),
        EntriesQuery {
            kinds: vec![KindPattern::parse("companion.session.*").expect("pattern")],
            about: session_thing(session),
            after,
            limit: Count(limit),
            bodies: BodyMode::Json,
        },
    )
}

async fn page(service: &MemoryService<FakeBackend>, request: MemoryRequest) -> EntriesPage {
    match ask(service, request).await {
        MemoryReply::Entries(page) => page,
        other => panic!("{other:?}"),
    }
}

fn slugs(page: &EntriesPage) -> Vec<String> {
    page.entries
        .iter()
        .map(|e| {
            e.summary
                .kind
                .as_str()
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_owned()
        })
        .collect()
}

#[tokio::test]
async fn one_sessions_entries_page_in_order_across_interleaved_events() {
    let service = service();
    let mut s1 = Vec::new();
    for (i, (session, slug)) in [
        ("s-1", "opened"),
        ("s-2", "opened"),
        ("s-1", "turn"),
        ("s-2", "turn"),
        ("s-1", "taint"),
        ("s-1", "words"),
        ("s-2", "words"),
        ("s-1", "closed"),
    ]
    .into_iter()
    .enumerate()
    {
        if i % 3 == 0 {
            let mail = mail_thread_archived().expect("fixture");
            ask(&service, MemoryRequest::Record(mail)).await;
        }
        let ack = append(&service, session, slug, &format!("entry {i}")).await;
        if session == "s-1" {
            s1.push((slug, ack.event.seq));
        }
    }

    let mut seen: Vec<(String, Seq)> = Vec::new();
    let mut after = None;
    let mut sizes = Vec::new();
    loop {
        let p = page(&service, session_query("s-1", after, 2)).await;
        sizes.push(p.entries.len());
        seen.extend(
            p.entries
                .iter()
                .map(|e| (e.summary.kind.as_str().to_owned(), e.summary.event.seq)),
        );
        match p.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    assert_eq!(sizes, vec![2, 2, 1], "five entries in pages of two");
    let kinds: Vec<&str> = seen.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "companion.session.opened",
            "companion.session.turn",
            "companion.session.taint",
            "companion.session.words",
            "companion.session.closed"
        ]
    );
    let seqs: Vec<Seq> = seen.iter().map(|(_, s)| *s).collect();
    assert_eq!(seqs, s1.iter().map(|(_, s)| *s).collect::<Vec<_>>());
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "append order");
}

#[tokio::test]
async fn a_cursor_resumes_after_new_entries_and_bodies_come_back_as_json() {
    let service = service();
    append(&service, "s-1", "opened", "first").await;
    let first = page(&service, session_query("s-1", None, 10)).await;
    assert_eq!(first.next, None, "nothing more yet");
    let last = first.entries.last().expect("entry").summary.event.seq;
    let body = first.entries[0].body.as_ref().expect("json body");
    assert_eq!(body.as_str(), r#"{"session":"s-1","text":"first"}"#);

    append(&service, "s-2", "opened", "other session").await;
    append(&service, "s-1", "turn", "second").await;
    let more = page(&service, session_query("s-1", Some(Cursor(last)), 10)).await;
    assert_eq!(slugs(&more), ["turn"]);
}

#[tokio::test]
async fn without_a_session_the_kind_prefix_alone_pages_every_session_in_order() {
    let service = service();
    for (session, slug) in [("s-1", "opened"), ("s-2", "opened"), ("s-1", "turn")] {
        append(&service, session, slug, "x").await;
    }
    let request = MemoryRequest::Entries(
        work(),
        EntriesQuery {
            kinds: vec![KindPattern::parse("companion.session.*").expect("pattern")],
            about: None,
            after: None,
            limit: Count(2),
            bodies: BodyMode::Without,
        },
    );
    let p = page(&service, request).await;
    assert_eq!(slugs(&p), ["opened", "opened"]);
    assert!(p.next.is_some());
    assert!(p.entries.iter().all(|e| e.body.is_none()));
}

#[tokio::test]
async fn the_ack_sequence_is_monotonic_and_names_the_stored_event() {
    let service = service();
    let mut seqs = Vec::new();
    for slug in ["opened", "turn", "taint"] {
        seqs.push(append(&service, "s-1", slug, "x").await.event.seq);
        // Memory's own audit events and other writers take numbers in between.
        ask(
            &service,
            MemoryRequest::Record(mail_thread_archived().expect("fixture")),
        )
        .await;
    }
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
    let p = page(&service, session_query("s-1", None, 10)).await;
    let stored: Vec<Seq> = p.entries.iter().map(|e| e.summary.event.seq).collect();
    assert_eq!(stored, seqs, "the ack is the event's position in the log");
}

#[tokio::test]
async fn a_paused_space_refuses_a_durable_append_instead_of_dropping_it() {
    let service = service();
    let paused = service
        .handle(
            &Caller::ShellUi,
            MemoryRequest::Pause(work(), UnixSeconds(NOW.0 + 3600)),
        )
        .await;
    assert_eq!(paused, MemoryReply::Ok);
    let record = session_entry("s-1", "taint", "x").expect("fixture");
    assert_eq!(
        ask(&service, MemoryRequest::RecordDurable(record)).await,
        MemoryReply::Refused(Refusal::NotKept(DropReason::Paused))
    );
}

fn consolidation_events(service: &MemoryService<FakeBackend>) -> Vec<String> {
    service
        .backend()
        .consolidator()
        .inputs()
        .iter()
        .flat_map(|i| i.events.iter().map(|e| e.kind.as_str().to_owned()))
        .collect()
}

#[tokio::test]
async fn session_entries_never_reach_recall_but_recent_entries_and_export_return_them() {
    let service = service();
    ask(
        &service,
        MemoryRequest::Record(mail_thread_archived().expect("fixture")),
    )
    .await;
    let session = append(&service, "s-1", "words", "the Q4 budget is secret").await;
    let from_session = |hits: &[RecallHit]| {
        hits.iter().any(|h| {
            matches!(&h.doc, MemoryItem::Event(e) if e.seq == session.event.seq)
                || h.links
                    .iter()
                    .any(|l| matches!(l, Link::Event(e) if e.seq == session.event.seq))
                || h.text.as_str().contains("secret")
        })
    };

    // Search and Inject.
    let search = MemoryRequest::Search(RecallQuery {
        space: work(),
        text: "secret".into(),
        limit: Count(10),
        over: RecallOver::Both,
    });
    let MemoryReply::Hits(hits) = ask(&service, search).await else {
        panic!("hits")
    };
    assert!(!from_session(&hits), "{hits:?}");
    let inject = MemoryRequest::Inject(InjectQuery {
        space: work(),
        text: "the Q4 budget is secret".into(),
        budget: Tokens(1500),
        k: Count(8),
        over: RecallOver::Both,
        trust: TrustFilter::Any,
    });
    let MemoryReply::Hits(hits) = ask(&service, inject).await else {
        panic!("hits")
    };
    assert!(!from_session(&hits), "{hits:?}");

    // Related: the session names its thing, yet is not "related" in the recall sense.
    let related = ask(
        &service,
        MemoryRequest::Related(work(), session_thing("s-1").expect("thing")),
    )
    .await;
    assert_eq!(related, MemoryReply::Related(vec![]));

    // Consolidation sees the mail event and not the session entry.
    let run = service
        .handle(&Caller::ShellUi, MemoryRequest::RunConsolidation(work()))
        .await;
    assert!(matches!(run, MemoryReply::Consolidation(_)), "{run:?}");
    let kinds = consolidation_events(&service);
    assert!(kinds.iter().any(|k| k == "thing.archived"), "{kinds:?}");
    assert!(
        kinds.iter().all(|k| !k.starts_with("companion.session")),
        "{kinds:?}"
    );

    // Recent, Entries and export still return it.
    let recent = ask(
        &service,
        MemoryRequest::Recent(
            work(),
            RecentQuery {
                since: UnixSeconds(0),
                kinds: vec![KindPattern::parse("companion.session.*").expect("pattern")],
                trust: TrustFilter::Any,
                limit: Count(10),
                bodies: BodyMode::Json,
            },
        ),
    )
    .await;
    let MemoryReply::Recent(recent) = recent else {
        panic!("recent")
    };
    assert_eq!(recent.len(), 1);
    assert_eq!(
        page(&service, session_query("s-1", None, 10))
            .await
            .entries
            .len(),
        1
    );

    let mut tar = Vec::new();
    let options = ExportOptions {
        spaces: vec![],
        verification_key: VerificationKey::Omit,
    };
    let MemoryReply::Exported(_) = service.export(&Caller::ShellUi, &options, &mut tar).await
    else {
        panic!("export")
    };
    let mut events = String::new();
    for entry in tar::Archive::new(tar.as_slice()).entries().expect("tar") {
        let mut entry = entry.expect("entry");
        if entry
            .path()
            .expect("path")
            .to_string_lossy()
            .ends_with("work/events.jsonl")
        {
            entry.read_to_string(&mut events).expect("read");
        }
    }
    assert!(events.contains("companion.session.words"), "{events}");
}
