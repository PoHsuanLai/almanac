//! recall's pure parts and the contracts the fill must meet.

use recall::*;

fn id(s: &str) -> DocId {
    DocId(s.to_owned())
}

fn ranked(ids: &[&str]) -> Vec<Ranked> {
    ids.iter()
        .enumerate()
        .map(|(i, s)| Ranked {
            id: id(s),
            rank: u32::try_from(i + 1).expect("rank"),
        })
        .collect()
}

#[test]
fn fuse_rrf_table() {
    // (name, lexical, semantic, expected (id, score, why) best first)
    type Row = (
        &'static str,
        Vec<&'static str>,
        Vec<&'static str>,
        Vec<(&'static str, u32, HitWhy)>,
    );
    let cases: Vec<Row> = vec![
        ("empty", vec![], vec![], vec![]),
        (
            "lexical only",
            vec!["a", "b"],
            vec![],
            vec![
                ("a", 16_393, HitWhy::Lexical { rank: 1 }),
                ("b", 16_129, HitWhy::Lexical { rank: 2 }),
            ],
        ),
        (
            "semantic only",
            vec![],
            vec!["x"],
            vec![("x", 16_393, HitWhy::Semantic { rank: 1 })],
        ),
        (
            "a hit in both outranks a first place in one",
            vec!["a", "b"],
            vec!["c", "b"],
            vec![
                (
                    "b",
                    32_258,
                    HitWhy::Both {
                        lexical: 2,
                        semantic: 2,
                    },
                ),
                ("a", 16_393, HitWhy::Lexical { rank: 1 }),
                ("c", 16_393, HitWhy::Semantic { rank: 1 }),
            ],
        ),
    ];
    for (name, lexical, semantic, want) in cases {
        let got = fuse_rrf(&[ranked(&lexical), ranked(&semantic)], RrfK(60));
        let got: Vec<(&str, u32, HitWhy)> = got
            .iter()
            .map(|f| (f.id.0.as_str(), f.score_millionths, f.why))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn fuse_rrf_ignores_a_third_list_and_breaks_ties_by_id() {
    let fused = fuse_rrf(&[ranked(&["b"]), ranked(&["a"]), ranked(&["z"])], RrfK(60));
    let ids: Vec<&str> = fused.iter().map(|f| f.id.0.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn chunk_table() {
    let cases: Vec<(&str, &str, u32, Vec<&str>)> = vec![
        ("empty", "", 4, vec![]),
        ("blank", "  \n\t ", 4, vec![]),
        ("fits", "one two three", 10, vec!["one two three"]),
        (
            "splits on words, 4 chars a token",
            "aaaa bbbb cccc dddd",
            2,
            vec!["aaaa", "bbbb", "cccc", "dddd"],
        ),
        (
            "packs greedily",
            "aa bb cc dd ee",
            2,
            vec!["aa bb cc", "dd ee"],
        ),
        (
            "a long word is its own chunk",
            "x abcdefghijkl y",
            2,
            vec!["x", "abcdefghijkl", "y"],
        ),
        ("whitespace is normalised", "a\n\nb\t c", 10, vec!["a b c"]),
        (
            "zero tokens means one token",
            "aaaa bbbb",
            0,
            vec!["aaaa", "bbbb"],
        ),
    ];
    for (name, text, max, want) in cases {
        let got: Vec<String> = chunk(text, max).into_iter().map(|c| c.text).collect();
        assert_eq!(got, want, "{name}");
    }
    let indexes: Vec<u32> = chunk("aa bb cc dd ee", 2).iter().map(|c| c.index).collect();
    assert_eq!(indexes, [0, 1]);
}

#[test]
fn exact_scan_nearest_orders_by_cosine() {
    let v = |x: &[f32]| Vector(x.to_vec());
    let items = vec![
        (id("east"), v(&[1.0, 0.0])),
        (id("north"), v(&[0.0, 2.0])),
        (id("north-east"), v(&[1.0, 1.0])),
        (id("zero"), v(&[0.0, 0.0])),
    ];
    let q = v(&[0.0, 1.0]);
    let order = |metric, k| -> Vec<String> {
        nearest_exact(&q, &items, metric, TopK(k))
            .into_iter()
            .map(|r| r.id.0)
            .collect()
    };
    assert_eq!(order(Metric::Cosine, 3), ["north", "north-east", "east"]);
    assert_eq!(order(Metric::Cosine, 1), ["north"]);
    // Dot rewards length: north (2.0) beats north-east (1.0); ties by id.
    assert_eq!(
        order(Metric::Dot, 4),
        ["north", "north-east", "east", "zero"]
    );
    let ranks: Vec<u32> = nearest_exact(&q, &items, Metric::Cosine, TopK(3))
        .iter()
        .map(|r| r.rank)
        .collect();
    assert_eq!(ranks, [1, 2, 3]);
    assert_eq!(
        Metric::Cosine.score(&v(&[1.0]), &v(&[1.0, 2.0])),
        0.0,
        "mismatched lengths score zero"
    );
}

#[test]
fn vectors_round_trip_through_their_blob() {
    let original = Vector(vec![0.5, -1.25, 3.0e10, f32::MIN_POSITIVE]);
    let blob = original.to_blob();
    assert_eq!(blob.len(), 16);
    assert_eq!(&blob[..4], &0.5f32.to_le_bytes());
    assert_eq!(Vector::from_blob(&blob), Some(original));
    assert_eq!(Vector::from_blob(&blob[..15]), None);
}

#[test]
fn embedder_card_change_marks_stale() {
    let ready = IndexState::Ready;
    assert_eq!(
        step(ready, IndexEvent::CardChanged),
        IndexState::Stale(StaleWhy::EmbedderChanged)
    );
    let cards = (
        EmbedderCard {
            model: "a".into(),
            dims: 4,
            max_tokens: 8,
            max_batch: MaxBatch(4),
            prompts: PromptPrefixes::default(),
            metric: Metric::Cosine,
        },
        EmbedderCard {
            model: "b".into(),
            dims: 4,
            max_tokens: 8,
            max_batch: MaxBatch(4),
            prompts: PromptPrefixes::default(),
            metric: Metric::Cosine,
        },
    );
    assert_ne!(cards.0, cards.1);
}

#[test]
fn index_state_table() {
    use DegradedWhy::EmbedderUnavailable as Gone;
    let c = Count;
    let cases: Vec<(&str, IndexState, IndexEvent, IndexState)> = vec![
        (
            "absent builds",
            IndexState::Absent,
            IndexEvent::Begin { total: c(10) },
            IndexState::Building {
                done: c(0),
                total: c(10),
            },
        ),
        (
            "progress",
            IndexState::Building {
                done: c(0),
                total: c(10),
            },
            IndexEvent::Progress { done: c(4) },
            IndexState::Building {
                done: c(4),
                total: c(10),
            },
        ),
        (
            "finished",
            IndexState::Building {
                done: c(10),
                total: c(10),
            },
            IndexEvent::Finished,
            IndexState::Ready,
        ),
        (
            "card changed",
            IndexState::Ready,
            IndexEvent::CardChanged,
            IndexState::Stale(StaleWhy::EmbedderChanged),
        ),
        (
            "format changed",
            IndexState::Ready,
            IndexEvent::FormatChanged,
            IndexState::Stale(StaleWhy::FormatChanged),
        ),
        (
            "truth changed",
            IndexState::Ready,
            IndexEvent::TruthChanged,
            IndexState::Stale(StaleWhy::TruthNewer),
        ),
        (
            "embedder lost when ready",
            IndexState::Ready,
            IndexEvent::EmbedderLost(Gone),
            IndexState::LexicalOnly(Gone),
        ),
        (
            "embedder lost while building",
            IndexState::Building {
                done: c(1),
                total: c(2),
            },
            IndexEvent::EmbedderLost(Gone),
            IndexState::LexicalOnly(Gone),
        ),
        (
            "stale rebuilds",
            IndexState::Stale(StaleWhy::TruthNewer),
            IndexEvent::Begin { total: c(3) },
            IndexState::Building {
                done: c(0),
                total: c(3),
            },
        ),
        (
            "lexical-only rebuilds when the embedder returns",
            IndexState::LexicalOnly(Gone),
            IndexEvent::Begin { total: c(3) },
            IndexState::Building {
                done: c(0),
                total: c(3),
            },
        ),
        (
            "absent ignores a lost embedder",
            IndexState::Absent,
            IndexEvent::EmbedderLost(Gone),
            IndexState::Absent,
        ),
        (
            "stale ignores a changed card",
            IndexState::Stale(StaleWhy::FormatChanged),
            IndexEvent::CardChanged,
            IndexState::Stale(StaleWhy::FormatChanged),
        ),
    ];
    for (name, from, event, to) in cases {
        assert_eq!(step(from, event), to, "{name}");
    }
}

#[test]
fn fts5_is_compiled_in_and_the_schema_and_expression_work() {
    let conn = rusqlite::Connection::open_in_memory().expect("memory db");
    conn.execute_batch(SCHEMA_V1).expect("schema");
    conn.execute("INSERT INTO docs(text, id, kind, app, trust, at_s) VALUES (?1, 'f:1', 'fact', NULL, 'trusted', 0)", ["Prefers meetings after ten"]).expect("insert");
    conn.execute("INSERT INTO docs(text, id, kind, app, trust, at_s) VALUES (?1, 'f:2', 'fact', NULL, 'trusted', 0)", ["Lisbon receipts go to accounting"]).expect("insert");
    let expr = match_expression("receipts, (lisbon) AND\"quote").expect("expression");
    assert_eq!(expr, "\"receipts\" OR \"lisbon\" OR \"AND\" OR \"quote\"");
    let found: Vec<String> = conn
        .prepare("SELECT id FROM docs WHERE docs MATCH ?1 ORDER BY bm25(docs)")
        .expect("prepare")
        .query_map([expr], |r| r.get(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    assert_eq!(found, ["f:2"]);
    assert_eq!(match_expression("  ,, "), None);
}

#[tokio::test]
async fn the_fake_embedder_is_deterministic_and_word_sensitive() {
    let e = FakeEmbedder::new();
    let texts = vec![
        "lisbon receipts".to_owned(),
        "lisbon receipts".to_owned(),
        "quarterly roadmap".to_owned(),
    ];
    let v = e
        .embed(&texts, EmbedRole::Document, Urgency::Interactive)
        .await
        .expect("embed");
    assert_eq!(v[0], v[1]);
    assert_eq!(v[0].0.len(), usize::try_from(e.card().dims).expect("dims"));
    let near = Metric::Cosine.score(&v[0], &FakeEmbedder::vector("receipts from lisbon"));
    let far = Metric::Cosine.score(&v[0], &v[2]);
    assert!(near > far, "{near} vs {far}");
    assert_eq!(
        FakeEmbedder::unavailable()
            .embed(&texts, EmbedRole::Query, Urgency::Background)
            .await,
        Err(EmbedError::Unavailable)
    );
    assert_ne!(FakeEmbedder::named("other").card(), e.card());
}

#[test]
fn a_new_index_is_absent_and_allow_filters() {
    let conn = rusqlite::Connection::open_in_memory().expect("db");
    let card = FakeEmbedder::new().card().clone();
    let index = Index::new(
        Fts5::new(conn),
        ExactScan::new(rusqlite::Connection::open_in_memory().expect("db"), card),
    );
    assert_eq!(index.state(), IndexState::Absent);
    let allow = Allow::Only([id("a")].into());
    assert!(
        allow.permits(&id("a")) && !allow.permits(&id("b")) && Allow::Everything.permits(&id("b"))
    );
}
