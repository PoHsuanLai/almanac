//! A restart adopts the index file it finds (no embedding of what is already there), and a
//! refused data class fails only its own documents.

use recall::*;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

fn doc_in(id: &str, text: &str, class: &str) -> Doc {
    Doc {
        id: DocId(id.to_owned()),
        text: text.to_owned(),
        at: 5,
        facets: Facets {
            kind: "fact".into(),
            app: None,
            trust: TrustTier::Trusted,
        },
        class: ClassTag(class.to_owned()),
    }
}

fn doc(id: &str, text: &str) -> Doc {
    doc_in(id, text, "")
}

fn docs() -> Vec<Doc> {
    vec![
        doc("f:1", "Prefers meetings after ten in the morning"),
        doc("f:2", "Lisbon receipts go to accounting"),
        doc("f:3", "The quarterly roadmap is owned by Ana"),
    ]
}

/// The index file at `path` as a daemon opens it: both halves over their own connection, the
/// schema made when the file is new.
fn open(path: &Path, card: EmbedderCard) -> Index<ExactScan> {
    let fresh = !path.exists();
    let lexical = rusqlite::Connection::open(path).expect("open");
    let fts = Fts5::new(lexical);
    if fresh {
        fts.create().expect("schema");
    }
    let vectors = ExactScan::new(rusqlite::Connection::open(path).expect("open"), card);
    Index::new(fts, vectors)
}

/// Counts how many texts it was asked to embed (and in how many calls), vectors are the fake's.
#[derive(Debug)]
struct Counting {
    inner: FakeEmbedder,
    texts: Mutex<usize>,
}

impl Counting {
    fn new(inner: FakeEmbedder) -> Self {
        Self {
            inner,
            texts: Mutex::new(0),
        }
    }

    fn embedded(&self) -> usize {
        *self.texts.lock().expect("lock")
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
            *self.texts.lock().expect("lock") += texts.len();
        }
        self.inner.embed(texts, role, urgency).await
    }
}

fn ids_with_vectors(index: &Index<ExactScan>) -> BTreeSet<String> {
    index
        .parts()
        .1
        .ids()
        .expect("ids")
        .into_iter()
        .map(|d| d.0)
        .collect()
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.to_owned(),
        k: TopK(10),
        allow: Allow::Everything,
        urgency: Urgency::Interactive,
    }
}

#[tokio::test]
async fn a_restart_adopts_the_index_and_embeds_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let first = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, first.card().clone());
    index.sync(docs().into_iter(), &first).await.expect("sync");
    assert_eq!(first.embedded(), 3);
    drop(index);

    let again = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, again.card().clone());
    index.sync(docs().into_iter(), &again).await.expect("sync");
    assert_eq!(again.embedded(), 0, "nothing was embedded again");
    assert_eq!(index.state(), IndexState::Ready);
    assert_eq!(ids_with_vectors(&index).len(), 3);
    let hits = index
        .search(&query("lisbon receipts"), &again)
        .await
        .expect("search");
    assert!(matches!(hits[0].why, HitWhy::Both { .. }), "{hits:?}");
}

#[tokio::test]
async fn only_what_changed_while_the_daemon_was_down_is_embedded() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let e = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, e.card().clone());
    index.sync(docs().into_iter(), &e).await.expect("sync");
    drop(index);

    let again = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, again.card().clone());
    let truth = vec![
        doc("f:1", "Prefers meetings after ten in the morning"),
        doc("f:2", "Lisbon receipts now go to finance"),
        doc("f:4", "A fact that arrived meanwhile"),
    ];
    index.sync(truth.into_iter(), &again).await.expect("sync");
    assert_eq!(again.embedded(), 2, "the edited and the new document");
    assert_eq!(
        ids_with_vectors(&index),
        BTreeSet::from(["f:1".to_owned(), "f:2".to_owned(), "f:4".to_owned()]),
        "f:3 left the vectors"
    );
    let gone = index
        .search(&query("roadmap"), &again)
        .await
        .expect("search");
    assert!(
        gone.iter().all(|h| h.id != DocId("f:3".into())),
        "f:3 is in neither half: {gone:?}"
    );
    let edited = index
        .search(&query("finance"), &again)
        .await
        .expect("search");
    assert_eq!(edited[0].id, DocId("f:2".into()));
}

