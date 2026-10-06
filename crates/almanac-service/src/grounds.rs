//! What a proposed hunk still stands on. A run applied later (`memory.consolidation.apply =
//! review`) runs against a Space that may have changed since it drafted: an event may have been
//! forgotten or its body erased, a fact may be gone or already superseded. A hunk that cites
//! something no longer there is skipped, never applied, so a proposal cannot bring back what the
//! person forgot. Pure: the facts and events are passed in.

use almanac_core::{EventRef, Fact, FactId, Hunk, Link};
use std::collections::BTreeSet;

/// The events (with a readable body) and facts the Space holds now.
#[derive(Debug, Default)]
pub(crate) struct Grounds {
    /// Events whose body is still present.
    pub events: BTreeSet<EventRef>,
    /// Facts that are active (not superseded, not pending).
    pub active: BTreeSet<FactId>,
    /// Facts of any state.
    pub known: BTreeSet<FactId>,
}

impl Grounds {
    /// Whether everything `hunk` cites or replaces is still there.
    pub(crate) fn holds(&self, hunk: &Hunk) -> bool {
        match hunk {
            Hunk::Promote { fact, .. } => self.fact_holds(fact),
            Hunk::Supersede { old, new } => self.active.contains(old) && self.fact_holds(new),
            Hunk::Flag { facts, .. } => facts.iter().all(|f| self.known.contains(f)),
            Hunk::Tidy(_) | Hunk::Stamp { .. } | Hunk::ExternalEdit { .. } => true,
        }
    }

    fn fact_holds(&self, fact: &Fact) -> bool {
        fact.links.iter().all(|l| match l {
            Link::Event(e) => self.events.contains(e),
            Link::Fact(id) => self.known.contains(id),
            Link::Thing(_) | Link::Run(_) => true,
        })
    }
}
