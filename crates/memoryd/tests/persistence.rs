//! The real backend over scratch directories: what a daemon leaves on disk and what the next
//! one finds, what is encrypted, what a wrong key opens, and what deleting a Space removes.

mod common;

use almanac_core::*;
use almanac_fake::{mail_thread_archived, thing};
use almanac_service::{Backend, BackendError, MemoryService, rules_from_toml, spaces_from_toml};
use common::{SharedKeys, TestBackend, backend, dirs_in, service, space};
use eventlog::LogError;
use memoryd::{Daemon, TablePeers};
use recall::{
    ClassTag, Doc, DocId, Embedder, Facets, FakeEmbedder, SearchQuery, TopK, TrustTier, VectorIndex,
};
use std::path::{Path, PathBuf};

async fn ask(
    service: &MemoryService<TestBackend>,
    caller: &Caller,
    request: MemoryRequest,
) -> MemoryReply {
    service.handle(caller, request).await
}

fn titled(title: &str, key: &str) -> Record {
    let mut record = mail_thread_archived().expect("fixture");
    if let EventBody::Thing { thing, .. } = &mut record.body {
        thing.title = title.into();
        thing.thing.key = ThingKey::parse(key).expect("key");
    }
    record
}

fn draft(topic: &str, text: &str) -> FactDraft {
    FactDraft {
        topic: TopicPath::parse(topic).expect("topic"),
        text: FactText::parse(text).expect("text"),
        links: vec![],
        supersedes: vec![],
    }
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn mentions(root: &Path, needle: &str) -> Vec<PathBuf> {
    files_under(root)
        .into_iter()
        .filter(|p| {
            std::fs::read(p)
                .is_ok_and(|bytes| bytes.windows(needle.len()).any(|w| w == needle.as_bytes()))
        })
        .collect()
}

#[tokio::test]
async fn a_restarted_daemon_finds_everything_where_the_last_one_left_it() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();
    let marked = thing("mail.thread", "marked1").expect("thing");

    // First run: events, a fact, a mark and a rule; the daemon persists its two files.
    let first = Daemon::new(service(&dirs, &keys), TablePeers::new(), dirs.clone());
    let queue = first.queue();
    let recorded = queue
        .handle(
            &Caller::Router,
            MemoryRequest::Record(titled("Lisbon receipts", "7f3a")),
        )
        .await;
    let MemoryReply::Recorded(first_event) = recorded else {
        panic!("{recorded:?}")
    };
    let proposed = queue
        .handle(
            &Caller::ShellUi,
            MemoryRequest::Propose(
                space("work"),
                draft("people/ana", "Ana is the CFO of Porto."),
            ),
        )
        .await;
    assert!(matches!(
        proposed,
        MemoryReply::Proposed(_, FactState::Active)
    ));
    queue
        .handle(
            &Caller::ShellUi,
            MemoryRequest::Mark(MarkRequest {
                space: space("work"),
                thing: marked.clone(),
                mark: MarkKind::DoNotRemember,
            }),
        )
        .await;
    let rule = RememberRule {
        id: RuleId::parse("r-1").expect("id"),
        scope: RuleScope::Kind(KindPattern::parse("search.*").expect("kind")),
        mode: RememberMode::HeaderOnly,
        retention: Retention::Days(DayCount(5)),
    };
    queue
        .handle(&Caller::ShellUi, MemoryRequest::SetRule(rule.clone()))
        .await;
    first.persist();
    let rules_before = first.queue().service().rules();
    drop(first);

    // Second run: spaces.toml and memory.toml are read back, the Space opens from its files.
    let spaces_text = std::fs::read_to_string(dirs.spaces_toml()).expect("spaces.toml");
    let metas = spaces_from_toml(&spaces_text).expect("spaces").spaces;
    let rules_text = std::fs::read_to_string(dirs.memory_toml()).expect("memory.toml");
    let rules = rules_from_toml(&rules_text).expect("rules");
    assert_eq!(rules, rules_before);
    assert!(rules.rules.contains(&rule));
    let second = MemoryService::new(backend(&dirs, &keys, Default::default()), rules);
    metas.into_iter().for_each(|m| second.register(m));

    let MemoryReply::Timeline(page) = ask(
        &second,
        &Caller::ShellUi,
        MemoryRequest::Timeline(
            space("work"),
            TimelineQuery {
                before: None,
                limit: Count(50),
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
    .await
    else {
        panic!("timeline")
    };
    assert!(page.entries.iter().any(|e| e.event == first_event));
    let MemoryReply::Facts(facts) = ask(
        &second,
        &Caller::ShellUi,
        MemoryRequest::Facts(FactQuery {
            space: space("work"),
            topic: None,
            about: None,
            state: FactFilter::Active,
            limit: Count(10),
        }),
    )
    .await
    else {
        panic!("facts")
    };
    assert_eq!(facts.len(), 1, "the sealed topic file reads back");
    assert_eq!(facts[0].fact.text.as_str(), "Ana is the CFO of Porto.");
    let MemoryReply::Hits(hits) = ask(
        &second,
        &Caller::Router,
        MemoryRequest::Search(RecallQuery {
            space: space("work"),
            text: "Lisbon".into(),
            limit: Count(5),
            over: RecallOver::Events,
        }),
    )
    .await
    else {
        panic!("hits")
    };
    assert!(
        hits.iter()
            .any(|h| h.doc == MemoryItem::Event(first_event.clone())),
        "the index serves events from the log: {hits:?}"
    );
    // The mark survived: a marked thing is still not remembered.
    let mut about_marked = titled("Secret", "marked1");
    about_marked.label = almanac_fake::trusted_label();
    assert_eq!(
        ask(
            &second,
            &Caller::Router,
            MemoryRequest::Record(about_marked)
        )
        .await,
        MemoryReply::Ok
    );
    // The log is the same chain, on the same replica, and still verifies.
    let MemoryReply::Recorded(next) = ask(
        &second,
        &Caller::Router,
        MemoryRequest::Record(titled("Porto invoices", "9c01")),
    )
    .await
    else {
        panic!("recorded")
    };
    assert_eq!(next.replica, first_event.replica);
    assert!(next.seq > first_event.seq);
    let verified = ask(
        &second,
        &Caller::ShellUi,
        MemoryRequest::Verify(space("work")),
    )
    .await;
    assert!(
        matches!(verified, MemoryReply::Verified(ChainReport::Intact { .. })),
        "{verified:?}"
    );
}

#[tokio::test]
async fn sealed_spaces_are_unreadable_at_rest_and_plain_ones_are_markdown() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();
    let svc = service(&dirs, &keys);
    svc.register(SpaceMeta {
        id: space("notebook"),
        created: UnixSeconds(1),
        replica: ReplicaId([4; 16]),
        vault: VaultKind::Plain,
        format: 1,
    });
    for id in ["work", "notebook"] {
        let mut record = titled("Quarterly zebra budget", "7f3a");
        record.space = space(id);
        ask(&svc, &Caller::Router, MemoryRequest::Record(record)).await;
        ask(
            &svc,
            &Caller::ShellUi,
            MemoryRequest::Propose(space(id), draft("people/ana", "Ana is the CFO of Porto.")),
        )
        .await;
    }
    // Nothing of what was said is in any file of the sealed Space: not the log, not the index,
    // not the topic file.
    for needle in ["Quarterly zebra budget", "CFO of Porto", "zebra"] {
        let leaks: Vec<PathBuf> = mentions(&dirs.space(&space("work")), needle)
            .into_iter()
            .chain(mentions(&dirs.index_dir(&space("work")), needle))
            .collect();
        assert!(leaks.is_empty(), "{needle:?} is readable in {leaks:?}");
    }
    // The plain Space keeps its topic file as readable markdown; its log and index stay sealed.
    let topic = dirs.topic(
        &space("notebook"),
        &TopicPath::parse("people/ana").expect("topic"),
    );
    let text = std::fs::read_to_string(&topic).expect("a plain topic file");
    assert!(text.contains("- Ana is the CFO of Porto."), "{text}");
    assert!(mentions(&dirs.events_db(&space("notebook")), "Quarterly").is_empty());
    assert!(mentions(&dirs.index_dir(&space("notebook")), "CFO").is_empty());
}

#[tokio::test]
async fn deleting_a_space_destroys_its_key_removes_its_directories_and_anchors_its_head() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();
    let svc = service(&dirs, &keys);
    ask(
        &svc,
        &Caller::Router,
        MemoryRequest::Record(titled("Lisbon receipts", "7f3a")),
    )
    .await;
    assert!(dirs.events_db(&space("work")).exists());
    assert!(dirs.index_db(&space("work")).exists());

    let MemoryReply::Plan(plan) = ask(
        &svc,
        &Caller::ShellUi,
        MemoryRequest::PlanForget(space("work"), ForgetScope::Space),
    )
    .await
    else {
        panic!("plan")
    };
    let report = ask(&svc, &Caller::ShellUi, MemoryRequest::Forget(plan.token)).await;
    assert!(matches!(report, MemoryReply::Forgot(_)), "{report:?}");

    assert!(
        !dirs.space(&space("work")).exists(),
        "the Space's directory is gone"
    );
    assert!(!dirs.index_dir(&space("work")).exists(), "and its index");
    assert_eq!(
        almanac_seal::KeyStore::get(&keys, &space("work")).await,
        Err(almanac_seal::KeyError::Missing),
        "and its key"
    );
    // The desktop log kept where the deleted chain ended.
    let MemoryReply::Timeline(page) = ask(
        &svc,
        &Caller::ShellUi,
        MemoryRequest::Timeline(
            SpaceId::desktop(),
            TimelineQuery {
                before: None,
                limit: Count(20),
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
    .await
    else {
        panic!("timeline")
    };
    assert!(
        page.entries
            .iter()
            .any(|e| e.kind.as_str() == "memory.space_deleted")
    );
    assert!(dirs.events_db(&SpaceId::desktop()).exists());
}

fn key_for(seed: u8) -> almanac_seal::SpaceKey {
    almanac_seal::SpaceKey::from_bytes([seed; 32])
}

#[test]
fn a_wrong_key_opens_neither_the_log_nor_the_index() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();
    let backend: TestBackend = backend(&dirs, &keys, Default::default());
    let (right, wrong) = (key_for(1), key_for(2));
    let id = space("work");

    drop(
        backend
            .open_log(&id, ReplicaId([1; 16]), &right)
            .expect("created"),
    );
    drop(backend.open_index(&id, &right).expect("created"));
    drop(
        backend
            .open_log(&id, ReplicaId([1; 16]), &right)
            .expect("reopened"),
    );
    drop(backend.open_index(&id, &right).expect("reopened"));

    assert!(matches!(
        backend.open_log(&id, ReplicaId([1; 16]), &wrong),
        Err(BackendError::Log(LogError::Locked))
    ));
    assert!(matches!(
        backend.open_index(&id, &wrong),
        Err(BackendError::Index(_))
    ));
}

fn doc(id: &str, text: &str) -> Doc {
    Doc {
        id: DocId(id.to_owned()),
        text: text.to_owned(),
        at: 1,
        facets: Facets {
            kind: "fact".to_owned(),
            app: None,
            trust: TrustTier::Trusted,
        },
        class: ClassTag::default(),
    }
}

#[tokio::test]
async fn the_index_file_is_created_with_its_schema_and_keeps_what_was_put_in_it() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();
    let backend: TestBackend = backend(&dirs, &keys, Default::default());
    let key = key_for(3);
    let id = space("work");
    let embedder = FakeEmbedder::new();

    let mut index = backend.open_index(&id, &key).expect("created");
    index
        .upsert(
            &[
                doc("f:1", "Ana runs the Lisbon office"),
                doc("f:2", "Bob likes tea"),
            ],
            &embedder,
        )
        .await
        .expect("upsert");
    drop(index);
    assert!(dirs.index_db(&id).exists());

    let index = backend.open_index(&id, &key).expect("reopened");
    let query = SearchQuery {
        text: "Lisbon".into(),
        k: TopK(5),
        allow: recall::Allow::Everything,
        urgency: recall::Urgency::Interactive,
    };
    let lexical = index.lexical(&query).expect("lexical");
    assert_eq!(lexical.first().map(|r| r.id.0.as_str()), Some("f:1"));
    let nearest = index
        .parts()
        .1
        .nearest(
            &FakeEmbedder::vector("Lisbon office"),
            TopK(1),
            &recall::Allow::Everything,
        )
        .expect("nearest");
    assert_eq!(
        nearest.first().map(|r| r.id.0.as_str()),
        Some("f:1"),
        "the vectors are in the file too"
    );
    assert_eq!(index.parts().1.card(), embedder.card());
}

#[test]
fn the_backends_random_bytes_differ_from_call_to_call() {
    let scratch = tempfile::tempdir().expect("scratch");
    let backend: TestBackend = backend(
        &dirs_in(scratch.path()),
        &SharedKeys::default(),
        Default::default(),
    );
    let draws: Vec<[u8; 16]> = (0..4).map(|_| backend.random()).collect();
    for (i, a) in draws.iter().enumerate() {
        assert_ne!(*a, [0u8; 16]);
        for b in &draws[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn removing_a_space_that_is_not_there_is_not_an_error() {
    let scratch = tempfile::tempdir().expect("scratch");
    let backend: TestBackend = backend(
        &dirs_in(scratch.path()),
        &SharedKeys::default(),
        Default::default(),
    );
    assert_eq!(backend.remove_space(&space("never-was")), Ok(()));
}
