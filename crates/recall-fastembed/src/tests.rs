//! Pure tests: no model is loaded and nothing is downloaded.

use crate::plan::{Priority, card_for, check_reply, plan};
use recall::{EmbedError, EmbedRole, RetryClass, Urgency};

fn texts(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("t{i}")).collect()
}

#[test]
fn cards_carry_the_prefixes_the_model_was_trained_with() {
    let cases = [
        ("NomicEmbedTextV15", "search_query: ", "search_document: "),
        ("MultilingualE5Small", "query: ", "passage: "),
        (
            "BGESmallENV15",
            "Represent this sentence for searching relevant passages: ",
            "",
        ),
        ("AllMiniLML6V2", "", ""),
    ];
    for (model, query, document) in cases {
        let card = card_for(model, 384);
        assert_eq!(card.prompts.query, query, "{model}");
        assert_eq!(card.prompts.document, document, "{model}");
        assert_eq!(card.dims, 384);
    }
}

#[test]
fn the_plan_prefixes_by_role_and_never_exceeds_max_batch() {
    let card = card_for("NomicEmbedTextV15", 4);
    let max = usize::try_from(card.max_batch.0).expect("fits");
    let batches = plan(&card, EmbedRole::Document, &texts(max * 2 + 5));
    let sizes: Vec<usize> = batches.iter().map(Vec::len).collect();
    assert_eq!(sizes, vec![max, max, 5]);
    assert_eq!(batches[0][0], "search_document: t0");
    let flat: Vec<&String> = batches.iter().flatten().collect();
    assert_eq!(flat[max].as_str(), format!("search_document: t{max}"));
    let query = plan(&card, EmbedRole::Query, &texts(1));
    assert_eq!(query, vec![vec!["search_query: t0".to_owned()]]);
}

#[test]
fn an_empty_input_plans_no_batch() {
    let card = card_for("AllMiniLML6V2", 4);
    assert!(plan(&card, EmbedRole::Query, &[]).is_empty());
}

#[test]
fn a_reply_of_the_wrong_count_or_width_is_fatal() {
    let card = card_for("AllMiniLML6V2", 2);
    let ok = check_reply(&card, 2, vec![vec![1.0, 0.0], vec![0.0, 1.0]]).expect("matches");
    assert_eq!(ok.len(), 2);
    let short = check_reply(&card, 2, vec![vec![1.0, 0.0]]);
    let wide = check_reply(&card, 1, vec![vec![1.0, 0.0, 0.5]]);
    for error in [short, wide] {
        match error {
            Err(e @ EmbedError::Failed { .. }) => {
                assert_eq!(e.retry_class(), RetryClass::Fatal);
            }
            other => panic!("expected a fatal failure, got {other:?}"),
        }
    }
}

#[test]
fn background_waits_only_while_an_interactive_call_is_in_flight() {
    let gate = Priority::default();
    assert!(!gate.must_wait(Urgency::Background));
    let turn = gate.enter(Urgency::Interactive);
    assert!(gate.must_wait(Urgency::Background));
    assert!(!gate.must_wait(Urgency::Interactive));
    let other = gate.clone();
    assert!(other.must_wait(Urgency::Background));
    drop(turn);
    assert!(!gate.must_wait(Urgency::Background));
    let _background = gate.enter(Urgency::Background);
    assert!(!gate.must_wait(Urgency::Background));
}

#[test]
fn an_unknown_model_name_is_refused_without_a_download() {
    match crate::FastembedEmbedder::load("NoSuchModel") {
        Err(EmbedError::Refused(why)) => assert!(why.contains("NoSuchModel"), "{why}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "downloads a model (network, ~90 MB) and runs ONNX; run by hand with --ignored"]
async fn a_real_model_embeds_queries_and_documents_in_its_own_space() {
    use recall::Embedder;
    let cache = std::env::temp_dir().join("recall-fastembed-test-cache");
    let e = crate::FastembedEmbedder::load_in("AllMiniLML6V2", &cache).expect("load");
    let many = texts(70);
    let vectors = e
        .embed(&many, EmbedRole::Document, Urgency::Background)
        .await
        .expect("embed");
    assert_eq!(vectors.len(), 70);
    assert!(
        vectors
            .iter()
            .all(|v| v.0.len() == usize::try_from(e.card_of().dims).expect("fits"))
    );
}
