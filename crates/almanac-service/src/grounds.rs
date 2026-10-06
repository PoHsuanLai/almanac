//! What a proposed hunk still stands on. A run applied later (`memory.consolidation.apply =
//! review`) runs against a Space that may have changed since it drafted: an event may have been
//! forgotten or its body erased, a fact may be gone or already superseded. A hunk that cites
//! something no longer there is skipped, never applied, so a proposal cannot bring back what the
//! person forgot. Pure: the facts and events are passed in.

use almanac_core::{EventRef, Fact, FactId, Hunk, Link, SkipReason};
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
    /// The first thing `hunk` stands on that is gone, if any.
    pub(crate) fn why_not(&self, hunk: &Hunk) -> Option<SkipReason> {
        match hunk {
            Hunk::Promote { fact, .. } => self.fact_gap(fact),
            Hunk::Supersede { old, new } => (!self.active.contains(old))
                .then_some(SkipReason::ReplacedFactGone)
                .or_else(|| self.fact_gap(new)),
            Hunk::Flag { facts, .. } => facts
                .iter()
                .any(|f| !self.known.contains(f))
                .then_some(SkipReason::FactGone),
            Hunk::Tidy(_) | Hunk::Stamp { .. } | Hunk::ExternalEdit { .. } => None,
        }
    }

    fn fact_gap(&self, fact: &Fact) -> Option<SkipReason> {
        fact.links.iter().find_map(|l| match l {
            Link::Event(e) => (!self.events.contains(e)).then_some(SkipReason::EventGone),
            Link::Fact(id) => (!self.known.contains(id)).then_some(SkipReason::FactGone),
            Link::Thing(_) | Link::Run(_) => None,
        })
    }
}
