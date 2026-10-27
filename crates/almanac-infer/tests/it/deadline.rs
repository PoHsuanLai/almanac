//! A session that never answers is given up at its deadline, for both adapters.

use crate::support::{Scripted, card, texts};
use almanac_fake::consolidation_input;
use almanac_infer::{Deadlines, InferdConsolidator, InferdEmbedder};
use almanac_store::{EmbedError, EmbedRole, Embedder, Urgency};
use almanac_tidy::{ConsolidateError, Consolidator};

#[tokio::test]
async fn a_session_that_never_answers_is_given_up_at_its_deadline() {
    let quick = Deadlines {
        open: std::time::Duration::from_secs(5),
        turn: std::time::Duration::from_millis(50),
    };
    let embedder = InferdEmbedder::new(Scripted::hanging(), card()).with_deadlines(quick);
    let embedded = embedder
        .embed(&texts(1), EmbedRole::Document, Urgency::Background)
        .await;
    assert_eq!(embedded, Err(EmbedError::Unavailable));
    let drafted = InferdConsolidator::new(Scripted::hanging())
        .with_deadlines(quick)
        .draft(consolidation_input())
        .await;
    assert_eq!(
        drafted.expect_err("gives up"),
        ConsolidateError::Unavailable
    );
}
