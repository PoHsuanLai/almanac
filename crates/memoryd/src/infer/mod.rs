//! memoryd's two models, both through inferd (`org.quire.Inference1`, via porter-client):
//! embeddings for recall and drafts for consolidation. Each is written against porter-client's
//! `Transport` and `InferSession`, so tests drive them with a scripted session. When inferd is
//! not reachable the embedder answers `Unavailable` (the index stays lexical-only) and the
//! consolidator `Unavailable` (no run), as designed.

mod consolidate;
mod embed;
mod prompt;

pub use almanac_service::class_of;
pub use consolidate::InferdConsolidator;
pub use embed::InferdEmbedder;
pub use prompt::{parse_draft, render_prompt};

use porter_infer::{ClientFrame, InferEvent, InferReply, InferRequest, InferSession, SessionError};

/// Runs one turn: sends `request`, then reads events until the one that ends the turn. Progress
/// events (who answers, the engine loading, usage) are not the caller's concern here.
pub(crate) async fn turn(
    session: &mut impl InferSession,
    request: InferRequest,
) -> Result<InferReply, SessionError> {
    session.send(ClientFrame::Request(request)).await?;
    loop {
        if let InferEvent::Finished(reply) = session.next().await? {
            return Ok(reply);
        }
    }
}
