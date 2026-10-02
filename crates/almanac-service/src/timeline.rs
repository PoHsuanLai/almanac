//! Timeline assembly: an event-log entry as the UI's row.

use almanac_core::{
    Count, EntryBody, EraseCause, EventRef, SpaceId, ThingView, TimelineEntry, UndoRef,
};
use eventlog::{BodyState, Entry};

/// The timeline row of `entry` in `space`. `erased_by` says why an erased body is gone and
/// `derived_facts` how many facts came from the event: the service knows both (the audit log
/// and the fact graph); the entry alone does not.
pub fn timeline_entry(
    space: &SpaceId,
    entry: &Entry,
    erased_by: EraseCause,
    derived_facts: Count,
) -> TimelineEntry {
    let h = &entry.header;
    let (things, body): (Vec<ThingView>, EntryBody) = match &entry.body {
        BodyState::Present(body) => (
            body.things()
                .into_iter()
                .map(|(view, _)| view.clone())
                .collect(),
            EntryBody::Present,
        ),
        BodyState::Erased => (Vec::new(), EntryBody::Erased { by: erased_by }),
    };
    TimelineEntry {
        event: EventRef {
            space: space.clone(),
            replica: h.replica,
            seq: h.seq,
        },
        occurred: h.occurred,
        actor: h.actor.clone(),
        kind: h.kind.clone(),
        effect: h.effect,
        label: h.label.clone(),
        things,
        body,
        derived_facts,
        undo: UndoRef::None,
    }
}
