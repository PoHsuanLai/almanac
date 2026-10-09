//! Reciprocal-rank fusion and the text chunker: defined in `almanac-store`, re-exported here.

pub use almanac_store::{CHARS_PER_TOKEN, Fused, HitWhy, RrfK, chunk, fuse_rrf};
