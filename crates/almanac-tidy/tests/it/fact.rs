//! The fact machine: one table, and where a proposal lands (memory section 4.2).

use crate::support::*;
use almanac_core::*;
use almanac_tidy::fact::{self, FactEffect, FactEvent, FactLife};

#[test]
fn fact_state_machine_table() {
    use FactEffect as E;
    use FactEvent as V;
    use FactLife as L;
    let by = FactId::mint(9, [9; 10]);
    let cases: Vec<(&str, FactLife, FactEvent, FactLife, Vec<FactEffect>)> = vec![
        (
            "trusted proposal is active",
            L::Unborn,
            V::Propose(Lands::Active),
            L::Active,
            vec![E::AppendToTopic, E::LogAdded],
        ),
        (
            "untrusted proposal is pending",
            L::Unborn,
            V::Propose(Lands::Pending),
            L::Pending,
            vec![E::StageInPending],
        ),
        (
            "keep",
            L::Pending,
            V::Keep,
            L::Active,
            vec![
                E::DeclassifyWithReceipt,
                E::AppendToTopic,
                E::RemoveFromPending,
                E::LogConfirmed,
            ],
        ),
        (
            "discard",
            L::Pending,
            V::Discard,
            L::Removed,
            vec![E::RemoveFromPending, E::LogRejected],
        ),
        (
            "ages out at 14 days",
            L::Pending,
            V::Age(DayCount(14)),
            L::Removed,
            vec![E::RemoveFromPending, E::LogRejected],
        ),
        (
            "not yet aged out",
            L::Pending,
            V::Age(DayCount(13)),
            L::Pending,
            vec![],
        ),
        (
            "superseded",
            L::Active,
            V::SupersededBy(by.clone()),
            L::Superseded(by.clone()),
            vec![],
        ),
        (
            "plan removes an active fact",
            L::Active,
            V::PlanApplied,
            L::Removed,
            vec![E::RemoveFromTopic, E::RemoveFromIndex],
        ),
        (
            "plan removes a superseded fact",
            L::Superseded(by.clone()),
            V::PlanApplied,
            L::Removed,
            vec![E::RemoveFromTopic, E::RemoveFromIndex],
        ),
        (
            "plan removes a pending fact",
            L::Pending,
            V::PlanApplied,
            L::Removed,
            vec![E::RemoveFromPending, E::RemoveFromIndex],
        ),
        (
            "keeping an active fact is refused",
            L::Active,
            V::Keep,
            L::Active,
            vec![E::Refuse(Refusal::NotPending)],
        ),
        (
            "keeping nothing is refused",
            L::Unborn,
            V::Keep,
            L::Unborn,
            vec![E::Refuse(Refusal::NoSuchFact)],
        ),
        (
            "removed stays removed",
            L::Removed,
            V::Age(DayCount(99)),
            L::Removed,
            vec![],
        ),
    ];
    for (name, from, event, to, effects) in cases {
        assert_eq!(fact::step(from, event), (to, effects), "{name}");
    }
}

#[test]
fn a_proposal_lands_by_its_label() {
    assert_eq!(fact::lands(&label(Integrity::Trusted)), Lands::Active);
    assert_eq!(fact::lands(&label(Integrity::Untrusted)), Lands::Pending);
}
