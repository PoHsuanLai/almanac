//! One open Space as the machines see it.

use crate::backend::Backend;
use crate::forget::{Plan, PlanState};
use almanac_core::{Marks, PlanToken, Record, SpaceMeta, SpaceState};
use memfiles_format::Store;
use std::collections::BTreeMap;

/// One open Space.
pub struct SpaceRuntime<B: Backend> {
    /// Its `spaces.toml` entry.
    pub meta: SpaceMeta,
    /// Where it is in its lifecycle.
    pub state: SpaceState,
    /// Its event log.
    pub log: B::Log,
    /// Its facts.
    pub store: Store<B::Files>,
    /// Its index.
    pub index: B::Index,
    /// Things marked "do not remember".
    pub marks: Marks,
    /// Records waiting while it is locked (at most `BUFFER_LIMIT`).
    pub buffer: Vec<Record>,
    /// Forget plans made and not yet applied or lapsed.
    pub plans: BTreeMap<PlanToken, (Plan, PlanState)>,
}

impl<B: Backend> std::fmt::Debug for SpaceRuntime<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpaceRuntime")
            .field("meta", &self.meta)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}
