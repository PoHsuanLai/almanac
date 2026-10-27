//! The consolidation run machine: one table.

use almanac_core::*;
use almanac_tidy::consolidation::{self, RunEffect, RunEvent};

#[test]
fn consolidation_run_table() {
    use RunEffect as E;
    use RunEvent as V;
    use RunState as S;
    use consolidation::{Desktop, Power};
    let night = V::Tick {
        desktop: Desktop::Idle,
        power: Power::Ac,
    };
    let cases: Vec<(&str, RunState, RunEvent, RunState, Vec<RunEffect>)> = vec![
        ("tonight, idle, on AC", S::Idle, night, S::Due, vec![]),
        (
            "busy desktop waits",
            S::Idle,
            V::Tick {
                desktop: Desktop::Busy,
                power: Power::Ac,
            },
            S::Idle,
            vec![],
        ),
        (
            "battery waits",
            S::Idle,
            V::Tick {
                desktop: Desktop::Idle,
                power: Power::Battery,
            },
            S::Idle,
            vec![],
        ),
        ("start", S::Due, V::Start, S::Gathering, vec![E::Gather]),
        (
            "input ready",
            S::Gathering,
            V::InputReady,
            S::Drafting,
            vec![E::Draft],
        ),
        (
            "draft ok",
            S::Drafting,
            V::DraftOk,
            S::Checking,
            vec![E::Check],
        ),
        (
            "draft failed",
            S::Drafting,
            V::DraftFailed(ConsolidateFailure::ModelUnavailable),
            S::Failed(ConsolidateFailure::ModelUnavailable),
            vec![E::RetryNextNight],
        ),
        (
            "embedder busy",
            S::Drafting,
            V::DraftFailed(ConsolidateFailure::EmbedderBusy),
            S::Failed(ConsolidateFailure::EmbedderBusy),
            vec![E::RetryNextNight],
        ),
        (
            "checks done",
            S::Checking,
            V::ChecksDone,
            S::Proposed,
            vec![],
        ),
        (
            "proceed",
            S::Proposed,
            V::Proceed,
            S::Applied,
            vec![E::ApplyHunks, E::EmitReady, E::LogConsolidated],
        ),
        (
            "revert",
            S::Applied,
            V::Revert,
            S::Reverted,
            vec![E::RestorePreImages, E::LogReverted],
        ),
        (
            "reported failure goes idle",
            S::Failed(ConsolidateFailure::Unparseable),
            V::Reported,
            S::Idle,
            vec![],
        ),
        (
            "next night after applying",
            S::Applied,
            night,
            S::Due,
            vec![],
        ),
        (
            "next night after reverting",
            S::Reverted,
            night,
            S::Due,
            vec![],
        ),
        (
            "cannot revert what was not applied",
            S::Proposed,
            V::Revert,
            S::Proposed,
            vec![],
        ),
        ("cannot apply a due run", S::Due, V::Proceed, S::Due, vec![]),
        (
            "discard a proposal",
            S::Proposed,
            V::Discard,
            S::Discarded,
            vec![],
        ),
        (
            "a newer run supersedes",
            S::Proposed,
            V::Supersede,
            S::Superseded,
            vec![],
        ),
        (
            "an applied run cannot be discarded",
            S::Applied,
            V::Discard,
            S::Applied,
            vec![],
        ),
        (
            "a discarded run cannot be applied",
            S::Discarded,
            V::Proceed,
            S::Discarded,
            vec![],
        ),
        (
            "a superseded run cannot be applied",
            S::Superseded,
            V::Proceed,
            S::Superseded,
            vec![],
        ),
        (
            "a superseded run cannot be discarded",
            S::Superseded,
            V::Discard,
            S::Superseded,
            vec![],
        ),
        (
            "next night after discarding",
            S::Discarded,
            night,
            S::Due,
            vec![],
        ),
        (
            "next night after superseding",
            S::Superseded,
            night,
            S::Due,
            vec![],
        ),
    ];
    for (name, from, event, to, effects) in cases {
        assert_eq!(consolidation::step(from, event), (to, effects), "{name}");
    }
}
