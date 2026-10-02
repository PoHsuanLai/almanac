//! `Fts5` and `ExactScan` on their own, then `Index` behaviours beyond the frozen contract:
//! replacement by id (the newest event wins), separate documents with their own facets,
//! allow-lists, chunked documents, batching to `max_batch` and card checks.

use recall::*;
use std::collections::BTreeSet;
use std::sync::Mutex;

fn id(s: &str) -> DocId {
    DocId(s.to_owned())
}

fn doc_of(id_text: &str, kind: &str, trust: TrustTier, text: &str) -> Doc {
    Doc {
        id: id(id_text),
        text: text.to_owned(),
        at: 7,
        facets: Facets {
            kind: kind.into(),
            app: Some("mail".into()),
            trust,
        },
    }
}

fn doc(id_text: &str, text: &str) -> Doc {
    doc_of(id_text, "fact", TrustTier::Trusted, text)
}

fn fts() -> Fts5 {
    let fts = Fts5::new(rusqlite::Connection::open_in_memory().expect("db"));
    fts.create().expect("schema");
    fts
}

fn ids(ranked: &[Ranked]) -> Vec<&str> {
    ranked.iter().map(|r| r.id.0.as_str()).collect()
}

fn index_over(card: EmbedderCard) -> Index<ExactScan> {
    Index::new(fts(), ExactScan::in_memory(card).expect("vectors"))
}

fn query(text: &str, k: u32) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        k: TopK(k),
        allow: Allow::Everything,
        urgency: Urgency::Interactive,
    }
}

#[test]
fn create_records_the_format() {
    let f = fts();
    let format: i64 = f
        .connection()
        .query_row("SELECT format FROM meta", [], |r| r.get(0))
        .expect("meta row");
    assert_eq!(format, 1);
}

#[test]
fn fts_ranks_by_bm25_and_ranks_start_at_one() {
    let mut f = fts();
    f.upsert(&[
        doc("f:a", "apple"),
        doc("f:b", "apple apple apple pie and more filler words here"),
        doc("f:c", "banana"),
    ])
    .expect("upsert");
    let hits = f
        .search("apple", TopK(10), &Allow::Everything)
        .expect("search");
    assert_eq!(
        hits.iter().map(|r| r.rank).collect::<Vec<_>>(),
        vec![1, 2],
        "{hits:?}"
    );
    assert_eq!(ids(&hits).len(), 2);
    assert!(!ids(&hits).contains(&"f:c"));
}

#[test]
fn fts_search_cases() {
    let mut f = fts();
    f.upsert(&[doc("f:1", "Café résumé"), doc("f:2", "plain text")])
        .expect("upsert");
    let only_two = Allow::Only(BTreeSet::from([id("f:2")]));
    let cases: [(&str, u32, &Allow, Vec<&str>); 7] = [
        ("", 5, &Allow::Everything, vec![]),
        ("   !!! ", 5, &Allow::Everything, vec![]),
        ("cafe resume", 5, &Allow::Everything, vec!["f:1"]),
        ("text OR \"quote", 5, &Allow::Everything, vec!["f:2"]),
        ("plain cafe", 1, &Allow::Everything, vec!["f:1"]),
        ("plain cafe", 5, &only_two, vec!["f:2"]),
        ("nothing here", 5, &Allow::Everything, vec![]),
    ];
    for (text, k, allow, want) in cases {
        let hits = f.search(text, TopK(k), allow).expect("search");
        if text == "plain cafe" && k == 1 {
            assert_eq!(hits.len(), 1, "{text}");
        } else {
            assert_eq!(ids(&hits), want, "{text}");
        }
    }
}

#[test]
fn fts_upsert_replaces_by_id_and_remove_counts() {
    let mut f = fts();
    f.upsert(&[doc("f:1", "first version")]).expect("upsert");
    f.upsert(&[doc("f:1", "second version")]).expect("upsert");
    assert!(
        f.search("first", TopK(5), &Allow::Everything)
            .expect("s")
            .is_empty()
    );
    assert_eq!(
        ids(&f.search("second", TopK(5), &Allow::Everything).expect("s")),
        vec!["f:1"]
    );
    assert_eq!(f.remove(&[id("f:1"), id("f:none")]).expect("remove"), 1);
    assert_eq!(f.remove(&[id("f:1")]).expect("remove"), 0);
}

