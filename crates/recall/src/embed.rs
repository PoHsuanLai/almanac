//! The embedder seam and the fake that tests use.

use crate::vector::{EmbedderCard, Urgency, Vector};
use std::future::Future;

/// Why texts could not be embedded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbedError {
    /// No embedder is reachable.
    #[error("embedder unavailable")]
    Unavailable,
    /// The embedder refused (a data-class floor, a cap).
    #[error("embedder refused: {0}")]
    Refused(String),
    /// A text is longer than the model takes.
    #[error("text too long")]
    TooLong,
    /// The embedder failed.
    #[error("embedder failed: {0}")]
    Failed(String),
}

/// Turns texts into vectors. Implementations: `FakeEmbedder` (feature `testing`),
/// `FastembedEmbedder` (recall-fastembed), `InferdEmbedder` (memoryd, over porter's inference
/// session).
pub trait Embedder: Send + Sync {
    /// Which vector space it produces.
    fn card(&self) -> &EmbedderCard;
    /// One vector per text, in order. Background requests yield to interactive ones.
    fn embed(
        &self,
        texts: &[String],
        urgency: Urgency,
    ) -> impl Future<Output = Result<Vec<Vector>, EmbedError>> + Send;
}
