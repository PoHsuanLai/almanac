//! A restarted daemon adopts the index it finds: opening a Space embeds nothing again (the
//! documents are already in `index.db`), unless the embedder is another one or the file is gone.
//! The counting embedder is the proof; SQLCipher, the sealed vault and the files are real.

use crate::support::{SharedKeys, dirs_in, space};
use almanac_core::*;
use almanac_fake::{ScriptedConsolidator, mail_thread_archived};
use almanac_service::MemoryService;
use memoryd::SystemBackend;
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, FakeEmbedder, Urgency, Vector};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The fake embedder, counting the document texts it is asked for.
#[derive(Debug, Clone)]
struct Counting {
    inner: FakeEmbedder,
    documents: Arc<AtomicUsize>,
}

impl Counting {
    fn new(inner: FakeEmbedder) -> Self {
        Self {
            inner,
            documents: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn documents(&self) -> usize {
        self.documents.load(Ordering::SeqCst)
    }
}

impl Embedder for Counting {
    fn card(&self) -> &EmbedderCard {
        self.inner.card()
    }

    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        if role == EmbedRole::Document {
            self.documents.fetch_add(texts.len(), Ordering::SeqCst);
        }
        self.inner.embed(texts, role, urgency).await
    }
}

type Counted = SystemBackend<SharedKeys, Counting, ScriptedConsolidator>;

fn service(dirs: &Dirs, keys: &SharedKeys, embedder: &Counting) -> MemoryService<Counted> {
    MemoryService::new(
        SystemBackend::with(
            dirs.clone(),
            keys.clone(),
            embedder.clone(),
            ScriptedConsolidator::default(),
        ),
        RuleSet::standard(),
    )
}

async fn ask(service: &MemoryService<Counted>, caller: &Caller, r: MemoryRequest) -> MemoryReply {
    service.handle(caller, r).await
}

fn titled(title: &str, key: &str) -> Record {
    let mut record = mail_thread_archived().expect("fixture");
    if let EventBody::Thing { thing, .. } = &mut record.body {
        thing.title = title.into();
        thing.thing.key = ThingKey::parse(key).expect("key");
    }
    record
}

async fn fill(service: &MemoryService<Counted>) {
    for (title, key) in [("Lisbon receipts", "7f3a"), ("Porto invoices", "8b4c")] {
        let reply = ask(
            service,
            &Caller::Router,
            MemoryRequest::Record(titled(title, key)),
        )
        .await;
        assert!(matches!(reply, MemoryReply::Recorded(_)), "{reply:?}");
    }
    let draft = FactDraft {
        topic: TopicPath::parse("people/ana").expect("topic"),
        text: FactText::parse("Ana is the CFO of Porto.").expect("text"),
        links: vec![],
        supersedes: vec![],
    };
    let reply = ask(
        service,
        &Caller::ShellUi,
        MemoryRequest::Propose(space("work"), draft),
    )
    .await;
    assert!(matches!(reply, MemoryReply::Proposed(..)), "{reply:?}");
}

async fn search(service: &MemoryService<Counted>, text: &str) -> Vec<RecallHit> {
    let query = RecallQuery {
        space: space("work"),
        text: text.into(),
        limit: Count(10),
        over: RecallOver::Both,
    };
    match ask(service, &Caller::Router, MemoryRequest::Search(query)).await {
        MemoryReply::Hits(hits) => hits,
        other => panic!("{other:?}"),
    }
}

async fn status(service: &MemoryService<Counted>) -> SpaceStatus {
    match ask(
        service,
        &Caller::ShellUi,
        MemoryRequest::Status(space("work")),
    )
    .await
    {
        MemoryReply::Status(s) => s,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_restart_does_not_embed_the_documents_again() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();

    let first = Counting::new(FakeEmbedder::new());
    let before = service(&dirs, &keys, &first);
    fill(&before).await;
    assert!(
        first.documents() >= 3,
        "two events and a fact were embedded as they came: {}",
        first.documents()
    );
    drop(before);

    let second = Counting::new(FakeEmbedder::new());
    let after = service(&dirs, &keys, &second);
    let open = status(&after).await;
    assert_eq!(open.index, IndexView::Ready, "{open:?}");
    assert_eq!(
        second.documents(),
        0,
        "the Space opened on the index it found"
    );
    let hits = search(&after, "Lisbon receipts").await;
    assert!(
        hits.iter()
            .any(|h| matches!(h.why, RecallWhy::Both { .. } | RecallWhy::Lexical { .. })),
        "{hits:?}"
    );
    assert_eq!(
        second.documents(),
        0,
        "and a search embeds only its query as a query"
    );
}

#[tokio::test]
async fn a_deleted_index_is_rebuilt_and_another_model_rebuilds_it_too() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dirs = dirs_in(scratch.path());
    let keys = SharedKeys::default();
    let first = Counting::new(FakeEmbedder::new());
    let before = service(&dirs, &keys, &first);
    fill(&before).await;
    let embedded = first.documents();
    drop(before);

    // The cache was cleared: the index is made again from the files and the log.
    std::fs::remove_dir_all(dirs.index_dir(&space("work"))).expect("clear the cache");
    let second = Counting::new(FakeEmbedder::new());
    let again = service(&dirs, &keys, &second);
    assert_eq!(status(&again).await.index, IndexView::Ready);
    assert_eq!(
        second.documents(),
        embedded,
        "every document that was embedded the first time"
    );
    drop(again);

    // The configured model changed: the vectors are in another space, so all of them are made again.
    let third = Counting::new(FakeEmbedder::named("another-model"));
    let other = service(&dirs, &keys, &third);
    assert_eq!(status(&other).await.index, IndexView::Ready);
    assert_eq!(third.documents(), embedded);
    drop(other);

    // And a restart on that model adopts it.
    let fourth = Counting::new(FakeEmbedder::named("another-model"));
    let adopted = service(&dirs, &keys, &fourth);
    assert_eq!(status(&adopted).await.index, IndexView::Ready);
    assert_eq!(fourth.documents(), 0);
}