#[test]
fn fts_clear_empties_and_facets_are_stored() {
    let mut f = fts();
    f.upsert(&[doc_of("n:1", "narrative", TrustTier::Untrusted, "words")])
        .expect("upsert");
    let (kind, app, trust, at): (String, String, String, i64) = f
        .connection()
        .query_row("SELECT kind, app, trust, at_s FROM docs", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .expect("row");
    assert_eq!(
        (kind.as_str(), app.as_str(), trust.as_str(), at),
        ("narrative", "mail", "untrusted", 7)
    );
    f.clear().expect("clear");
    assert!(
        f.search("words", TopK(5), &Allow::Everything)
            .expect("s")
            .is_empty()
    );
}

fn card(dims: u32) -> EmbedderCard {
    EmbedderCard {
        dims,
        ..FakeEmbedder::new().card().clone()
    }
}

fn v(xs: &[f32]) -> Vector {
    Vector(xs.to_vec())
}

#[test]
fn exact_scan_orders_by_cosine_and_respects_allow_and_k() {
    let mut x = ExactScan::in_memory(card(2)).expect("db");
    x.upsert(&[
        (id("a"), v(&[1.0, 0.0])),
        (id("b"), v(&[0.7, 0.7])),
        (id("c"), v(&[0.0, 1.0])),
    ])
    .expect("upsert");
    let q = v(&[1.0, 0.1]);
    assert_eq!(
        ids(&x.nearest(&q, TopK(3), &Allow::Everything).expect("n")),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        ids(&x.nearest(&q, TopK(1), &Allow::Everything).expect("n")),
        vec!["a"]
    );
    let allow = Allow::Only(BTreeSet::from([id("b"), id("c")]));
    assert_eq!(
        ids(&x.nearest(&q, TopK(5), &allow).expect("n")),
        vec!["b", "c"]
    );
    assert_eq!(x.remove(&[id("a"), id("zz")]).expect("remove"), 1);
    x.upsert(&[(id("b"), v(&[0.0, 1.0]))]).expect("replace");
    assert_eq!(
        ids(&x
            .nearest(&v(&[0.0, 1.0]), TopK(5), &Allow::Everything)
            .expect("n")),
        vec!["b", "c"]
    );
    x.clear().expect("clear");
    assert!(
        x.nearest(&q, TopK(5), &Allow::Everything)
            .expect("n")
            .is_empty()
    );
}

#[test]
fn exact_scan_refuses_the_wrong_width() {
    let mut x = ExactScan::in_memory(card(2)).expect("db");
    assert_eq!(
        x.upsert(&[(id("a"), v(&[1.0, 0.0, 0.0]))]),
        Err(IndexError::CardMismatch)
    );
    assert_eq!(
        x.nearest(&v(&[1.0]), TopK(1), &Allow::Everything),
        Err(IndexError::CardMismatch)
    );
}

#[tokio::test]
async fn the_newest_upsert_of_an_id_wins_and_parts_keep_their_own_facets() {
    let e = FakeEmbedder::new();
    let mut index = index_over(e.card().clone());
    index
        .upsert(
            &[
                doc_of("e:r:1", "episode", TrustTier::Trusted, "opened the invoice"),
                doc_of(
                    "n:r:1",
                    "narrative",
                    TrustTier::Untrusted,
                    "the person seemed rushed",
                ),
            ],
            &e,
        )
        .await
        .expect("upsert");
    // The narrated event is a second event with the same id: the newest replaces the old.
    index
        .upsert(
            &[doc_of(
                "e:r:1",
                "episode",
                TrustTier::Trusted,
                "opened the receipt",
            )],
            &e,
        )
        .await
        .expect("upsert");
    let hits = index
        .search(&query("invoice receipt rushed", 10), &e)
        .await
        .expect("search");
    let found: Vec<&str> = hits.iter().map(|h| h.id.0.as_str()).collect();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.contains(&"e:r:1") && found.contains(&"n:r:1"));
    let old = index.lexical(&query("invoice", 10)).expect("lexical");
    assert!(old.is_empty(), "the replaced text is gone: {old:?}");
}

#[tokio::test]
async fn allow_restricts_both_halves() {
    let e = FakeEmbedder::new();
    let mut index = index_over(e.card().clone());
    index
        .upsert(&[doc("f:1", "alpha beta"), doc("f:2", "alpha gamma")], &e)
        .await
        .expect("upsert");
    let mut q = query("alpha", 10);
    q.allow = Allow::Only(BTreeSet::from([id("f:2")]));
    let hits = index.search(&q, &e).await.expect("search");
    assert_eq!(
        hits.iter().map(|h| h.id.0.as_str()).collect::<Vec<_>>(),
        vec!["f:2"]
    );
}

