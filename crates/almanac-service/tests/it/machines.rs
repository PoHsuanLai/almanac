//! The Space machine: one table (memory section 4).

use crate::support::*;
use almanac_core::*;
use almanac_service::space::{self, BUFFER_LIMIT};
use almanac_service::*;

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