#[tokio::test]
async fn a_changed_facet_alone_is_embedded_again_only_for_that_document() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let e = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, e.card().clone());
    index.sync(docs().into_iter(), &e).await.expect("sync");
    drop(index);
    let again = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, again.card().clone());
    let mut truth = docs();
    truth[0].facets.trust = TrustTier::Untrusted;
    index.sync(truth.into_iter(), &again).await.expect("sync");
    assert_eq!(again.embedded(), 1);
}

#[tokio::test]
async fn another_model_or_prefix_is_a_full_rebuild() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let e = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, e.card().clone());
    index.sync(docs().into_iter(), &e).await.expect("sync");
    drop(index);

    let other = Counting::new(FakeEmbedder::named("another-model"));
    let mut index = open(&path, other.card().clone());
    index.sync(docs().into_iter(), &other).await.expect("sync");
    assert_eq!(other.embedded(), 3, "every document, in the new space");
    drop(index);

    // The same model with a prefix is another space too.
    let prefixed = Counting::new(
        FakeEmbedder::named("another-model").with_document_prefix("search_document: "),
    );
    assert_ne!(prefixed.card().space_key(), other.card().space_key());
    let mut index = open(&path, prefixed.card().clone());
    index
        .sync(docs().into_iter(), &prefixed)
        .await
        .expect("sync");
    assert_eq!(prefixed.embedded(), 3);
}

#[tokio::test]
async fn a_file_that_never_recorded_its_space_is_rebuilt() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    // The index as an earlier daemon left it: the schema without the recorded space.
    let old = rusqlite::Connection::open(&path).expect("open");
    old.execute_batch(
        "CREATE VIRTUAL TABLE docs USING fts5(
           text, id UNINDEXED, kind UNINDEXED, app UNINDEXED, trust UNINDEXED, at_s UNINDEXED,
           tokenize = 'unicode61 remove_diacritics 2');
         CREATE TABLE vectors(id TEXT PRIMARY KEY, vec BLOB NOT NULL);
         CREATE TABLE meta(format INTEGER NOT NULL, model TEXT, dims INTEGER, metric TEXT);
         INSERT INTO meta(format, model, dims, metric) VALUES (1, NULL, NULL, NULL);",
    )
    .expect("old schema");
    drop(old);
    let e = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, e.card().clone());
    index.sync(docs().into_iter(), &e).await.expect("sync");
    assert_eq!(e.embedded(), 3);
    drop(index);
    let again = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, again.card().clone());
    index.sync(docs().into_iter(), &again).await.expect("sync");
    assert_eq!(again.embedded(), 0, "the rebuild recorded the space");
}

#[tokio::test]
async fn documents_that_never_got_a_vector_are_embedded_at_the_next_start() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let away = FakeEmbedder::unavailable();
    let mut index = open(&path, away.card().clone());
    index.sync(docs().into_iter(), &away).await.expect("sync");
    assert!(matches!(index.state(), IndexState::LexicalOnly(_)));
    assert!(ids_with_vectors(&index).is_empty());
    drop(index);

    let back = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, back.card().clone());
    index.sync(docs().into_iter(), &back).await.expect("sync");
    assert_eq!(back.embedded(), 3);
    assert_eq!(index.state(), IndexState::Ready);
}

#[tokio::test]
async fn a_document_with_no_text_never_counts_as_missing_a_vector() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let truth = vec![doc("f:1", "something"), doc("f:2", "   ")];
    let e = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, e.card().clone());
    index
        .sync(truth.clone().into_iter(), &e)
        .await
        .expect("sync");
    drop(index);
    let again = Counting::new(FakeEmbedder::new());
    let mut index = open(&path, again.card().clone());
    index.sync(truth.into_iter(), &again).await.expect("sync");
    assert_eq!(again.embedded(), 0);
}

/// Refuses any batch that holds a text of the class `refuses`, the way inferd's session for
/// that class would; every other batch is the fake's.
#[derive(Debug)]
struct Refusing {
    inner: FakeEmbedder,
    refuses: &'static str,
    calls: Mutex<Vec<usize>>,
}

