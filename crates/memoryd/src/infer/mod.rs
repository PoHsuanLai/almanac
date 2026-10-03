//! memoryd's two models, both through inferd (`org.quire.Inference1`, via porter-client):
//! embeddings for recall and drafts for consolidation. Each is written against porter-client's
//! `Transport` and `InferSession`, so tests drive them with a scripted session.

mod consolidate;
mod embed;
mod prompt;

pub use consolidate::InferdConsolidator;
pub use embed::InferdEmbedder;
pub use prompt::{class_of, parse_draft, render_prompt};

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

/// No inference on this machine (yet): every `open` says inferd is unreachable, so the embedder
/// answers `Unavailable` (the index stays lexical-only) and consolidation is unavailable. This is
/// the link `main` uses until porter-client's transports are filled.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoInference;

/// The session of [`NoInference`]: never opened.
#[derive(Debug)]
pub enum NoSession {}

impl InferSession for NoSession {
    async fn send(&mut self, _frame: ClientFrame) -> Result<(), SessionError> {
        match *self {}
    }

    async fn next(&mut self) -> Result<InferEvent, SessionError> {
        match *self {}
    }
}

impl porter_client::Transport for NoInference {
    type Session = NoSession;

    async fn call(
        &self,
        _request: porter_core::AccountsRequest,
    ) -> Result<porter_core::AccountsReply, porter_client::TransportError> {
        Err(porter_client::TransportError::Unreachable)
    }

    async fn open_with(
        &self,
        _need: &porter_core::Need,
        _class: porter_core::DataClass,
        _tier: porter_core::Tier,
        _options: &porter_infer::OpenOptions,
    ) -> Result<NoSession, porter_client::TransportError> {
        Err(porter_client::TransportError::Unreachable)
    }
}
