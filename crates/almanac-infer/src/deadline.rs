//! How long each stage of an inferd exchange may take, and the turn that runs within it.

use porter_infer::{ClientFrame, InferEvent, InferReply, InferRequest, InferSession, SessionError};
use std::future::Future;
use std::time::Duration;

/// How long each stage of an inferd exchange may take before it is given up. An exchange that
/// outlives its deadline is `Unavailable`, so the Space queue that waits on it is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadlines {
    /// Finding a model and opening the session (inferd may be starting).
    pub open: Duration,
    /// The turn: from sending the request to the event that ends it.
    pub turn: Duration,
}

impl Deadlines {
    /// For embedding: a batch of vectors is quick.
    pub const EMBED: Deadlines = Deadlines {
        open: Duration::from_secs(60),
        turn: Duration::from_secs(60),
    };
    /// For a consolidation draft: a local model reading a day of events is slow.
    pub const DRAFT: Deadlines = Deadlines {
        open: Duration::from_secs(60),
        turn: Duration::from_secs(15 * 60),
    };
}

/// A stage ran out of time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("inferd did not answer in time")]
pub(crate) struct TimedOut;

/// Runs `work`, giving it up after `limit`.
pub(crate) async fn within<T>(
    limit: Duration,
    work: impl Future<Output = T>,
) -> Result<T, TimedOut> {
    tokio::time::timeout(limit, work)
        .await
        .map_err(|_| TimedOut)
}

/// Why a turn did not finish.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TurnError {
    /// The session failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// The turn outlived its deadline.
    #[error(transparent)]
    TimedOut(#[from] TimedOut),
}

/// Runs one turn: sends `request`, then reads events until the one that ends the turn, all
/// within `limit`. Progress events (who answers, the engine loading, usage) are not the
/// caller's concern here.
pub(crate) async fn turn(
    session: &mut impl InferSession,
    request: InferRequest,
    limit: Duration,
) -> Result<InferReply, TurnError> {
    let exchange = async {
        session.send(ClientFrame::Request(request)).await?;
        loop {
            if let InferEvent::Finished(reply) = session.next().await? {
                return Ok::<InferReply, SessionError>(reply);
            }
        }
    };
    Ok(within(limit, exchange).await??)
}
