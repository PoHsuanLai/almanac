//! The memory wire: what memoryd answers.

use crate::chain::ChainReport;
use crate::entries::{Ack, EntriesPage};
use crate::event::EventRef;
use crate::export::ExportManifest;
use crate::fact::{FactState, Link, MemoryItem};
use crate::ids::FactId;
use crate::inject::RecentEntry;
use crate::op::ForgetCounts;
use crate::relocate::Relocation;
use crate::rules::{DropReason, RuleSet};
use crate::space::{SpaceStatus, SpaceSummary};
use crate::text::PlanDigest;
use crate::text::UserText;
use crate::timeline::TimelinePage;
use crate::views::{DraftView, EventSummary, FactView, FileProvenance, ForgetPlanView};
use porter_core::{Count, UnixSeconds};
use prov::Label;
use serde::{Deserialize, Serialize};

/// Why a hit matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RecallWhy {
    /// Full-text only.
    Lexical {
        /// Its rank in the lexical list, from 1.
        rank: u32,
    },
    /// Embedding only.
    Semantic {
        /// Its rank in the semantic list, from 1.
        rank: u32,
    },
    /// Both lists.
    Both {
        /// Its lexical rank.
        lexical: u32,
        /// Its semantic rank.
        semantic: u32,
    },
}

/// One search hit. It carries its label: the planner applies taint from it, and the
/// quarantined reader gets no recall at all.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecallHit {
    /// What matched.
    pub doc: MemoryItem,
    /// Its text.
    pub text: UserText,
    /// When it was learned or happened.
    pub at: UnixSeconds,
    /// Its provenance.
    pub label: Label,
    /// What it came from.
    pub links: Vec<Link>,
    /// Why it matched.
    pub why: RecallWhy,
}

/// What a forget did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForgetReport {
    /// The plan's closure digest.
    pub plan: PlanDigest,
    /// What it removed; equals the plan's preview.
    pub counts: ForgetCounts,
}

/// What a retention sweep removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SweepReport {
    /// Event bodies erased (their headers stay).
    pub bodies: Count,
    /// Headers pruned behind a new checkpoint.
    pub headers: Count,
}

/// Why memoryd said no. Each maps 1:1 to an `org.quire.Memory1.Error.<Variant>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Refusal {
    /// The caller's class may not do this.
    NotAllowed,
    /// The Space's key is not available.
    SpaceLocked,
    /// No such Space.
    SpaceUnknown,
    /// The request reaches into another Space.
    OutsideSpace,
    /// The closure changed since the plan was made.
    PlanStale,
    /// The plan lapsed.
    PlanExpired,
    /// No such fact.
    NoSuchFact,
    /// The fact is not pending.
    NotPending,
    /// Busy; try again.
    Busy,
    /// The Space's storage is full: the event was not stored.
    SpaceFull,
    /// The Space's storage cannot be written now (an I/O failure, a corrupt log): the event
    /// was not stored.
    Unavailable,
    /// Admission kept nothing, for this reason: a durable append refuses where `Record` would
    /// answer `Ok` (memory paused, a `Never` rule, the thing marked).
    NotKept(DropReason),
    /// The request is malformed.
    Invalid(String),
}

/// What memoryd answers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryReply {
    /// One event recorded.
    Recorded(EventRef),
    /// A durable append was committed.
    Durable(Ack),
    /// A batch recorded: the first event and how many.
    RecordedBatch(EventRef, Count),
    /// Done.
    Ok,
    /// Search hits (also the answer to `Inject`, ranked and within its budget).
    Hits(Vec<RecallHit>),
    /// Recent activity, newest first.
    Recent(Vec<RecentEntry>),
    /// A page of one stream, oldest first.
    Entries(EntriesPage),
    /// Facts.
    Facts(Vec<FactView>),
    /// Related events.
    Related(Vec<EventSummary>),
    /// A file's provenance.
    Provenance(FileProvenance),
    /// The primer, markdown.
    Primer(String),
    /// A fact proposed, and where it landed.
    Proposed(FactId, FactState),
    /// The Spaces.
    Spaces(Vec<SpaceSummary>),
    /// One status.
    Status(SpaceStatus),
    /// A timeline page.
    Timeline(TimelinePage),
    /// A forget plan.
    Plan(ForgetPlanView),
    /// A forget report.
    Forgot(ForgetReport),
    /// Pending facts.
    Pending(Vec<FactView>),
    /// A consolidation diff.
    Consolidation(DraftView),
    /// The rules.
    Rules(RuleSet),
    /// A verification result.
    Verified(ChainReport),
    /// An export manifest.
    Exported(ExportManifest),
    /// Refused.
    Refused(Refusal),
    /// A retention sweep ran.
    Swept(SweepReport),
    /// A removed Space's memories were settled.
    Relocated(Relocation),
}
