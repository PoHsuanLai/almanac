//! Vectors, embedder cards, and the pure scoring and encoding of exact scan.

use crate::doc::{DocId, Ranked, TopK};

/// An embedding. Embeddings are floats end to end, so this is `PartialEq` without `Eq`.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector(pub Vec<f32>);

/// How two vectors are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    /// Cosine similarity.
    Cosine,
    /// Dot product (for normalised vectors).
    Dot,
}

/// How urgent an embedding request is: indexing yields to the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Urgency {
    /// The person is waiting.
    Interactive,
    /// Indexing in the background.
    Background,
}

/// Which vector space an embedder produces: an index built in one is useless in another.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmbedderCard {
    /// The model's name.
    pub model: String,
    /// The vector length.
    pub dims: u32,
    /// The longest input in tokens.
    pub max_tokens: u32,
    /// How vectors are compared.
    pub metric: Metric,
}

impl Vector {
    /// The little-endian `f32` bytes stored in the index's BLOB column.
    pub fn to_blob(&self) -> Vec<u8> {
        self.0.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// The vector stored by [`Vector::to_blob`], or `None` if the length is not a whole number
    /// of floats.
    pub fn from_blob(blob: &[u8]) -> Option<Vector> {
        if !blob.len().is_multiple_of(4) {
            return None;
        }
        let (floats, _) = blob.as_chunks::<4>();
        Some(Vector(
            floats.iter().map(|b| f32::from_le_bytes(*b)).collect(),
        ))
    }
}

impl Metric {
    /// How alike `a` and `b` are; larger is closer. Vectors of different length score 0.
    pub fn score(self, a: &Vector, b: &Vector) -> f32 {
        if a.0.len() != b.0.len() {
            return 0.0;
        }
        let dot: f32 = a.0.iter().zip(&b.0).map(|(x, y)| x * y).sum();
        match self {
            Metric::Dot => dot,
            Metric::Cosine => {
                let norm = |v: &Vector| v.0.iter().map(|x| x * x).sum::<f32>().sqrt();
                let denom = norm(a) * norm(b);
                if denom == 0.0 { 0.0 } else { dot / denom }
            }
        }
    }
}

/// The `k` items nearest to `query` by `metric`, best first, ties by id. This is what
/// `ExactScan::nearest` does over its rows.
pub fn nearest_exact(
    query: &Vector,
    items: &[(DocId, Vector)],
    metric: Metric,
    k: TopK,
) -> Vec<Ranked> {
    let mut scored: Vec<(f32, &DocId)> = items
        .iter()
        .map(|(id, v)| (metric.score(query, v), id))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored
        .into_iter()
        .take(usize::try_from(k.0).unwrap_or(usize::MAX))
        .enumerate()
        .map(|(i, (_, id))| Ranked {
            id: id.clone(),
            rank: u32::try_from(i + 1).unwrap_or(u32::MAX),
        })
        .collect()
}
