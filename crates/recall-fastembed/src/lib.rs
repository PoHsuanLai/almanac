//! `FastembedEmbedder`: in-process ONNX embeddings (CPU, or CUDA when present) for recall.
//!
//! Kept out of `recall` and out of the gate: `ort` downloads binaries at build time, so this
//! crate is excluded from clippy and test. Check it by hand with network access:
//! `cargo check -p recall-fastembed`. A standalone or portable embedder; memoryd's default is
//! `InferdEmbedder`, which shares inferd's GPU queue.

use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, Urgency, Vector};

/// An embedder running a fastembed model in this process. A stub until the recall fill.
#[derive(Debug, Clone)]
pub struct FastembedEmbedder {
    card: EmbedderCard,
}

impl FastembedEmbedder {
    /// Loads `model` (a fastembed model name), downloading it on first use.
    pub fn load(model: &str) -> Result<Self, EmbedError> {
        let _ = model;
        todo!("fastembed::TextEmbedding::try_new for the named model; the card from its dimensions")
    }

    /// The model's card.
    pub fn card_of(&self) -> &EmbedderCard {
        &self.card
    }
}

impl Embedder for FastembedEmbedder {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        let _ = (texts, role, urgency);
        todo!(
            "TextEmbedding::embed on a blocking thread, the card's prefix for the role in front, at most max_batch texts; Background yields to Interactive"
        )
    }
}
