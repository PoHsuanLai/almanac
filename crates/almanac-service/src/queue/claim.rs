//! What a request needs exclusive use of.

use crate::auth::{Allowed, allowed};
use almanac_core::{Caller, EventBody, MemoryRequest, Record, SpaceId};
use std::collections::BTreeSet;

/// What a request needs exclusive use of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// Nothing: it only reads the service's lists (`Spaces`, `Rules`) or the settings
    /// (`PendingSettings`).
    Nothing,
    /// These Spaces.
    Spaces(BTreeSet<SpaceId>),
    /// Every Space.
    Everything,
}

impl Claim {
    /// Whether two requests with these claims may not run at the same time.
    pub(super) fn conflicts(&self, other: &Claim) -> bool {
        match (self, other) {
            (Claim::Nothing, _) | (_, Claim::Nothing) => false,
            (Claim::Everything, _) | (_, Claim::Everything) => true,
            (Claim::Spaces(a), Claim::Spaces(b)) => !a.is_disjoint(b),
        }
    }
}

/// The Space a record lands in: a message goes to the receiver's Space.
fn landing(record: &Record) -> SpaceId {
    match &record.body {
        EventBody::Message(m) => m.to.space.clone(),
        _ => record.space.clone(),
    }
}

/// What `request` must hold. Pure, so it is a table.
pub fn claim_of(request: &MemoryRequest) -> Claim {
    use MemoryRequest as R;
    match request {
        R::Spaces | R::Rules | R::PendingSettings => Claim::Nothing,
        R::Record(r) => Claim::Spaces(BTreeSet::from([landing(r)])),
        R::RecordBatch(rs) => Claim::Spaces(rs.iter().map(landing).collect()),
        R::Forget(_)
        | R::Settle(..)
        | R::Revert(_)
        | R::ApplyConsolidation(_)
        | R::DiscardConsolidation(_)
        | R::SetRule(_)
        | R::RemoveRule(_)
        | R::RemoveSpace(..)
        | R::ConfirmSettings(_)
        | R::Export(_) => Claim::Everything,
        other => other.space().map_or(Claim::Everything, |s| {
            Claim::Spaces(BTreeSet::from([s.clone()]))
        }),
    }
}

/// What `request` from `caller` must hold: nothing when the service will refuse it anyway, so a
/// refused call never waits for, or stalls, the queue (ALM-20). Pure, so it is a table.
pub fn claim_for(caller: &Caller, request: &MemoryRequest) -> Claim {
    claim_under(&allowed(caller, request), request)
}

/// [`claim_for`] once the verdict is known.
pub(super) fn claim_under(verdict: &Allowed, request: &MemoryRequest) -> Claim {
    match verdict {
        Allowed::Yes => claim_of(request),
        Allowed::No(_) => Claim::Nothing,
    }
}
