//! The pure half of the embedder: model cards, batching with role prefixes, reply checks and the
//! interactive-first gate. No model, no network.

use recall::{
    EmbedError, EmbedRole, EmbedderCard, MaxBatch, Metric, PromptPrefixes, RetryClass, Urgency,
    Vector,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// The longest input fastembed truncates to by default.
const MAX_TOKENS: u32 = 512;
/// How many texts one ONNX call takes.
const MAX_BATCH: u32 = 64;

/// The card for fastembed model `model` of `dims` floats. The prefixes are the ones the model
/// was trained with (nomic: `search_query: `, e5: `query: `, bge-en: an instruction on queries);
/// other models are symmetric. fastembed pools and normalises, so vectors compare by cosine.
pub fn card_for(model: &str, dims: usize) -> EmbedderCard {
    let lower = model.to_ascii_lowercase();
    let (query, document) = if lower.contains("nomic") {
        ("search_query: ", "search_document: ")
    } else if lower.contains("e5") {
        ("query: ", "passage: ")
    } else if lower.starts_with("bge") && lower.contains("en") {
        (
            "Represent this sentence for searching relevant passages: ",
            "",
        )
    } else {
        ("", "")
    };
    EmbedderCard {
        model: model.to_owned(),
        dims: u32::try_from(dims).unwrap_or(u32::MAX),
        max_tokens: MAX_TOKENS,
        max_batch: MaxBatch(MAX_BATCH),
        prompts: PromptPrefixes {
            query: query.to_owned(),
            document: document.to_owned(),
        },
        metric: Metric::Cosine,
    }
}

/// The batches to send: every text prefixed for `role`, in order, at most `max_batch` each.
pub fn plan(card: &EmbedderCard, role: EmbedRole, texts: &[String]) -> Vec<Vec<String>> {
    let mut rest = texts.iter().map(|t| card.prefixed(role, t));
    card.batch_sizes(texts.len())
        .into_iter()
        .map(|size| rest.by_ref().take(size).collect())
        .collect()
}

/// A reply as vectors, or a fatal error when the count or a width is not what was asked for: a
/// wrong-width vector must never reach the index.
pub fn check_reply(
    card: &EmbedderCard,
    want: usize,
    reply: Vec<Vec<f32>>,
) -> Result<Vec<Vector>, EmbedError> {
    let width = usize::try_from(card.dims).unwrap_or(usize::MAX);
    if reply.len() != want {
        return Err(fatal(format!(
            "asked for {want} embeddings, got {}",
            reply.len()
        )));
    }
    match reply.iter().find(|v| v.len() != width) {
        Some(bad) => Err(fatal(format!(
            "expected vectors of {width} floats, got {}",
            bad.len()
        ))),
        None => Ok(reply.into_iter().map(Vector).collect()),
    }
}

fn fatal(why: String) -> EmbedError {
    EmbedError::Failed {
        class: RetryClass::Fatal,
        why,
    }
}

/// How many interactive calls are in flight, shared by the clones of one embedder.
#[derive(Debug, Clone, Default)]
pub struct Priority(Arc<AtomicU32>);

/// An interactive call in flight; dropping it ends it.
#[derive(Debug)]
pub struct Turn(Option<Arc<AtomicU32>>);

impl Priority {
    /// Starts a call; an interactive one holds the gate until the returned turn is dropped.
    pub fn enter(&self, urgency: Urgency) -> Turn {
        match urgency {
            Urgency::Interactive => {
                self.0.fetch_add(1, Ordering::SeqCst);
                Turn(Some(Arc::clone(&self.0)))
            }
            Urgency::Background => Turn(None),
        }
    }

    /// Whether a call of `urgency` must wait: only background ones, and only while an
    /// interactive call is in flight.
    pub fn must_wait(&self, urgency: Urgency) -> bool {
        match urgency {
            Urgency::Interactive => false,
            Urgency::Background => self.0.load(Ordering::SeqCst) > 0,
        }
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Some(count) = &self.0 {
            count.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
