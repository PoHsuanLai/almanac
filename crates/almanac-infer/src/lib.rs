//! The two models almanac needs from inferd (`org.quire.Inference1`, via porter-client):
//! embeddings for recall ([`InferdEmbedder`]) and drafts for consolidation
//! ([`InferdConsolidator`]). Each is written against porter-client's `Transport` and
//! `InferSession`, so a test drives it with a scripted session and an app that is not the memory
//! daemon (mailo) can use it over its own transport. When inferd is not reachable the embedder
//! answers `Unavailable` (the index stays lexical-only) and the consolidator `Unavailable` (no
//! run), as designed.

mod consolidate;
mod deadline;
mod embed;

pub use consolidate::InferdConsolidator;
pub use deadline::Deadlines;
pub use embed::InferdEmbedder;
