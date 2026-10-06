//! Views for the memory UI: facts with their sources, the forget preview, the consolidation
//! diff.

use crate::event::EventRef;
use crate::fact::{Fact, FactState};
use crate::file::{FileChange, FileWhy};
use crate::ids::KindTag;
use crate::ids::{FactId, PlanToken, SpacePath, TopicPath, UseCount};
use crate::slug::slug_enum;
use crate::text::UserText;
use crate::thing::ThingView;
use crate::timeline::TimelineEntry;
use porter_core::{Count, UnixSeconds};
use prov::{Actor, RunId};
use serde::{Deserialize, Serialize};

/// Where one source of a fact stands now.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SourceView {
    /// A thing that still exists.
    Present(ThingView),
    /// An event.
    Event(TimelineEntry),
    /// Gone (forgotten or expired).
    Purged,
}

/// A note a consolidation run left about facts that looked wrong or stale (`Hunk::Flag`). The
/// run changed nothing; the person decides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagNote {
    /// The run that flagged it.
    pub run: RunId,
    /// Why, in the model's words.
    pub note: UserText,
}

/// A fact with what the UI needs around it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactView {
    /// The fact.
    pub fact: Fact,
    /// Its topic.
    pub topic: TopicPath,
    /// Where it stands.
    pub state: FactState,
    /// What it came from.
    pub sources: Vec<SourceView>,
    /// How often it was read into a prompt.
    pub used: UseCount,
    /// When it was last read.
    pub last_used: Option<UnixSeconds>,
    /// What consolidation runs flagged about it, oldest run first; empty when nothing did.
    #[serde(default)]
    pub flagged: Vec<FlagNote>,
}

/// What a forget would remove, exactly: apply removes what this shows, no more.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForgetPlanView {
    /// The token to confirm with.
    pub token: PlanToken,
    /// When the plan lapses.
    pub expires: UnixSeconds,
    /// Event bodies.
    pub events: Count,
    /// Facts (active ones, with their sources).
    pub facts: Vec<FactView>,
    /// Procedures.
    pub procedures: Count,
    /// Index documents.
    pub index_docs: Count,
    /// Pending facts.
    pub pending: Count,
}

/// A summary of one event for "related" lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventSummary {
    /// Which event.
    pub event: EventRef,
    /// When.
    pub occurred: UnixSeconds,
    /// What kind.
    pub kind: KindTag,
    /// Who.
    pub actor: Actor,
    /// What about; empty when erased.
    pub things: Vec<ThingView>,
}

/// One change to a file and why.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileHistoryEntry {
    /// The event.
    pub event: EventRef,
    /// When.
    pub occurred: UnixSeconds,
    /// What happened to it.
    pub change: FileChange,
    /// Why.
    pub why: FileWhy,
}

/// Where a file came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileProvenance {
    /// The file (followed through renames).
    pub path: SpacePath,
    /// Its changes, newest first.
    pub history: Vec<FileHistoryEntry>,
}

slug_enum!(
    /// Where a promoted fact lands.
    Lands {
        /// In a topic file.
        Active => "active",
        /// In `pending/`.
        Pending => "pending"
    }
);

/// A tidy of one topic file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TidyHunk {
    /// The topic.
    pub topic: TopicPath,
    /// Before.
    pub before: UserText,
    /// After.
    pub after: UserText,
}

/// One change a consolidation run proposes or made.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Hunk {
    /// Reword or merge without changing meaning.
    Tidy(TidyHunk),
    /// A new fact from events.
    Promote {
        /// The fact.
        fact: Fact,
        /// Where it lands.
        to: Lands,
    },
    /// A fact replaced by a newer one.
    Supersede {
        /// The old one.
        old: FactId,
        /// The new one.
        new: Fact,
    },
    /// Facts that look wrong or stale; the person decides.
    Flag {
        /// Which.
        facts: Vec<FactId>,
        /// Why.
        note: UserText,
    },
    /// The person edited a file outside memoryd.
    ExternalEdit {
        /// The topic.
        topic: TopicPath,
        /// Before.
        before: UserText,
        /// After.
        after: UserText,
    },
    /// A bullet the person added, now stamped.
    Stamp {
        /// The topic.
        topic: TopicPath,
        /// The bullet.
        text: UserText,
    },
}

slug_enum!(
    /// Why a consolidation run failed.
    ConsolidateFailure {
        /// The model could not be reached or refused.
        ModelUnavailable => "model_unavailable",
        /// The draft could not be read.
        Unparseable => "unparseable",
        /// The embedder was busy.
        EmbedderBusy => "embedder_busy"
    }
);

/// Where a consolidation run is (the machine in `almanac-service::consolidation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum RunState {
    /// Nothing to do.
    Idle,
    /// Due tonight.
    Due,
    /// Collecting facts and events.
    Gathering,
    /// The model is drafting.
    Drafting,
    /// Checking the draft's invariants.
    Checking,
    /// Valid hunks are ready.
    Proposed,
    /// Applied, with pre-images kept.
    Applied,
    /// Reverted from the pre-images.
    Reverted,
    /// The person discarded the proposal; it stays on disk and cannot be applied.
    Discarded,
    /// A newer run replaced the proposal before it was applied; it stays on disk.
    Superseded,
    /// Failed; retried next night.
    Failed(ConsolidateFailure),
}

slug_enum!(
    /// Why a proposed hunk was skipped when its run was applied.
    SkipReason {
        /// An event it cites is gone or its body was erased.
        EventGone => "event_gone",
        /// A fact it cites is gone.
        FactGone => "fact_gone",
        /// The fact it would replace is no longer active.
        ReplacedFactGone => "replaced_fact_gone",
        /// The file it rewrites is not as it was when the hunk was drafted.
        FileChanged => "file_changed"
    }
);

/// A hunk that was proposed but not applied, and why.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkippedHunk {
    /// The hunk.
    pub hunk: Hunk,
    /// Why it was left out.
    pub reason: SkipReason,
}

/// One run's diff for review. Once a run is applied, `hunks` holds only what was applied and
/// `skipped` what was left out (a proposal has none skipped yet).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DraftView {
    /// The run.
    pub run: RunId,
    /// Its hunks.
    pub hunks: Vec<Hunk>,
    /// Where it is.
    pub state: RunState,
    /// Proposed hunks that were not applied, with the reason.
    #[serde(default)]
    pub skipped: Vec<SkippedHunk>,
}
