//! `Index` over `ExactScan` and `Fts5`: the contract the recall fill must meet.

use recall::*;

fn id(s: &str) -> DocId {
    DocId(s.to_owned())
}

fn doc(id_text: &str, text: &str) -> Doc {
    Doc {
        id: id(id_text),
        text: text.to_owned(),
        at: 0,
        facets: Facets {
            kind: "fact".into(),
            app: None,
            trust: TrustTier::Trusted,
        },
    }
}

fn docs() -> Vec<Doc> {
    vec![
        doc("f:1", "Prefers meetings after ten in the morning"),
        doc("f:2", "Lisbon receipts go to accounting"),
        doc("f:3", "The quarterly roadmap is owned by Ana"),
    ]
}

fn fresh_index() -> Index<ExactScan> {
    let card = FakeEmbedder::new().card().clone();
    let fts = Fts5::new(rusqlite::Connection::open_in_memory().expect("db"));
    fts.create().expect("schema");
    Index::new(fts, ExactScan::in_memory(card).expect("vectors"))
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
#[ignore = "Index and ExactScan SQLite bodies are todo!() until the recall fill (FINDINGS.md)"]
async fn rebuild_equals_incremental() {
    let e = FakeEmbedder::new();
    let mut rebuilt = fresh_index();
    rebuilt
        .rebuild(docs().into_iter(), &e)
        .await
        .expect("rebuild");
    let mut incremental = fresh_index();
    for d in docs() {
        incremental.upsert(&[d], &e).await.expect("upsert");
    }
    for text in ["receipts lisbon", "roadmap", "meetings morning"] {
        assert_eq!(
            rebuilt.search(&query(text), &e).await.expect("search"),
            incremental.search(&query(text), &e).await.expect("search"),
            "{text}"
        );
    }
    assert_eq!(rebuilt.state(), IndexState::Ready);
}

#[tokio::test]
#[ignore = "Index and ExactScan SQLite bodies are todo!() until the recall fill (FINDINGS.md)"]
async fn search_degrades_to_lexical() {
    let mut index = fresh_index();
    index
        .upsert(&docs(), &FakeEmbedder::new())
        .await
        .expect("upsert");
    let hits = index
        .search(&query("receipts"), &FakeEmbedder::unavailable())
        .await
        .expect("search never fails for a busy embedder");
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| matches!(h.why, HitWhy::Lexical { .. })));
    assert_eq!(
        index.state(),
        IndexState::LexicalOnly(DegradedWhy::EmbedderUnavailable)
    );
}

#[tokio::test]
#[ignore = "Index and ExactScan SQLite bodies are todo!() until the recall fill (FINDINGS.md)"]
async fn remove_drops_from_both_indexes() {
    let e = FakeEmbedder::new();
    let mut index = fresh_index();
    index.upsert(&docs(), &e).await.expect("upsert");
    assert_eq!(index.remove(&[id("f:2")]).expect("remove"), 1);
    let hits = index
        .search(&query("receipts lisbon accounting"), &e)
        .await
        .expect("search");
    assert!(hits.iter().all(|h| h.id != id("f:2")));
    let (fts, vectors) = index.parts();
    assert!(
        fts.search("receipts", TopK(5), &Allow::Everything)
            .expect("fts")
            .is_empty()
    );
    assert!(
        vectors
            .nearest(
                &FakeEmbedder::vector("receipts"),
                TopK(5),
                &Allow::Everything
            )
            .expect("vectors")
            .iter()
            .all(|r| r.id != id("f:2"))
    );
}
