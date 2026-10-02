//! `FastembedEmbedder`: in-process ONNX embeddings (CPU, or CUDA when present) for recall.
//!
//! Kept out of `recall` and out of the gate: `ort` downloads binaries at build time, so this
//! crate is excluded from clippy and test. Check it by hand with network access:
//! `cargo check -p recall-fastembed`. A standalone or portable embedder; memoryd's default is
//! `InferdEmbedder`, which shares inferd's GPU queue.
//!
//! This embedder applies the model's role prefix itself (fastembed does not), batches to the
//! card's `max_batch`, checks every reply for count and width, and runs the blocking ONNX call on
//! a blocking thread. A `Background` call waits between batches while an `Interactive` one is in
//! flight. Tests here never download a model: the pure parts ([`plan`], [`check_reply`],
//! [`card_for`]) are tested with a fake engine, and the tests that load a real model are
//! `#[ignore]`d.

mod plan;

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use plan::{Priority, check_reply, plan};
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, RetryClass, Urgency, Vector};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use plan::card_for;

/// How long a background batch waits before looking again for interactive work.
const YIELD_FOR: Duration = Duration::from_millis(5);

/// The loaded model behind a lock (`TextEmbedding::embed` takes `&mut self`).
struct Engine(Mutex<TextEmbedding>);

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Engine(fastembed)")
    }
}

/// An embedder running a fastembed model in this process.
#[derive(Debug, Clone)]
pub struct FastembedEmbedder {
    card: EmbedderCard,
    engine: Arc<Engine>,
    priority: Priority,
}

impl FastembedEmbedder {
    /// Loads `model` (a fastembed model name, any case, e.g. `NomicEmbedTextV15`), downloading it
    /// on first use into fastembed's default cache.
    pub fn load(model: &str) -> Result<Self, EmbedError> {
        Self::open(model, None)
    }

    /// Like [`FastembedEmbedder::load`] with the model cache in `cache_dir`.
    pub fn load_in(model: &str, cache_dir: &Path) -> Result<Self, EmbedError> {
        Self::open(model, Some(cache_dir))
    }

    fn open(model: &str, cache_dir: Option<&Path>) -> Result<Self, EmbedError> {
        let name = EmbeddingModel::from_str(model).map_err(EmbedError::Refused)?;
        let info = TextEmbedding::get_model_info(&name).map_err(failed(RetryClass::Fatal))?;
        let card = card_for(model, info.dim);
        let options = match cache_dir {
            Some(dir) => TextInitOptions::new(name).with_cache_dir(dir.to_path_buf()),
            None => TextInitOptions::new(name),
        };
        let engine = TextEmbedding::try_new(options).map_err(failed(RetryClass::Retry))?;
        Ok(Self {
            card,
            engine: Arc::new(Engine(Mutex::new(engine))),
            priority: Priority::default(),
        })
    }

    /// The model's card.
    pub fn card_of(&self) -> &EmbedderCard {
        &self.card
    }

    async fn run_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbedError> {
        let engine = Arc::clone(&self.engine);
        tokio::task::spawn_blocking(move || {
            let mut model = engine.0.lock().map_err(|_| EmbedError::Failed {
                class: RetryClass::Fatal,
                why: "the model lock is poisoned".to_owned(),
            })?;
            model.embed(texts, None).map_err(failed(RetryClass::Retry))
        })
        .await
        .map_err(failed(RetryClass::Retry))?
    }
}

fn failed<E: std::fmt::Display>(class: RetryClass) -> impl Fn(E) -> EmbedError {
    move |e| EmbedError::Failed {
        class,
        why: e.to_string(),
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
        let _turn = self.priority.enter(urgency);
        let mut out = Vec::with_capacity(texts.len());
        for batch in plan(&self.card, role, texts) {
            while self.priority.must_wait(urgency) {
                tokio::time::sleep(YIELD_FOR).await;
            }
            let want = batch.len();
            let reply = self.run_batch(batch).await?;
            out.extend(check_reply(&self.card, want, reply)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