#[tokio::test]
async fn both_halves_agree_in_the_why() {
    let e = FakeEmbedder::new();
    let mut index = index_over(e.card().clone());
    index
        .upsert(&[doc("f:1", "lisbon receipts")], &e)
        .await
        .expect("upsert");
    let hits = index.search(&query("lisbon", 5), &e).await.expect("search");
    assert!(matches!(hits[0].why, HitWhy::Both { .. }), "{hits:?}");
    assert_eq!(index.state(), IndexState::Ready);
}

/// Records every call, to check batching and roles; vectors are the fake's.
#[derive(Debug)]
struct Recording {
    inner: FakeEmbedder,
    calls: Mutex<Vec<(usize, EmbedRole, Urgency)>>,
}

impl Embedder for Recording {
    fn card(&self) -> &EmbedderCard {
        self.inner.card()
    }
    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        self.calls
            .lock()
            .expect("lock")
            .push((texts.len(), role, urgency));
        self.inner.embed(texts, role, urgency).await
    }
}

fn recording() -> Recording {
    Recording {
        inner: FakeEmbedder::new(),
        calls: Mutex::new(Vec::new()),
    }
}

#[tokio::test]
async fn rebuild_embeds_documents_in_background_batches_of_max_batch() {
    let e = recording();
    let docs: Vec<Doc> = (0..70)
        .map(|i| doc(&format!("f:{i}"), &format!("word{i} shared")))
        .collect();
    let mut index = index_over(e.card().clone());
    index.rebuild(docs.into_iter(), &e).await.expect("rebuild");
    let calls = e.calls.lock().expect("lock");
    assert_eq!(
        calls.iter().map(|c| c.0).collect::<Vec<_>>(),
        vec![32, 32, 6]
    );
    assert!(
        calls
            .iter()
            .all(|c| c.1 == EmbedRole::Document && c.2 == Urgency::Background)
    );
    assert_eq!(index.state(), IndexState::Ready);
}

#[tokio::test]
async fn search_embeds_the_query_with_the_query_role() {
    let e = recording();
    let mut index = index_over(e.card().clone());
    index.upsert(&[doc("f:1", "alpha")], &e).await.expect("up");
    e.calls.lock().expect("lock").clear();
    index.search(&query("alpha", 3), &e).await.expect("search");
    assert_eq!(
        *e.calls.lock().expect("lock"),
        vec![(1, EmbedRole::Query, Urgency::Interactive)]
    );
}

#[tokio::test]
async fn an_unavailable_embedder_during_upsert_keeps_the_lexical_half() {
    let mut index = index_over(FakeEmbedder::new().card().clone());
    index
        .upsert(&[doc("f:1", "alpha")], &FakeEmbedder::unavailable())
        .await
        .expect("a retry-class failure is not an error");
    assert_eq!(
        index.state(),
        IndexState::LexicalOnly(DegradedWhy::EmbedderUnavailable)
    );
    assert_eq!(index.lexical(&query("alpha", 5)).expect("lexical").len(), 1);
}

#[tokio::test]
async fn another_embedder_space_is_refused_on_write_and_degrades_search() {
    let e = FakeEmbedder::new();
    let mut index = index_over(e.card().clone());
    index.upsert(&[doc("f:1", "alpha")], &e).await.expect("up");
    let other = FakeEmbedder::named("another-model");
    assert_eq!(
        index.upsert(&[doc("f:2", "beta")], &other).await,
        Err(IndexError::CardMismatch)
    );
    let hits = index
        .search(&query("alpha", 5), &other)
        .await
        .expect("search");
    assert!(hits.iter().all(|h| matches!(h.why, HitWhy::Lexical { .. })));
    assert_eq!(index.state(), IndexState::Stale(StaleWhy::EmbedderChanged));
}

#[tokio::test]
async fn a_long_document_is_chunked_and_still_found() {
    let e = FakeEmbedder::new();
    let mut index = index_over(e.card().clone());
    let long = format!("{} needle", "filler ".repeat(2000));
    index
        .upsert(&[doc("f:long", &long), doc("f:short", "other")], &e)
        .await
        .expect("upsert");
    let hits = index.search(&query("needle", 5), &e).await.expect("search");
    assert_eq!(hits[0].id, id("f:long"));
}

#[tokio::test]
async fn search_with_an_empty_query_returns_nothing() {
    let e = FakeEmbedder::new();
    let index = index_over(e.card().clone());
    assert!(
        index
            .search(&query("  ", 5), &e)
            .await
            .expect("s")
            .is_empty()
    );
}
