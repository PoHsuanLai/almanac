//! Facts: what the companion knows, as add-only dated statements linked to their sources.

use crate::event::EventRef;
use crate::ids::{FactId, FactText, TopicPath};
use crate::thing::ThingRef;
use porter_core::UnixSeconds;
use prov::{Actor, Label, RunId};
use serde::{Deserialize, Serialize};

/// Something a fact was learned from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Link {
    /// An event in a log.
    Event(EventRef),
    /// A thing an app owns.
    Thing(ThingRef),
    /// Another fact (derivation).
    Fact(FactId),
    /// A computer-use run.
    Run(RunId),
}

/// When a fact holds. v1 writes and reads only `Unstated`; the file format reserves the `valid:`
/// key so bi-temporal dating can arrive without a migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Validity {
    /// No dating beyond when it was learned.
    Unstated,
}

/// One fact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fact {
    /// Its id.
    pub id: FactId,
    /// What is known.
    pub text: FactText,
    /// When it was learned.
    pub recorded: UnixSeconds,
    /// Who established it.
    pub by: Actor,
    /// Its provenance label (the join of its sources').
    pub label: Label,
    /// What it came from: at least one, except facts the person said themselves.
    pub links: Vec<Link>,
    /// The facts it replaces.
    pub supersedes: Vec<FactId>,
    /// When it holds.
    pub valid: Validity,
}

/// Where a fact stands. Derived, never stored.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum FactState {
    /// Waiting for the person in `pending/`.
    Pending,
    /// In a topic file.
    Active,
    /// Replaced by a newer fact.
    Superseded {
        /// The fact that replaced it.
        by: FactId,
    },
}

/// A fact proposed but not yet stamped: the id, time and actor come from memoryd.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactDraft {
    /// Where it should go.
    pub topic: TopicPath,
    /// What is known.
    pub text: FactText,
    /// What it came from.
    pub links: Vec<Link>,
    /// What it replaces.
    pub supersedes: Vec<FactId>,
}

/// What the launcher's Memory rows are about: sill's subject key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum MemoryItem {
    /// A fact.
    Fact(FactId),
    /// An event.
    Event(EventRef),
}

/// How the person settles a pending fact. `Keep` carries the receipt of the confirmation: only
/// the shell's own UI can produce one (`prov::InputProof::ShellCaller`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Settlement {
    /// Keep it, declassifying its label with the receipt.
    Keep(prov::ConfirmReceipt),
    /// Throw it away.
    Discard,
}
