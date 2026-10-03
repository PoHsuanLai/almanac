//! memoryd's own audit events and the counts they carry.

use crate::chain::Head;
use crate::ids::{FactId, RuleId, Seq, TopicPath};
use crate::rules::DropReason;
use crate::text::{Link32, PlanDigest};
use porter_core::{Count, SpaceId, UnixSeconds};
use prov::{Actor, RunId};
use serde::{Deserialize, Serialize};

/// What a read touched, for the `Memory.Read` audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadScope {
    /// A search.
    Search,
    /// A fact query.
    Facts,
    /// Events related to a thing.
    Related,
    /// A file's provenance.
    Provenance,
    /// The primer.
    Primer,
    /// An automatic, budgeted recall (`InjectQuery`).
    Inject,
    /// Recent activity (`RecentQuery`).
    Recent,
}

/// What a forget removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForgetCounts {
    /// Event bodies erased.
    pub events: Count,
    /// Facts removed.
    pub facts: Count,
    /// Pending facts removed.
    pub pending: Count,
    /// Procedures removed.
    pub procedures: Count,
    /// Index documents removed.
    pub index_docs: Count,
}

/// What an export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExportCounts {
    /// Event lines.
    pub events: Count,
    /// Facts.
    pub facts: Count,
    /// Pending facts.
    pub pending: Count,
    /// Procedures.
    pub procedures: Count,
}

/// memoryd's own audit: every change it makes to what it knows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum MemoryOp {
    /// A fact joined a topic.
    FactAdded {
        /// Which.
        fact: FactId,
        /// Where.
        topic: TopicPath,
    },
    /// The person kept a pending fact.
    FactConfirmed {
        /// Which.
        fact: FactId,
    },
    /// A pending fact was discarded.
    FactRejected {
        /// Which.
        fact: FactId,
    },
    /// A forget plan was applied.
    Forgot {
        /// The plan's closure digest.
        plan: PlanDigest,
        /// What it removed.
        counts: ForgetCounts,
    },
    /// Someone read memory.
    Read {
        /// Who.
        by: Actor,
        /// What kind of read.
        scope: ReadScope,
        /// The facts returned.
        facts: Vec<FactId>,
        /// How many events were returned.
        events: Count,
    },
    /// A consolidation run was applied.
    Consolidated {
        /// The run.
        run: RunId,
        /// Its hunks.
        hunks: Count,
    },
    /// A run was reverted.
    Reverted {
        /// The run.
        run: RunId,
    },
    /// An export was written.
    Exported {
        /// What it holds.
        counts: ExportCounts,
    },
    /// Memory was paused.
    Paused {
        /// Until when.
        until: UnixSeconds,
    },
    /// Memory resumed.
    Resumed,
    /// A rule was set or removed.
    RuleChanged {
        /// Which.
        rule: RuleId,
    },
    /// A prefix was pruned behind a checkpoint.
    Checkpoint {
        /// The last pruned sequence number.
        cut: Seq,
        /// The link there.
        link: Link32,
    },
    /// A Space was deleted: its final head, kept in the `desktop` Space's log so the deletion
    /// is visible and the removed chain's end is on record.
    SpaceDeleted {
        /// Which Space.
        space: SpaceId,
        /// Where its log ended.
        head: Head,
    },
    /// Records were dropped by admission.
    Dropped {
        /// How many.
        count: Count,
        /// Why.
        reason: DropReason,
    },
}

impl MemoryOp {
    /// The last element of the event's kind tag (`memory.fact_added`).
    pub fn slug(&self) -> &'static str {
        match self {
            MemoryOp::FactAdded { .. } => "fact_added",
            MemoryOp::FactConfirmed { .. } => "fact_confirmed",
            MemoryOp::FactRejected { .. } => "fact_rejected",
            MemoryOp::Forgot { .. } => "forgot",
            MemoryOp::Read { .. } => "read",
            MemoryOp::Consolidated { .. } => "consolidated",
            MemoryOp::Reverted { .. } => "reverted",
            MemoryOp::Exported { .. } => "exported",
            MemoryOp::Paused { .. } => "paused",
            MemoryOp::Resumed => "resumed",
            MemoryOp::RuleChanged { .. } => "rule_changed",
            MemoryOp::Checkpoint { .. } => "checkpoint",
            MemoryOp::SpaceDeleted { .. } => "space_deleted",
            MemoryOp::Dropped { .. } => "dropped",
        }
    }
}