impl Refusing {
    fn new(refuses: &'static str) -> Self {
        Self {
            inner: FakeEmbedder::new(),
            refuses,
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl Embedder for Refusing {
    fn card(&self) -> &EmbedderCard {
        self.inner.card()
    }
    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        self.inner.embed(texts, role, urgency).await
    }
    async fn embed_classed(
        &self,
        texts: &[Classed],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        self.calls.lock().expect("lock").push(texts.len());
        if texts.iter().any(|t| t.class.0 == self.refuses) {
            return Err(EmbedError::Refused("this class stays on the device".into()));
        }
        let plain: Vec<String> = texts.iter().map(|t| t.text.clone()).collect();
        self.inner.embed(&plain, role, urgency).await
    }
}

fn mixed() -> Vec<Doc> {
    vec![
        doc_in("f:1", "Lisbon receipts go to accounting", "mail"),
        doc_in("f:2", "The quarterly roadmap is owned by Ana", "prompt"),
        doc_in("f:3", "Prefers meetings after ten", "mail"),
        doc("f:4", "Standup is at nine"),
    ]
}

#[tokio::test]
async fn a_refused_class_fails_only_its_own_documents_on_upsert() {
    let dir = tempfile::tempdir().expect("dir");
    let e = Refusing::new("mail");
    let mut index = open(&dir.path().join("index.db"), e.card().clone());
    index
        .upsert(&mixed(), &e)
        .await
        .expect("a refusal is not a failure");
    assert_eq!(
        ids_with_vectors(&index),
        BTreeSet::from(["f:2".to_owned(), "f:4".to_owned()]),
        "the other classes were embedded"
    );
    let hits = index
        .search(&query("lisbon receipts"), &e)
        .await
        .expect("search");
    assert_eq!(hits[0].id, DocId("f:1".into()), "still found lexically");
    assert_eq!(index.state(), IndexState::Ready);
}

#[tokio::test]
async fn a_refused_class_fails_only_its_own_documents_on_rebuild_and_sync() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("index.db");
    let e = Refusing::new("mail");
    let mut index = open(&path, e.card().clone());
    index
        .rebuild(mixed().into_iter(), &e)
        .await
        .expect("rebuild");
    assert_eq!(
        ids_with_vectors(&index),
        BTreeSet::from(["f:2".to_owned(), "f:4".to_owned()])
    );
    assert_eq!(index.state(), IndexState::Ready);
    drop(index);

    // The next start retries the refused ones (one batch per class), and embeds none of the rest.
    let again = Refusing::new("mail");
    let mut index = open(&path, again.card().clone());
    index.sync(mixed().into_iter(), &again).await.expect("sync");
    let calls = again.calls.lock().expect("lock").clone();
    assert_eq!(calls, vec![2], "only the two refused documents were sent");
    assert_eq!(ids_with_vectors(&index).len(), 2);
}

#[tokio::test]
async fn when_every_document_is_refused_the_index_is_lexical_only() {
    let dir = tempfile::tempdir().expect("dir");
    let e = Refusing::new("mail");
    let mut index = open(&dir.path().join("index.db"), e.card().clone());
    let all_mail = vec![
        doc_in("f:1", "Lisbon receipts", "mail"),
        doc_in("f:2", "Quarterly roadmap", "mail"),
    ];
    index.upsert(&all_mail, &e).await.expect("upsert");
    assert_eq!(
        index.state(),
        IndexState::LexicalOnly(DegradedWhy::EmbedderRefused)
    );
    assert!(ids_with_vectors(&index).is_empty());
    let hits = index.search(&query("roadmap"), &e).await.expect("search");
    assert_eq!(hits[0].id, DocId("f:2".into()));
}

#[tokio::test]
async fn another_failure_still_fails_the_batch() {
    #[derive(Debug)]
    struct Broken(FakeEmbedder);
    impl Embedder for Broken {
        fn card(&self) -> &EmbedderCard {
            self.0.card()
        }
        async fn embed(
            &self,
            _: &[String],
            _: EmbedRole,
            _: Urgency,
        ) -> Result<Vec<Vector>, EmbedError> {
            Err(EmbedError::Failed {
                class: RetryClass::Fatal,
                why: "wrong vectors".into(),
            })
        }
    }
    let dir = tempfile::tempdir().expect("dir");
    let e = Broken(FakeEmbedder::new());
    let mut index = open(&dir.path().join("index.db"), e.card().clone());
    let failed = index.upsert(&mixed(), &e).await;
    assert!(matches!(failed, Err(IndexError::Embed(_))), "{failed:?}");
}

#[test]
fn the_recorded_space_key_agrees_with_space_vs() {
    let base = FakeEmbedder::new();
    let cards = [
        FakeEmbedder::new(),
        FakeEmbedder::named("other"),
        FakeEmbedder::new().with_document_prefix("p: "),
        FakeEmbedder::new().with_max_batch(4),
    ];
    for other in cards {
        let same = base.card().space_vs(other.card()) == SpaceCheck::Same;
        assert_eq!(
            same,
            base.card().space_key() == other.card().space_key(),
            "{:?}",
            other.card()
        );
    }
}
