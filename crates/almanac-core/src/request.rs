//! The memory wire: what a caller asks. One serde form on every carrier (the D-Bus members
//! carry the JSON of these types in `s` arguments).

use crate::event::{EventRef, Record};
use crate::fact::{FactDraft, Settlement};
use crate::ids::{FactId, KindPattern, PlanToken, RuleId, SpacePath};
use crate::inject::{InjectQuery, RecentQuery};
use crate::query::{ExportOptions, FactQuery, FileWhyClaim, MarkRequest, RecallQuery};
use crate::rules::RememberRule;
use crate::thing::ThingRef;
use crate::timeline::TimelineQuery;
use porter_core::{AppName, SpaceId, UnixSeconds};
use prov::RunId;
use serde::{Deserialize, Serialize};

/// What a forget covers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ForgetScope {
    /// One event.
    Event(EventRef),
    /// A thing and everything derived from it.
    Thing(ThingRef),
    /// One fact and what derives from it.
    Fact(FactId),
    /// Everything that happened in a time range.
    Range(UnixSeconds, UnixSeconds),
    /// Everything about one app.
    App(AppName),
    /// Everything of these kinds.
    Kind(KindPattern),
    /// The whole Space.
    Space,
}

/// A request to memoryd. Who may send which is `almanac-service::allowed`.
// One request is built and sent per call, and the wire form is the serde of this enum as it
// stands: boxing `Record` would only complicate every match for no saving.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum MemoryRequest {
    // Writers.
    /// Record one event.
    Record(Record),
    /// Record several, in order.
    RecordBatch(Vec<Record>),
    /// Say why a file changed.
    ExplainFile(FileWhyClaim),
    /// Mark or unmark a thing.
    Mark(MarkRequest),
    // Recall.
    /// Search.
    Search(RecallQuery),
    /// List facts.
    Facts(FactQuery),
    /// Events related to a thing.
    Related(SpaceId, ThingRef),
    /// Where a file came from.
    Provenance(SpaceId, SpacePath),
    /// Automatic recall for one turn: a search cut to a token budget (Q3).
    Inject(InjectQuery),
    /// Recent activity in a Space, newest first, with labels (the router's read; the shell's
    /// `Timeline` is the full, filtered UI view).
    Recent(SpaceId, RecentQuery),
    /// The primer: the index of what is known, for the start of a session.
    Primer(SpaceId),
    /// Propose a fact.
    Propose(SpaceId, FactDraft),
    // Control.
    /// List Spaces.
    Spaces,
    /// One Space's status.
    Status(SpaceId),
    /// A page of the timeline.
    Timeline(SpaceId, TimelineQuery),
    /// Compute what a forget would remove.
    PlanForget(SpaceId, ForgetScope),
    /// Apply a plan.
    Forget(PlanToken),
    /// List pending facts.
    Pending(SpaceId),
    /// Keep or discard a pending fact.
    Settle(FactId, Settlement),
    /// The last consolidation diff.
    Consolidation(SpaceId),
    /// Run consolidation now.
    RunConsolidation(SpaceId),
    /// Revert a consolidation run.
    Revert(RunId),
    /// Apply a proposed consolidation run (`memory.consolidation.apply = review` stops a run at
    /// `Proposed`; this is the person's go-ahead).
    ApplyConsolidation(RunId),
    /// Discard a proposed consolidation run: it stays on disk marked discarded and can no longer
    /// be applied. Any other state of the run is `Invalid`.
    DiscardConsolidation(RunId),
    /// List rules.
    Rules,
    /// Add or replace a rule.
    SetRule(RememberRule),
    /// Remove a rule.
    RemoveRule(RuleId),
    /// Pause memory for a Space.
    Pause(SpaceId, UnixSeconds),
    /// Resume it.
    Resume(SpaceId),
    /// Verify its hash chain.
    Verify(SpaceId),
    /// Rebuild its index.
    Rebuild(SpaceId),
    /// Export (the tar stream goes to the fd beside the request).
    Export(ExportOptions),
    /// Run the retention sweep now: erase the bodies and prune the headers that have outlived
    /// their keep (memoryd also sweeps on a timer).
    Sweep(SpaceId),
}

impl MemoryRequest {
    /// The Space the request is about, when it names exactly one up front.
    pub fn space(&self) -> Option<&SpaceId> {
        match self {
            MemoryRequest::Record(r) => Some(&r.space),
            MemoryRequest::ExplainFile(c) => Some(&c.space),
            MemoryRequest::Mark(m) => Some(&m.space),
            MemoryRequest::Search(q) => Some(&q.space),
            MemoryRequest::Facts(q) => Some(&q.space),
            MemoryRequest::Inject(q) => Some(&q.space),
            MemoryRequest::Related(s, _)
            | MemoryRequest::Provenance(s, _)
            | MemoryRequest::Primer(s)
            | MemoryRequest::Recent(s, _)
            | MemoryRequest::Propose(s, _)
            | MemoryRequest::Status(s)
            | MemoryRequest::Timeline(s, _)
            | MemoryRequest::PlanForget(s, _)
            | MemoryRequest::Pending(s)
            | MemoryRequest::Consolidation(s)
            | MemoryRequest::RunConsolidation(s)
            | MemoryRequest::Pause(s, _)
            | MemoryRequest::Resume(s)
            | MemoryRequest::Verify(s)
            | MemoryRequest::Rebuild(s)
            | MemoryRequest::Sweep(s) => Some(s),
            MemoryRequest::RecordBatch(_)
            | MemoryRequest::Spaces
            | MemoryRequest::Forget(_)
            | MemoryRequest::Settle(..)
            | MemoryRequest::Revert(_)
            | MemoryRequest::ApplyConsolidation(_)
            | MemoryRequest::DiscardConsolidation(_)
            | MemoryRequest::Rules
            | MemoryRequest::SetRule(_)
            | MemoryRequest::RemoveRule(_)
            | MemoryRequest::Export(_) => None,
        }
    }
}
