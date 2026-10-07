//! The defaults for the two models an app may not have.

use almanac_service::{ConsolidateError, ConsolidationInput, Consolidator, Draft};
use recall::{
    EmbedError, EmbedRole, Embedder, EmbedderCard, MaxBatch, Metric, PromptPrefixes, Urgency,
    Vector,
};

/// No embedder: every request says it is unavailable, so the index stays lexical-only (FTS5)
/// and recall answers by keyword. The exact vector scan has nothing to scan.
#[derive(Debug, Clone)]
pub struct NoEmbedder {
    card: EmbedderCard,
}

impl Default for NoEmbedder {
    fn default() -> Self {
        Self {
            card: EmbedderCard {
                model: "none".to_owned(),
                dims: 1,
                max_tokens: 1,
                max_batch: MaxBatch(1),
                prompts: PromptPrefixes::default(),
                metric: Metric::Cosine,
            },
        }
    }
}

impl Embedder for NoEmbedder {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    async fn embed(
        &self,
        _texts: &[String],
        _role: EmbedRole,
        _urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        Err(EmbedError::Unavailable)
    }
}

/// No consolidation model: a run fails with `Unavailable` and changes nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoConsolidator;

impl Consolidator for NoConsolidator {
    async fn draft(&self, _input: ConsolidationInput) -> Result<Draft, ConsolidateError> {
        Err(ConsolidateError::Unavailable)
    }
}
