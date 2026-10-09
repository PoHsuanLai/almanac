//! The embedder seam and the fake that tests use.

use crate::doc::ClassTag;
use crate::vector::{EmbedRole, EmbedderCard, Urgency, Vector};
use std::future::Future;

/// Whether trying again can help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RetryClass {
    /// Try again later (the engine is busy, loading or away): the index stays lexical-only
    /// meanwhile and the work is retried, not dropped.
    Retry,
    /// Trying again gives the same answer (a refusal, a text that is too long).
    Fatal,
}

/// Why texts could not be embedded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbedError {
    /// No embedder is reachable. Retry.
    #[error("embedder unavailable")]
    Unavailable,
    /// The embedder is busy with interactive work, or loading. Retry.
    #[error("embedder busy")]
    Busy,
    /// The embedder refused (a data-class floor, a cap). Fatal.
    #[error("embedder refused: {0}")]
    Refused(String),
    /// A text is longer than the model takes. Fatal.
    #[error("text too long")]
    TooLong,
    /// The embedder failed; the embedder says whether trying again can help.
    #[error("embedder failed: {why}")]
    Failed {
        /// Whether to try again.
        class: RetryClass,
        /// What happened.
        why: String,
    },
}

impl EmbedError {
    /// Whether trying again can help. `Index::upsert` marks the index `LexicalOnly` and queues a
    /// retry on `Retry`, and reports the document as unembeddable on `Fatal`.
    pub fn retry_class(&self) -> RetryClass {
        match self {
            EmbedError::Unavailable | EmbedError::Busy => RetryClass::Retry,
            EmbedError::Refused(_) | EmbedError::TooLong => RetryClass::Fatal,
            EmbedError::Failed { class, .. } => *class,
        }
    }
}

/// A text to embed with the data class it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classed {
    /// The class of the text (see [`ClassTag`]).
    pub class: ClassTag,
    /// The text.
    pub text: String,
}

/// Turns texts into vectors. Implementations: `FakeEmbedder` (feature `testing`),
/// `FastembedEmbedder` (recall-fastembed), `InferdEmbedder` (memoryd, over porter's inference
/// session).
pub trait Embedder: Send + Sync {
    /// Which vector space it produces, how many texts it takes at once and its prompt prefixes.
    fn card(&self) -> &EmbedderCard;
    /// One vector per text, in order, for texts of one `role` (the implementation puts the
    /// card's prefix in front, or leaves that to the engine that owns the model; never both).
    /// At most `card().max_batch` texts. Background requests yield to interactive ones.
    fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> impl Future<Output = Result<Vec<Vector>, EmbedError>> + Send;

    /// Like [`Embedder::embed`], for texts that each carry a data class. One vector per text, in
    /// input order, whatever the classes. The default ignores the classes (an embedder that
    /// runs on this computer has no floor to keep apart); one that sends texts elsewhere keeps
    /// each class in its own request.
    fn embed_classed(
        &self,
        texts: &[Classed],
        role: EmbedRole,
        urgency: Urgency,
    ) -> impl Future<Output = Result<Vec<Vector>, EmbedError>> + Send {
        async move {
            let plain: Vec<String> = texts.iter().map(|t| t.text.clone()).collect();
            self.embed(&plain, role, urgency).await
        }
    }
}
