//! `FakeEmbedder`: a deterministic embedder for tests and almanac-fake.

use crate::embed::{EmbedError, Embedder};
use crate::vector::{EmbedRole, EmbedderCard, MaxBatch, Metric, PromptPrefixes, Urgency, Vector};

/// Whether the fake answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Working,
    Unavailable,
}

/// A deterministic embedder: hashed bag-of-words into `DIMS` buckets, normalised. Texts that
/// share words are close; nothing else is promised.
#[derive(Debug, Clone)]
pub struct FakeEmbedder {
    card: EmbedderCard,
    mode: Mode,
}

/// The fake's vector length.
pub const FAKE_DIMS: u32 = 64;

impl FakeEmbedder {
    /// A working fake.
    pub fn new() -> Self {
        Self {
            card: EmbedderCard {
                model: "fake-hash-64".to_owned(),
                dims: FAKE_DIMS,
                max_tokens: 512,
                max_batch: MaxBatch(32),
                prompts: PromptPrefixes::default(),
                metric: Metric::Cosine,
            },
            mode: Mode::Working,
        }
    }

    /// A fake that always answers `EmbedError::Unavailable`.
    pub fn unavailable() -> Self {
        Self {
            mode: Mode::Unavailable,
            ..Self::new()
        }
    }

    /// A working fake whose card says it takes at most `max_batch` texts at once.
    pub fn with_max_batch(mut self, max_batch: u32) -> Self {
        self.card.max_batch = MaxBatch(max_batch);
        self
    }

    /// A working fake that claims to be another model, so an index built with one is stale for
    /// the other.
    pub fn named(model: &str) -> Self {
        let mut fake = Self::new();
        fake.card.model = model.to_owned();
        fake
    }

    /// The vector of one text.
    pub fn vector(text: &str) -> Vector {
        let mut buckets = vec![0.0f32; usize::try_from(FAKE_DIMS).unwrap_or(64)];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            let hash = word
                .to_lowercase()
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
                });
            let slot = usize::try_from(hash % u64::from(FAKE_DIMS)).unwrap_or(0);
            buckets[slot] += 1.0;
        }
        let norm = buckets.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            buckets.iter_mut().for_each(|x| *x /= norm);
        }
        Vector(buckets)
    }
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for FakeEmbedder {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    async fn embed(
        &self,
        texts: &[String],
        _role: EmbedRole,
        _urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        match self.mode {
            Mode::Unavailable => Err(EmbedError::Unavailable),
            Mode::Working => Ok(texts.iter().map(|t| Self::vector(t)).collect()),
        }
    }
}
