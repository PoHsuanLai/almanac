//! What a reply changed, as a pure function: the facts about memory that anyone listening
//! should hear after a request. How they are told (a bus signal, a callback) is the host's
//! business; the ones that need a lookup first (the pending count, the status) are named, not
//! answered.

use almanac_core::{EventRef, FactState, MemoryReply, MemoryRequest, RunId, SpaceId};

/// One thing that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Change {
    /// An event was stored; `kind` is its kind tag.
    Recorded {
        /// The stored event.
        event: EventRef,
        /// The kind tag of its body.
        kind: String,
    },
    /// A consolidation run has a draft for the person.
    ConsolidationReady {
        /// The Space it ran in.
        space: SpaceId,
        /// The run.
        run: RunId,
    },
    /// The Space's pending facts changed; the host counts them.
    PendingChanged(SpaceId),
    /// The Space's status changed; the host reads it.
    StatusChanged(SpaceId),
}

fn recorded(event: &EventRef, kind: &str) -> Change {
    Change::Recorded {
        event: event.clone(),
        kind: kind.to_owned(),
    }
}

/// What `reply` to `request` changed. A refusal changes nothing, so it implies none.
pub fn changes(request: &MemoryRequest, reply: &MemoryReply) -> Vec<Change> {
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
            vec![Change::PendingChanged(space.clone())]
        }
        (R::RunConsolidation(space), P::Consolidation(view)) => {
            vec![Change::ConsolidationReady {
                space: space.clone(),
                run: view.run.clone(),
            }]
        }
        (R::Pause(space, _) | R::Resume(space) | R::Rebuild(space), P::Ok) => {
            vec![Change::StatusChanged(space.clone())]
        }
        _ => Vec::new(),
    }
}
