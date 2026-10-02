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
