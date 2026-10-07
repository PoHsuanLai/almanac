//! The embedder card and error contracts: prompt prefixes, batching, vector space, retry class.

use recall::*;

fn card(model: &str) -> EmbedderCard {
    EmbedderCard {
        model: model.into(),
        dims: 4,
        max_tokens: 8,
        max_batch: MaxBatch(3),
        prompts: PromptPrefixes {
            query: "search_query: ".into(),
            document: "search_document: ".into(),
        },
        metric: Metric::Cosine,
    }
}

#[test]
fn the_card_puts_the_role_prefix_in_front() {
    let c = card("a");
    assert_eq!(c.prefixed(EmbedRole::Query, "x"), "search_query: x");
    assert_eq!(c.prefixed(EmbedRole::Document, "x"), "search_document: x");
    let symmetric = EmbedderCard {
        prompts: PromptPrefixes::default(),
        ..c
    };
    assert_eq!(symmetric.prefixed(EmbedRole::Query, "x"), "x");
}

#[test]
fn batches_cover_every_text_at_most_max_batch_each() {
    let c = card("a");
    let cases: [(usize, Vec<usize>); 5] = [
        (0, vec![]),
        (1, vec![1]),
        (3, vec![3]),
        (7, vec![3, 3, 1]),
        (9, vec![3, 3, 3]),
    ];
    for (total, want) in cases {
        assert_eq!(c.batch_sizes(total), want, "{total}");
    }
    let zero = EmbedderCard {
        max_batch: MaxBatch(0),
        ..c
    };
    assert_eq!(zero.batch_sizes(2), vec![1, 1]);
}

#[test]
fn a_different_prefix_or_model_is_a_different_space_but_a_batch_size_is_not() {
    let a = card("a");
    let other_prefix = EmbedderCard {
        prompts: PromptPrefixes::default(),
        ..a.clone()
    };
    let other_batch = EmbedderCard {
        max_batch: MaxBatch(99),
        max_tokens: 1,
        ..a.clone()
    };
    assert_eq!(a.space_vs(&a), SpaceCheck::Same);
    assert_eq!(a.space_vs(&other_batch), SpaceCheck::Same);
    assert_eq!(a.space_vs(&other_prefix), SpaceCheck::Different);
    assert_eq!(a.space_vs(&card("b")), SpaceCheck::Different);
}

#[test]
fn embed_errors_say_whether_to_retry() {
    let cases = [
        (EmbedError::Unavailable, RetryClass::Retry),
        (EmbedError::Busy, RetryClass::Retry),
        (EmbedError::Refused("floor".into()), RetryClass::Fatal),
        (EmbedError::TooLong, RetryClass::Fatal),
        (
            EmbedError::Failed {
                class: RetryClass::Retry,
                why: "503".into(),
            },
            RetryClass::Retry,
        ),
        (
            EmbedError::Failed {
                class: RetryClass::Fatal,
                why: "bad dims".into(),
            },
            RetryClass::Fatal,
        ),
    ];
    for (error, want) in cases {
        assert_eq!(error.retry_class(), want, "{error}");
    }
}

/// Records what `embed_classed` was given, and answers one vector per text.
#[derive(Debug, Default)]
struct Recording {
    classed: std::sync::Mutex<Vec<Vec<(String, String)>>>,
    plain: std::sync::Mutex<Vec<Vec<String>>>,
    keeps_classes: bool,
}

impl Embedder for Recording {
    fn card(&self) -> &EmbedderCard {
        static CARD: std::sync::OnceLock<EmbedderCard> = std::sync::OnceLock::new();
        CARD.get_or_init(|| EmbedderCard {
            max_batch: MaxBatch(32),
            ..card("recording")
        })
    }

    async fn embed(
        &self,
        texts: &[String],
        _role: EmbedRole,
        _urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        self.plain.lock().expect("lock").push(texts.to_vec());
        Ok(texts
            .iter()
            .map(|_| Vector(vec![1.0, 0.0, 0.0, 0.0]))
            .collect())
    }

    async fn embed_classed(
        &self,
        texts: &[Classed],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        if !self.keeps_classes {
            let plain: Vec<String> = texts.iter().map(|t| t.text.clone()).collect();
            return self.embed(&plain, role, urgency).await;
        }
        self.classed.lock().expect("lock").push(
            texts
                .iter()
                .map(|t| (t.class.0.clone(), t.text.clone()))
                .collect(),
        );
        Ok(texts
            .iter()
            .map(|_| Vector(vec![1.0, 0.0, 0.0, 0.0]))
            .collect())
    }
}

fn doc_with_class(id: &str, text: &str, class: &str) -> Doc {
    Doc {
        id: DocId(id.into()),
        text: text.into(),
        at: 0,
        facets: Facets {
            kind: "fact".into(),
            app: None,
            trust: TrustTier::Trusted,
        },
        class: ClassTag(class.into()),
    }
}

fn new_index(card: &EmbedderCard) -> Index<ExactScan> {
    let fts = Fts5::new(rusqlite::Connection::open_in_memory().expect("db"));
    fts.create().expect("schema");
    Index::new(fts, ExactScan::in_memory(card.clone()).expect("vectors"))
}

#[tokio::test]
async fn the_index_hands_every_text_to_the_embedder_with_its_documents_class() {
    let embedder = Recording {
        keeps_classes: true,
        ..Recording::default()
    };
    let mut index = new_index(embedder.card());
    index
        .upsert(
            &[
                doc_with_class("f:1", "alpha", "mail"),
                doc_with_class("f:2", "beta", ""),
                doc_with_class("f:3", "gamma", "notes"),
            ],
            &embedder,
        )
        .await
        .expect("upsert");
    let seen = embedder.classed.lock().expect("lock").clone();
    let pair = |c: &str, t: &str| (c.to_owned(), t.to_owned());
    assert_eq!(
        seen,
        vec![vec![
            pair("mail", "alpha"),
            pair("", "beta"),
            pair("notes", "gamma")
        ]]
    );
    assert!(embedder.plain.lock().expect("lock").is_empty());
}

#[tokio::test]
async fn an_embedder_that_ignores_classes_still_gets_the_texts_in_order() {
    // The default `embed_classed` is `embed` without the classes: a local embedder needs nothing
    // more, and a query (which has no class) always goes through `embed`.
    let embedder = Recording::default();
    let mut index = new_index(embedder.card());
    index
        .upsert(
            &[
                doc_with_class("f:1", "alpha", "mail"),
                doc_with_class("f:2", "beta", "notes"),
            ],
            &embedder,
        )
        .await
        .expect("upsert");
    assert_eq!(
        embedder.plain.lock().expect("lock").clone(),
        vec![vec!["alpha".to_owned(), "beta".to_owned()]]
    );
    let direct = FakeEmbedder::new();
    let classed = [Classed {
        class: ClassTag("mail".into()),
        text: "alpha".into(),
    }];
    assert_eq!(
        direct
            .embed_classed(&classed, EmbedRole::Document, Urgency::Background)
            .await,
        direct
            .embed(
                &["alpha".to_owned()],
                EmbedRole::Document,
                Urgency::Background
            )
            .await
    );
}
