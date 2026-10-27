//! The fact lifecycle machine (memory section 4.2): a proposal lands active or pending by its
//! label; pending facts are settled or age out; a plan removes them.

use almanac_core::{DayCount, FactId, Integrity, Label, Lands, PENDING_TTL_DAYS, Refusal};

/// Where a fact is in its life. `Superseded` is derived at read time (the file is unchanged),
/// so it is a state here only for the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactLife {
    /// Not yet proposed.
    Unborn,
    /// Waiting in `pending/`.
    Pending,
    /// In a topic file.
    Active,
    /// Replaced by a newer fact.
    Superseded(FactId),
    /// Gone (rejected, aged out, or forgotten).
    Removed,
}

/// What happened to a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactEvent {
    /// A proposal, with where its label says it lands.
    Propose(Lands),
    /// The person kept it (with a receipt).
    Keep,
    /// The person discarded it.
    Discard,
    /// Time passed; the fact is this old.
    Age(DayCount),
    /// A newer fact supersedes it.
    SupersededBy(FactId),
    /// A forget plan containing it was applied.
    PlanApplied,
}

/// What the service must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactEffect {
    /// Append the fact to its topic file.
    AppendToTopic,
    /// Write it under `pending/`.
    StageInPending,
    /// Declassify its label with the settlement's receipt.
    DeclassifyWithReceipt,
    /// Remove it from `pending/`.
    RemoveFromPending,
    /// Remove it from its topic file.
    RemoveFromTopic,
    /// Remove it from the index.
    RemoveFromIndex,
    /// Log `Memory.FactAdded`.
    LogAdded,
    /// Log `Memory.FactConfirmed`.
    LogConfirmed,
    /// Log `Memory.FactRejected`.
    LogRejected,
    /// The request cannot apply.
    Refuse(Refusal),
}

/// Where a proposal lands: a trusted label goes straight to a topic; anything untrusted
/// waits in `pending/` for the person.
pub fn lands(label: &Label) -> Lands {
    match label.integrity {
        Integrity::Trusted => Lands::Active,
        Integrity::Untrusted => Lands::Pending,
    }
}

/// The next life and the effects.
pub fn step(life: FactLife, event: FactEvent) -> (FactLife, Vec<FactEffect>) {
    use FactEffect as E;
    use FactEvent as V;
    use FactLife as L;
    match (life, event) {
        (L::Unborn, V::Propose(Lands::Active)) => (L::Active, vec![E::AppendToTopic, E::LogAdded]),
        (L::Unborn, V::Propose(Lands::Pending)) => (L::Pending, vec![E::StageInPending]),
        (L::Pending, V::Keep) => (
            L::Active,
            vec![
                E::DeclassifyWithReceipt,
                E::AppendToTopic,
                E::RemoveFromPending,
                E::LogConfirmed,
            ],
        ),
        (L::Pending, V::Discard) => (L::Removed, vec![E::RemoveFromPending, E::LogRejected]),
        (L::Pending, V::Age(age)) if age >= PENDING_TTL_DAYS => {
            (L::Removed, vec![E::RemoveFromPending, E::LogRejected])
        }
        (L::Active, V::SupersededBy(by)) => (L::Superseded(by), vec![]),
        (L::Pending, V::PlanApplied) => {
            (L::Removed, vec![E::RemoveFromPending, E::RemoveFromIndex])
        }
        (L::Active | L::Superseded(_), V::PlanApplied) => {
            (L::Removed, vec![E::RemoveFromTopic, E::RemoveFromIndex])
        }
        (life @ (L::Active | L::Superseded(_)), V::Keep | V::Discard) => {
            (life, vec![E::Refuse(Refusal::NotPending)])
        }
        (life @ (L::Unborn | L::Removed), V::Keep | V::Discard) => {
            (life, vec![E::Refuse(Refusal::NoSuchFact)])
        }
        (unchanged, _) => (unchanged, vec![]),
    }
}
