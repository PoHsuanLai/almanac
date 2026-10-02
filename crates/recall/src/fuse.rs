//! Reciprocal-rank fusion of ranked lists, and the text chunker. Both pure.

use crate::doc::{Chunk, DocId, Ranked};
use std::collections::BTreeMap;

/// The RRF constant (60 is the usual choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RrfK(pub u32);

/// Why a hit matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitWhy {
    /// Full-text only.
    Lexical {
        /// Its rank in the lexical list.
        rank: u32,
    },
    /// Embedding only.
    Semantic {
        /// Its rank in the semantic list.
        rank: u32,
    },
    /// Both lists.
    Both {
        /// Its lexical rank.
        lexical: u32,
        /// Its semantic rank.
        semantic: u32,
    },
}

/// One fused result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fused {
    /// The document.
    pub id: DocId,
    /// `sum(1_000_000 / (k + rank))` over the lists it is in: integer arithmetic, so the order
    /// is deterministic everywhere.
    pub score_millionths: u32,
    /// Why it matched.
    pub why: HitWhy,
}

/// Fuses a lexical list (`lists[0]`) and a semantic list (`lists[1]`); lists beyond the second
/// are ignored. Best first; ties by id.
pub fn fuse_rrf(lists: &[Vec<Ranked>], k: RrfK) -> Vec<Fused> {
    let mut by_id: BTreeMap<&DocId, (u32, HitWhy)> = BTreeMap::new();
    for (position, list) in lists.iter().take(2).enumerate() {
        for hit in list {
            let share = 1_000_000 / k.0.saturating_add(hit.rank).max(1);
            let own = if position == 0 {
                HitWhy::Lexical { rank: hit.rank }
            } else {
                HitWhy::Semantic { rank: hit.rank }
            };
            by_id
                .entry(&hit.id)
                .and_modify(|(score, why)| {
                    *score = score.saturating_add(share);
                    if let HitWhy::Lexical { rank: lexical } = *why {
                        *why = HitWhy::Both {
                            lexical,
                            semantic: hit.rank,
                        };
                    }
                })
                .or_insert((share, own));
        }
    }
    let mut fused: Vec<Fused> = by_id
        .into_iter()
        .map(|(id, (score_millionths, why))| Fused {
            id: id.clone(),
            score_millionths,
            why,
        })
        .collect();
    fused.sort_by(|a, b| {
        b.score_millionths
            .cmp(&a.score_millionths)
            .then_with(|| a.id.cmp(&b.id))
    });
    fused
}

/// Characters per token the chunker assumes.
pub const CHARS_PER_TOKEN: u32 = 4;

/// Splits `text` into chunks of at most `max_tokens` (estimated at four characters a token),
/// on whitespace, never inside a word; a word longer than the limit is a chunk of its own.
/// Whitespace between words is normalised to single spaces.
pub fn chunk(text: &str, max_tokens: u32) -> Vec<Chunk> {
    let limit =
        usize::try_from(max_tokens.max(1).saturating_mul(CHARS_PER_TOKEN)).unwrap_or(usize::MAX);
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, chunks: &mut Vec<Chunk>| {
        if !current.is_empty() {
            let index = u32::try_from(chunks.len()).unwrap_or(u32::MAX);
            chunks.push(Chunk {
                index,
                text: std::mem::take(current),
            });
        }
    };
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > limit {
            flush(&mut current, &mut chunks);
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    flush(&mut current, &mut chunks);
    chunks
}
