//! Consolidation: the model seam, the run machine (memory section 4.4) and the draft check.

use almanac_core::{
    ConsolidateFailure, EventRef, Fact, Hunk, KindTag, Label, Link, RunId, RunState, SpaceId,
    ThingView, TidyHunk, TopicPath, UnixSeconds, UserText,
};
use std::future::Future;

/// One event as the consolidator sees it: no bodies beyond what the person saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEvent {
    /// Which event (a `Link::Event` target).
    pub event: EventRef,
    /// What kind.
    pub kind: KindTag,
    /// What it was about.
    pub things: Vec<ThingView>,
    /// Its provenance: a promoted fact's label is the join of its sources'.
    pub label: Label,
}

/// One topic file as the consolidator reads it: the whole file as text (trailers included, so
/// a Tidy can keep every fact's id, author, label and links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputTopic {
    /// The topic.
    pub topic: TopicPath,
    /// The file.
    pub text: UserText,
}

/// What a run reads: the active facts and the events since the last run's cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationInput {
    /// The Space (a draft never touches another).
    pub space: SpaceId,
    /// This run.
    pub run: RunId,
    /// When it started: the date of the facts it drafts.
    pub now: UnixSeconds,
    /// Active facts.
    pub facts: Vec<Fact>,
    /// Events since the last cut.
    pub events: Vec<InputEvent>,
    /// The topic files as they are now: what a Tidy rewrites.
    pub topics: Vec<InputTopic>,
}

/// What the model proposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// The proposed changes.
    pub hunks: Vec<Hunk>,
}

/// Why the consolidator failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConsolidateError {
    /// The model could not be reached or refused.
    #[error("the consolidation model is unavailable")]
    Unavailable,
    /// Its answer could not be read.
    #[error("the consolidation draft is unreadable")]
    Unparseable,
    /// The embedder or GPU was busy.
    #[error("busy")]
    Busy,
}

impl From<ConsolidateError> for ConsolidateFailure {
    fn from(e: ConsolidateError) -> Self {
        match e {
            ConsolidateError::Unavailable => ConsolidateFailure::ModelUnavailable,
            ConsolidateError::Unparseable => ConsolidateFailure::Unparseable,
            ConsolidateError::Busy => ConsolidateFailure::EmbedderBusy,
        }
    }
}

/// The model that drafts a consolidation. Implementations: `ScriptedConsolidator`
/// (almanac-fake), `InferdConsolidator` (memoryd: inferd chat, `Task::Extract`, background).
pub trait Consolidator: Send + Sync {
    /// Drafts hunks for `input`.
    fn draft(
        &self,
        input: ConsolidationInput,
    ) -> impl Future<Output = Result<Draft, ConsolidateError>> + Send;
}

/// Why a hunk was dropped by the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkFault {
    /// A promotion cites links outside the input.
    CitesOutsideInput,
    /// The new label is not the join of the cited sources' labels (untrusted laundered).
    LabelLaundered,
    /// The draft removes a fact (a draft never does).
    RemovesFact,
    /// The hunk touches a topic outside the Space.
    OutsideSpace,
}

/// A draft after the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedDraft {
    /// The hunks that passed.
    pub kept: Vec<Hunk>,
    /// The index (in the draft) and fault of each dropped hunk.
    pub dropped: Vec<(usize, HunkFault)>,
}

/// Checks a draft against its input: every `Promote` cites links inside the input; its label is
/// the join of the cited sources' labels (so untrusted content cannot be laundered); no hunk
/// removes a fact; none reaches outside the Space. Invalid hunks are dropped, not repaired.
pub fn check_draft(input: &ConsolidationInput, draft: Draft) -> CheckedDraft {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for (index, hunk) in draft.hunks.into_iter().enumerate() {
        match hunk_fault(input, &hunk) {
            None => kept.push(hunk),
            Some(fault) => dropped.push((index, fault)),
        }
    }
    CheckedDraft { kept, dropped }
}

fn hunk_fault(input: &ConsolidationInput, hunk: &Hunk) -> Option<HunkFault> {
    match hunk {
        Hunk::Promote { fact, .. } => fact_fault(input, fact),
        Hunk::Supersede { old, new } => (!input.facts.iter().any(|f| &f.id == old))
            .then_some(HunkFault::CitesOutsideInput)
            .or_else(|| fact_fault(input, new)),
        Hunk::Flag { facts, .. } => facts
            .iter()
            .any(|id| !input.facts.iter().any(|f| &f.id == id))
            .then_some(HunkFault::CitesOutsideInput),
        Hunk::Tidy(TidyHunk { before, after, .. }) | Hunk::ExternalEdit { before, after, .. } => {
            (!before.as_str().trim().is_empty() && after.as_str().trim().is_empty())
                .then_some(HunkFault::RemovesFact)
        }
        Hunk::Stamp { .. } => None,
    }
}

