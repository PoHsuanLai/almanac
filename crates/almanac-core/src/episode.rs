//! Episodes: what happened in one task, run or side conversation, kept between the event log and
//! facts (research-persistent-agent section C).
//!
//! An episode is a trusted typed [`Skeleton`], built without a model at the task's end, and an
//! optional model-written [`Narrative`] that carries its own label. It says what happened, and
//! never steers by itself; a fact says what is true and needs trust. Episodes are stored as
//! [`EventBody::Episode`](crate::EventBody) events, so they are searchable (recall indexes the
//! skeleton and the narrative as two documents, each with its own label), forgettable (cascade
//! through `touched`), paused and retained like any other audit-class event.
//!
//! Events are immutable, so the idle pass records a **second** `Episode` event with the same
//! `id` and the same skeleton once it has a narrative ([`Episode::narrates`]); the newest event
//! per `id` is the one recall indexes.

use crate::slug::slug_enum;
use crate::text::{UserText, text_id};
use crate::thing::{ThingRef, ThingRole, ThingView};
use porter_core::{Count, SpaceId, UnixSeconds, is_id};
use prov::{ActionName, AgentRef, Effect, Label, MessageText, ModelRole, OutcomeRef, UndoHandle};
use serde::{Deserialize, Serialize};

text_id!(
    /// One episode: the task or run id it closes, in porter's id grammar. A side conversation
    /// of the person with a worker or run gets its own.
    EpisodeId,
    "episode id",
    is_id
);
text_id!(
    /// The name of one typed result value (`messages_archived`), in porter's id grammar.
    ResultName,
    "result name",
    is_id
);

slug_enum!(
    /// What an episode covers.
    EpisodeKind {
        /// A task, worker or run, start to finish.
        Task => "task",
        /// The person's side conversation with a subagent: their turns and the subagent's typed
        /// steps since.
        Side => "side"
    }
);

/// How an episode ended.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum EpisodeOutcome {
    /// Finished what was asked.
    Done,
    /// Could not finish.
    Failed,
    /// Stopped early.
    Cancelled,
    /// Handed to another agent.
    Handed {
        /// To whom.
        to: AgentRef,
    },
    /// Not over yet (a long task's checkpoint).
    Open,
}

slug_enum!(
    /// What one step came to.
    StepOutcome {
        /// Done.
        Done => "done",
        /// The gate refused it.
        Denied => "denied",
        /// It failed.
        Failed => "failed",
        /// Not run (an earlier step failed or the plan changed).
        Skipped => "skipped"
    }
);

/// One typed step: an action, its targets, its effect and what came of it. No free text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StepLine {
    /// The action.
    pub action: ActionName,
    /// The things it acted on.
    pub targets: Vec<ThingRef>,
    /// Its effect class.
    pub effect: Effect,
    /// What came of it.
    pub outcome: StepOutcome,
    /// The undo journal entry, if it wrote something undoable.
    pub undo: Option<UndoHandle>,
}

/// One typed result value; text is never inlined, only referenced.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ResultValue {
    /// A number.
    Count(Count),
    /// A thing.
    Thing(ThingRef),
    /// An action outcome, by reference (its text, if any, stays behind a handle there).
    Outcome(OutcomeRef),
}

/// One named result.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResultLine {
    /// What it is.
    pub name: ResultName,
    /// Its value.
    pub value: ResultValue,
}

/// The trusted part, built without a model: the person's words, app-authored metadata, typed
/// outcomes, entity ids and closed-set values. Anything else is a handle elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Skeleton {
    /// Its label: `Trusted` by construction (the service refuses an untrusted skeleton).
    pub label: Label,
    /// The person's own words, capped per turn by the builder.
    pub asked: Vec<MessageText>,
    /// What was done, in order.
    pub steps: Vec<StepLine>,
    /// The things it touched, for cascade-forget and "related to this thread".
    pub touched: Vec<(ThingView, ThingRole)>,
    /// What it produced.
    pub results: Vec<ResultLine>,
}

/// The model-written account, behind its own label: the join of what the model read plus
/// `Source::Model(by)`. A tainted narrative reaches a planner only as a handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Narrative {
    /// The text.
    pub text: UserText,
    /// Its label.
    pub label: Label,
    /// Which model role wrote it (`Consolidator` unless a `Summarizer` role is added).
    pub by: ModelRole,
}

/// One episode.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Episode {
    /// Which episode.
    pub id: EpisodeId,
    /// Whose it is: the worker, run or companion task it closes.
    pub agent: AgentRef,
    /// What it covers.
    pub kind: EpisodeKind,
    /// The episode of whoever spawned it.
    pub parent: Option<EpisodeId>,
    /// Its Space: episodes live where their task ran.
    pub space: SpaceId,
    /// When it started.
    pub started: UnixSeconds,
    /// When it ended (or was last checkpointed).
    pub ended: UnixSeconds,
    /// How it ended.
    pub outcome: EpisodeOutcome,
    /// The trusted part.
    pub skeleton: Skeleton,
    /// The model's account, once the idle pass wrote it.
    pub narrative: Option<Narrative>,
}

/// Whether one episode event is the narrated successor of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Succession {
    /// Same episode, same skeleton, and the earlier had no narrative: this one replaces it.
    Narrates,
    /// Anything else.
    Unrelated,
}

impl Episode {
    /// Whether `self` is the narrated successor of `earlier`.
    pub fn narrates(&self, earlier: &Episode) -> Succession {
        let same = self.id == earlier.id && self.skeleton == earlier.skeleton;
        if same && earlier.narrative.is_none() && self.narrative.is_some() {
            Succession::Narrates
        } else {
            Succession::Unrelated
        }
    }
}

impl Skeleton {
    /// The skeleton as lines of text, for the index and for the planner's recent-episodes
    /// section: what was asked, each step, what was touched, each result.
    pub fn text(&self) -> String {
        let asked = self.asked.iter().map(|a| format!("asked: {}", a.as_str()));
        let steps = self.steps.iter().map(|s| {
            let targets: Vec<String> = s
                .targets
                .iter()
                .map(|t| format!("{}/{}", t.kind, t.key))
                .collect();
            format!(
                "step: {} {} {}",
                s.action,
                targets.join(" "),
                s.outcome.slug()
            )
        });
        let touched = self
            .touched
            .iter()
            .map(|(v, _)| format!("touched: {}", v.title.as_str()));
        asked
            .chain(steps)
            .chain(touched)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
