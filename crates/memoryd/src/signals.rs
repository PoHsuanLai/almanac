//! The signals a reply implies, as a pure function: what the bus tells everyone after a request
//! (`Recorded`, `ConsolidationReady`) and what the daemon must look up first (the pending count
//! and the status, which the signals carry).

use almanac_core::{EventRef, FactState, MemoryReply, MemoryRequest, SpaceId};
use almanac_dbus::Signal;

/// What follows a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowUp {
    /// Emit this signal as it is.
    Emit(Signal),
    /// Count the Space's pending facts, then emit `PendingChanged`.
    PendingChanged(SpaceId),
    /// Read the Space's status, then emit `StatusChanged`.
    StatusChanged(SpaceId),
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn recorded(event: &EventRef, kind: &str) -> FollowUp {
    FollowUp::Emit(Signal::Recorded {
        space: event.space.to_string(),
        event_ref: json(event),
        kind: kind.to_owned(),
    })
}

/// The follow-ups of `reply` to `request`. A refusal changes nothing, so it implies none.
pub fn follow_ups(request: &MemoryRequest, reply: &MemoryReply) -> Vec<FollowUp> {
    use MemoryReply as P;
    use MemoryRequest as R;
    match (request, reply) {
        (R::Record(r), P::Recorded(event)) => vec![recorded(event, r.body.kind().as_str())],
        (R::RecordBatch(rs), P::RecordedBatch(event, _)) => rs
            .first()
            .map(|r| recorded(event, r.body.kind().as_str()))
            .into_iter()
            .collect(),
        (R::Propose(space, _), P::Proposed(_, FactState::Pending)) => {
            vec![FollowUp::PendingChanged(space.clone())]
        }
        (R::RunConsolidation(space), P::Consolidation(view)) => {
            vec![FollowUp::Emit(Signal::ConsolidationReady {
                space: space.to_string(),
                run: view.run.to_string(),
            })]
        }
        (R::Pause(space, _) | R::Resume(space) | R::Rebuild(space), P::Ok) => {
            vec![FollowUp::StatusChanged(space.clone())]
        }
        _ => Vec::new(),
    }
}