/// The faults of a fact a hunk introduces: its links name events of another Space, or things,
/// facts and events outside the input; its label is less restrictive than the join of what it
/// cites.
fn fact_fault(input: &ConsolidationInput, fact: &Fact) -> Option<HunkFault> {
    let outside_space = fact
        .links
        .iter()
        .any(|l| matches!(l, Link::Event(e) if e.space != input.space));
    if outside_space {
        return Some(HunkFault::OutsideSpace);
    }
    let Some(joined) = cited_label(input, &fact.links) else {
        return Some(HunkFault::CitesOutsideInput);
    };
    (fact.label.join(&joined) != fact.label.join(&fact.label)).then_some(HunkFault::LabelLaundered)
}

/// The label a fact citing `links` must carry: the join of the labels of everything they cite,
/// or `None` when there is no link or one cites something the input does not hold. The check
/// ([`check_draft`]) holds every promotion to this; a consolidator builds its facts with it.
pub fn cited_label(input: &ConsolidationInput, links: &[Link]) -> Option<Label> {
    let labels: Option<Vec<Label>> = links
        .iter()
        .map(|link| source_labels(input, link))
        .collect::<Option<Vec<Vec<Label>>>>()
        .map(|all| all.into_iter().flatten().collect());
    let labels = labels.filter(|l| !l.is_empty())?;
    Some(
        labels
            .iter()
            .skip(1)
            .fold(labels[0].clone(), |a, b| a.join(b)),
    )
}

/// The labels of what one link cites inside the input (every event naming a thing), or `None`
/// when it cites something the input does not hold.
fn source_labels(input: &ConsolidationInput, link: &Link) -> Option<Vec<Label>> {
    let labels: Vec<Label> = match link {
        Link::Event(e) => input
            .events
            .iter()
            .filter(|ev| &ev.event == e)
            .map(|ev| ev.label.clone())
            .collect(),
        Link::Fact(id) => input
            .facts
            .iter()
            .filter(|f| &f.id == id)
            .map(|f| f.label.clone())
            .collect(),
        Link::Thing(t) => input
            .events
            .iter()
            .filter(|ev| ev.things.iter().any(|v| &v.thing == t))
            .map(|ev| ev.label.clone())
            .collect(),
        Link::Run(_) => Vec::new(),
    };
    (!labels.is_empty()).then_some(labels)
}

/// Whether the person is at the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desktop {
    /// In use.
    Busy,
    /// Idle.
    Idle,
}

/// Where the power comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    /// A battery.
    Battery,
    /// The mains.
    Ac,
}

/// What happened to a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEvent {
    /// The nightly tick, with the desktop's state.
    Tick {
        /// Whether the person is away.
        desktop: Desktop,
        /// Where the power comes from.
        power: Power,
    },
    /// Start gathering.
    Start,
    /// The facts and events are collected.
    InputReady,
    /// The consolidator answered.
    DraftOk,
    /// The consolidator failed.
    DraftFailed(ConsolidateFailure),
    /// Invalid hunks were dropped.
    ChecksDone,
    /// Apply the proposal.
    Proceed,
    /// The person reverted the run.
    Revert,
    /// The failure was logged.
    Reported,
}

/// What the service must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEffect {
    /// Collect active facts and events since the last cut.
    Gather,
    /// Ask the consolidator.
    Draft,
    /// Run `check_draft`.
    Check,
    /// Auto-apply Tidy, Stamp, Supersede and trusted Promote hunks, keeping pre-images;
    /// untrusted Promote hunks go to `pending/`.
    ApplyHunks,
    /// Emit `ConsolidationReady`.
    EmitReady,
    /// Log `Memory.Consolidated`.
    LogConsolidated,
    /// Put the pre-images back.
    RestorePreImages,
    /// Log `Memory.Reverted`.
    LogReverted,
    /// Retry at the next nightly tick.
    RetryNextNight,
}

/// The next state and effects.
pub fn step(state: RunState, event: RunEvent) -> (RunState, Vec<RunEffect>) {
    use RunEffect as E;
    use RunEvent as V;
    use RunState as S;
    let tonight = V::Tick {
        desktop: Desktop::Idle,
        power: Power::Ac,
    };
    match (state, event) {
        (S::Idle | S::Applied | S::Reverted, e) if e == tonight => (S::Due, vec![]),
        (S::Due, V::Start) => (S::Gathering, vec![E::Gather]),
        (S::Gathering, V::InputReady) => (S::Drafting, vec![E::Draft]),
        (S::Drafting, V::DraftOk) => (S::Checking, vec![E::Check]),
        (S::Drafting | S::Gathering, V::DraftFailed(why)) => {
            (S::Failed(why), vec![E::RetryNextNight])
        }
        (S::Checking, V::ChecksDone) => (S::Proposed, vec![]),
        (S::Proposed, V::Proceed) => (
            S::Applied,
            vec![E::ApplyHunks, E::EmitReady, E::LogConsolidated],
        ),
        (S::Applied, V::Revert) => (S::Reverted, vec![E::RestorePreImages, E::LogReverted]),
        (S::Failed(_), V::Reported) => (S::Idle, vec![]),
        (unchanged, _) => (unchanged, vec![]),
    }
}
