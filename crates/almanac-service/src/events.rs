//! What the service says happened that no reply says: a pending fact settled or aged out, a
//! Space whose key was lost or came back. memoryd drains them after each request and on its
//! timers and turns them into signals (`StatusChanged`, `PendingChanged`).

use almanac_core::{ChainHealth, Count, IndexView, SpaceId, SpaceState, SpaceStatus};

/// Something the bus should be told about.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceEvent {
    /// The Space's pending facts changed (settled, or aged out): count them.
    PendingChanged(SpaceId),
    /// The Space is open again, or its state changed in a way its own status shows: read it.
    StatusChanged(SpaceId),
    /// A consolidation run was applied or discarded: its `Consolidation` view changed.
    ConsolidationChanged(SpaceId, almanac_core::RunId),
    /// The Space's key is gone: its status is [`locked_status`] (nothing else can be read).
    Locked(SpaceId),
}

/// The status of a Space that cannot be opened: locked, nothing counted.
pub fn locked_status() -> SpaceStatus {
    SpaceStatus {
        state: SpaceState::Locked,
        index: IndexView::Absent,
        chain: ChainHealth::Unchecked,
        usage: almanac_core::Bytes(0),
        events: Count(0),
        facts: Count(0),
        pending: Count(0),
        last_run: None,
    }
}
