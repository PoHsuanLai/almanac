//! `MemoryService`: memoryd's core, over a [`Backend`]. Frozen shape; `handle` and `export` are
//! `todo!()` until fill wave 2 (FINDINGS.md), built from the pure parts in this crate.

use crate::auth::allowed;
use crate::backend::Backend;
use crate::forget::Plan;
use crate::forget::PlanState;
use almanac_core::{
    Caller, ExportOptions, Marks, MemoryReply, MemoryRequest, PlanToken, Record, RuleSet,
    SpaceMeta, SpaceState,
};
use memfiles::Store;
use recall::Index;
use std::collections::BTreeMap;
use std::future::Future;
use std::io::Write;
use std::sync::Mutex;

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
    pub index: Index<B::Vectors>,
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

/// memoryd's core: every request goes through [`allowed`], then the machines.
pub struct MemoryService<B: Backend> {
    backend: B,
    rules: Mutex<RuleSet>,
    spaces: Mutex<BTreeMap<almanac_core::SpaceId, SpaceRuntime<B>>>,
}

impl<B: Backend> std::fmt::Debug for MemoryService<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryService").finish_non_exhaustive()
    }
}

impl<B: Backend> MemoryService<B> {
    /// A service over `backend` with the person's rules. No Space is open yet.
    pub fn new(backend: B, rules: RuleSet) -> Self {
        Self {
            backend,
            rules: Mutex::new(rules),
            spaces: Mutex::new(BTreeMap::new()),
        }
    }

    /// The backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Answers one request from `caller`. Never fails: a refusal is a [`MemoryReply::Refused`].
    pub fn handle(
        &self,
        caller: &Caller,
        request: MemoryRequest,
    ) -> impl Future<Output = MemoryReply> + Send {
        let _ = (allowed(caller, &request), &self.rules, &self.spaces);
        async move { todo!("authorise, route to the Space, run the pure machines, apply their effects") }
    }

    /// Writes the export of `options` as a tar stream into `out`, and returns its manifest.
    pub fn export(
        &self,
        caller: &Caller,
        options: &ExportOptions,
        out: &mut (impl Write + Send),
    ) -> impl Future<Output = MemoryReply> + Send {
        let _ = (caller, options, out);
        async move {
            todo!("ExportWriter over each Space's entries and plain files; never the index or keys")
        }
    }
}
