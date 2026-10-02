//! The Space and fact machines: one table each (memory section 4).

mod common;

use almanac_core::*;
use almanac_service::fact::{self, FactEffect, FactEvent, FactLife};
use almanac_service::space::{self, BUFFER_LIMIT};
use almanac_service::*;
use common::*;

#[test]
fn space_state_machine_table() {
    use SpaceEffect as E;
    use SpaceEvent as V;
    use SpaceState as S;
    let until = UnixSeconds(NOW.0 + 600);
    let paused = S::Paused { until };
    let few = Count(3);
    let full = Count(BUFFER_LIMIT);
    let cases: Vec<(&str, SpaceState, SpaceEvent, SpaceState, Vec<SpaceEffect>)> = vec![
        (
            "key arrives",
            S::Locked,
            V::KeyAvailable,
            S::Open,
            vec![
                E::OpenDatabases,
                E::VerifyHeadAgainstAnchor,
                E::FlushBuffer,
                E::EmitStatus,
            ],
        ),
        (
            "record while locked buffers",
            S::Locked,
            V::Record { buffered: few },
            S::Locked,
            vec![E::Buffer],
        ),
        (
            "record while locked and full drops",
            S::Locked,
            V::Record { buffered: full },
            S::Locked,
            vec![E::CountDropped(DropReason::SpaceLocked)],
        ),
        (
            "record while open",
            S::Open,
            V::Record { buffered: Count(0) },
            S::Open,
            vec![E::Admit],
        ),
        (
            "record while paused goes to admission",
            paused,
            V::Record { buffered: Count(0) },
            paused,
            vec![E::Admit],
        ),
        (
            "pause",
            S::Open,
            V::Pause { until },
            paused,
            vec![E::LogPaused { until }, E::EmitStatus],
        ),
        (
            "re-pause extends",
            paused,
            V::Pause {
                until: UnixSeconds(until.0 + 5),
            },
            S::Paused {
                until: UnixSeconds(until.0 + 5),
            },
            vec![
                E::LogPaused {
                    until: UnixSeconds(until.0 + 5),
                },
                E::EmitStatus,
            ],
        ),
        (
            "tick before the end",
            paused,
            V::Tick {
                now: UnixSeconds(until.0 - 1),
            },
            paused,
            vec![],
        ),
        (
            "tick at the end resumes",
            paused,
            V::Tick { now: until },
            S::Open,
            vec![E::LogResumed, E::EmitStatus],
        ),
        (
            "resume",
            paused,
            V::Resume,
            S::Open,
            vec![E::LogResumed, E::EmitStatus],
        ),
        (
            "resume when open is nothing",
            S::Open,
            V::Resume,
            S::Open,
            vec![],
        ),
        (
            "pause while locked is ignored",
            S::Locked,
            V::Pause { until },
            S::Locked,
            vec![],
        ),
        (
            "forget the space",
            S::Open,
            V::ForgetConfirmed,
            S::Deleting,
            vec![],
        ),
        (
            "forget the space while paused",
            paused,
            V::ForgetConfirmed,
            S::Deleting,
            vec![],
        ),
        (
            "deletion finishes",
            S::Deleting,
            V::Done,
            S::Gone,
            vec![E::AnchorFinalHead, E::DestroyKey, E::RemoveDirs],
        ),
        (
            "record while deleting",
            S::Deleting,
            V::Record { buffered: Count(0) },
            S::Deleting,
            vec![E::CountDropped(DropReason::SpaceUnknown)],
        ),
        (
            "key lost",
            S::Open,
            V::KeyLost,
            S::Locked,
            vec![E::CloseDatabases, E::EmitStatus],
        ),
        (
            "key lost while paused",
            paused,
            V::KeyLost,
            S::Locked,
            vec![E::CloseDatabases, E::EmitStatus],
        ),
        ("gone stays gone", S::Gone, V::KeyAvailable, S::Gone, vec![]),
        (
            "gone ignores a lost key",
            S::Gone,
            V::KeyLost,
            S::Gone,
            vec![],
        ),
    ];
    for (name, from, event, to, effects) in cases {
        assert_eq!(space::step(from, event), (to, effects), "{name}");
    }
}

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
