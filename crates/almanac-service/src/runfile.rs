//! One consolidation run as the vault keeps it: `consolidation/<run>.toml`, written when the run
//! is drafted and rewritten each time its outcome changes (proposed, applied, reverted,
//! discarded, superseded). A file is never deleted for a state change: its `state` says what
//! became of the run. Pure apart from `load_all` and `save`, which take the vault.
//!
//! The file holds the run's hunks as the person reviews them (each hunk carries its own
//! pre-image: Tidy and ExternalEdit their `before` text, Promote and Supersede the facts they
//! cite, which `Grounds` checks again at apply time), the skipped hunks with their reasons, and
//! the log cut the run was drafted from. The pre-images of files an *applied* run rewrote stay in
//! memory (see FINDINGS "proposal-file"): the file would otherwise keep text the person may
//! later forget.

use almanac_core::{
    DraftView, Fact, FactId, Hunk, Link, RunId, RunState, Seq, SkippedHunk, UnixSeconds,
};
use almanac_store::{Vault, VaultError, VaultPath};
use serde::{Deserialize, Serialize};

/// The directory under the Space's vault.
pub(crate) const DIR: &str = "consolidation";
/// The record format (a bump is a format change; see the golden test).
const FORMAT: u32 = 1;

/// Where a run is, as a file records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecordState {
    /// Waiting for the person.
    Proposed,
    /// Applied (what was applied is `hunks`, what was left out is `skipped`).
    Applied,
    /// Applied, then reverted.
    Reverted,
    /// The person discarded it.
    Discarded,
    /// A newer run replaced it.
    Superseded,
}

impl RecordState {
    /// The record state of a run state; `None` for the states a run passes through.
    pub(crate) fn of(state: RunState) -> Option<Self> {
        match state {
            RunState::Proposed => Some(Self::Proposed),
            RunState::Applied => Some(Self::Applied),
            RunState::Reverted => Some(Self::Reverted),
            RunState::Discarded => Some(Self::Discarded),
            RunState::Superseded => Some(Self::Superseded),
            RunState::Idle
            | RunState::Due
            | RunState::Gathering
            | RunState::Drafting
            | RunState::Checking
            | RunState::Failed(_) => None,
        }
    }

    pub(crate) fn run_state(self) -> RunState {
        match self {
            Self::Proposed => RunState::Proposed,
            Self::Applied => RunState::Applied,
            Self::Reverted => RunState::Reverted,
            Self::Discarded => RunState::Discarded,
            Self::Superseded => RunState::Superseded,
        }
    }
}

/// What a forget removed: the facts and the events (by sequence number) whose text must leave
/// the run files too.
#[derive(Debug, Default)]
pub(crate) struct Gone {
    pub facts: Vec<FactId>,
    pub events: Vec<Seq>,
}

/// One run's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RunRecord {
    pub format: u32,
    pub run: RunId,
    pub state: RecordState,
    /// When the run was drafted.
    pub drafted: UnixSeconds,
    /// When it left `proposed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled: Option<UnixSeconds>,
    /// The log cut the run was drafted from (where it read from).
    pub cut: Seq,
    /// The log head it read up to.
    pub head: Seq,
    /// How many hunks a forget took out of this file (their text is gone; the count is the record).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub erased: u32,
    pub hunks: Vec<Hunk>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedHunk>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// `consolidation/<run>.toml`.
pub(crate) fn path(run: &RunId) -> Option<VaultPath> {
    VaultPath::parse(&format!("{DIR}/{run}.toml"))
}

impl RunRecord {
    /// A fresh record.
    pub(crate) fn new(
        run: RunId,
        state: RecordState,
        drafted: UnixSeconds,
        cut: Seq,
        head: Seq,
        hunks: Vec<Hunk>,
    ) -> Self {
        Self {
            format: FORMAT,
            run,
            state,
            drafted,
            settled: None,
            cut,
            head,
            erased: 0,
            hunks,
            skipped: Vec::new(),
        }
    }

    /// The same run in `state`, settled at `at` unless it is proposed.
    pub(crate) fn settled_as(self, state: RecordState, at: UnixSeconds) -> Self {
        Self {
            state,
            settled: (state != RecordState::Proposed).then_some(at),
            ..self
        }
    }

    pub(crate) fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    /// A record from its text, or `None` when it is torn, not TOML, or from another format.
    pub(crate) fn from_toml(text: &str) -> Option<Self> {
        let rec: Self = toml::from_str(text).ok()?;
        (rec.format == FORMAT).then_some(rec)
    }

    pub(crate) fn view(&self) -> DraftView {
        DraftView {
            run: self.run.clone(),
            hunks: self.hunks.clone(),
            state: self.state.run_state(),
            skipped: self.skipped.clone(),
        }
    }

    /// Takes out every hunk whose text a forget removed; `true` when anything went.
    pub(crate) fn scrub(&mut self, gone: &Gone) -> bool {
        let before = self.hunks.len() + self.skipped.len();
        self.hunks.retain(|h| !must_go(h, gone));
        self.skipped.retain(|s| !must_go(&s.hunk, gone));
        let removed = before - self.hunks.len() - self.skipped.len();
        self.erased += u32::try_from(removed).unwrap_or(u32::MAX);
        removed > 0
    }
}

/// Whether `hunk` carries text a forget removed: it cites a removed event or fact, or rewrites
/// topic text that may have held one (when any fact went, every rewrite of a file goes).
pub(crate) fn must_go(hunk: &Hunk, gone: &Gone) -> bool {
    let fact_hit = |f: &Fact| {
        gone.facts.contains(&f.id)
            || f.links.iter().any(|l| match l {
                Link::Event(e) => gone.events.contains(&e.seq),
                Link::Fact(id) => gone.facts.contains(id),
                Link::Thing(_) | Link::Run(_) => false,
            })
    };
    match hunk {
        Hunk::Tidy(_) | Hunk::Stamp { .. } | Hunk::ExternalEdit { .. } => !gone.facts.is_empty(),
        Hunk::Promote { fact, .. } => fact_hit(fact),
        Hunk::Supersede { old, new } => gone.facts.contains(old) || fact_hit(new),
        Hunk::Flag { facts, .. } => facts.iter().any(|f| gone.facts.contains(f)),
    }
}

/// Every run file the vault holds that reads; a torn or foreign file, and a temporary file left
/// by a write that never finished, are not records and are skipped.
pub(crate) fn load_all(vault: &impl Vault) -> Vec<RunRecord> {
    let Some(dir) = VaultPath::parse(DIR) else {
        return Vec::new();
    };
    vault
        .list(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            let stem = p.as_str().strip_prefix(DIR)?.strip_prefix('/')?;
            let run = RunId::parse(stem.strip_suffix(".toml")?).ok()?;
            let bytes = vault.read(&p).ok()?;
            let rec = RunRecord::from_toml(std::str::from_utf8(&bytes).ok()?)?;
            (rec.run == run).then_some(rec)
        })
        .collect()
}

/// The record of `run`, if its file reads.
pub(crate) fn load(vault: &impl Vault, run: &RunId) -> Option<RunRecord> {
    load_all(vault).into_iter().find(|r| &r.run == run)
}

/// Writes the record atomically (the vault's temporary file, then rename).
pub(crate) fn save(vault: &impl Vault, rec: &RunRecord) -> Result<(), VaultError> {
    let p = path(&rec.run).ok_or_else(|| VaultError::Io("run file path".into()))?;
    let text = rec.to_toml().map_err(|e| VaultError::Io(e.to_string()))?;
    vault.write_atomic(&p, text.as_bytes())
}
