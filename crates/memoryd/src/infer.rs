//! memoryd's two models, both through inferd (`org.quire.Inference1`, via porter-client):
//! embeddings for recall and drafts for consolidation. Stubs until porter's `DbusTransport::open`
//! and session fills land (FINDINGS.md).

use almanac_service::{ConsolidateError, ConsolidationInput, Consolidator, Draft};
use porter_client::Transport;
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, Urgency, Vector};
use std::sync::Arc;

/// Embeds through an inferd session: `Need::Embeddings`, `InferRequest::Embed` with
/// `Usage::Background` for indexing and the document's `DataClass`, so mail text keeps its
/// on-device floor. The card's model comes from `ServedBy.model` of the first reply.
#[derive(Debug)]
pub struct InferdEmbedder<T: Transport> {
    transport: Arc<T>,
    card: EmbedderCard,
}

impl<T: Transport> InferdEmbedder<T> {
    /// Embeds over `transport`; `card` is the configured embedder (replaced by what inferd
    /// reports once a reply names the model).
    pub fn new(transport: Arc<T>, card: EmbedderCard) -> Self {
        Self { transport, card }
    }
}

impl<T: Transport> Embedder for InferdEmbedder<T> {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        let _ = (&self.transport, texts, role, urgency);
        todo!(
            "Transport::open(Need embeddings, class, tier), send InferRequest::Embed with `EmbedRequest.role` mapped from `EmbedRole` (inferd puts the model's prefix in front; the card's prefixes are read from `EmbedCap.prompts`), read the Finished(Embed) reply; Interactive vs Background maps to porter's Usage; a refusal is EmbedError::Refused, an absent engine Unavailable, a rate limit or loading engine Busy"
        )
    }
}

/// Drafts a consolidation through an inferd chat session (`Task::Extract`, background).
#[derive(Debug)]
pub struct InferdConsolidator<T: Transport> {
    transport: Arc<T>,
}

impl<T: Transport> InferdConsolidator<T> {
    /// Drafts over `transport`.
    pub fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }
}

impl<T: Transport> Consolidator for InferdConsolidator<T> {
    async fn draft(&self, input: ConsolidationInput) -> Result<Draft, ConsolidateError> {
        let _ = (&self.transport, input);
        todo!(
            "render the input as the consolidation prompt, open a session, parse the reply into hunks (Unparseable on anything else)"
        )
    }
}
