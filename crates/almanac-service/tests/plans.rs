//! The plan and consolidation machines, the plan digest and the apply order.

mod common;

use almanac_core::*;
use almanac_service::consolidation::{self, RunEffect, RunEvent};
use almanac_service::forget::PLAN_TTL_SECONDS;
use almanac_service::*;
use common::*;

fn digest(n: u8) -> PlanDigest {
    PlanDigest([n; 32])
}

#[test]
fn plan_state_machine_table() {
    use PlanEffect as E;
    use PlanEvent as V;
    use PlanState as S;
    let made = S::planned(digest(1), NOW);
    let expires = UnixSeconds(NOW.0 + PLAN_TTL_SECONDS);
    assert_eq!(
        made,
        S::Planned {
            digest: digest(1),
            expires
        }
    );
    let cases: Vec<(&str, PlanState, PlanEvent, PlanState, Vec<PlanEffect>)> = vec![
        (
            "forget with the same closure",
            made,
            V::Forget {
                current: digest(1),
                now: NOW,
            },
            S::Applying,
            vec![E::Apply],
        ),
        (
            "forget at the last second",
            made,
            V::Forget {
                current: digest(1),
                now: expires,
            },
            S::Applying,
            vec![E::Apply],
        ),
        (
            "closure changed",
            made,
            V::Forget {
                current: digest(2),
                now: NOW,
            },
            S::Stale,
            vec![E::Refuse(Refusal::PlanStale)],
        ),
        (
            "lapsed",
            made,
            V::Forget {
                current: digest(1),
                now: UnixSeconds(expires.0 + 1),
            },
            S::Expired,
            vec![E::Refuse(Refusal::PlanExpired)],
        ),
        (
            "lapse by tick",
            made,
            V::Tick {
                now: UnixSeconds(expires.0 + 1),
            },
            S::Expired,
            vec![],
        ),
        (
            "tick before the end",
            made,
            V::Tick { now: expires },
            made,
            vec![],
        ),
        ("applied", S::Applying, V::Done, S::Applied, vec![]),
        (
            "a stale plan stays stale",
            S::Stale,
            V::Forget {
                current: digest(1),
                now: NOW,
            },
            S::Stale,
            vec![E::Refuse(Refusal::PlanStale)],
        ),
        (
            "an expired plan stays expired",
            S::Expired,
            V::Forget {
                current: digest(1),
                now: NOW,
            },
            S::Expired,
            vec![E::Refuse(Refusal::PlanExpired)],
        ),
        (
            "an applied plan cannot be forgotten again",
            S::Applied,
            V::Forget {
                current: digest(1),
                now: NOW,
            },
            S::Applied,
            vec![],
        ),
    ];
    for (name, from, event, to, effects) in cases {
        assert_eq!(plan_step(from, event), (to, effects), "{name}");
    }
}

fn plan() -> Plan {
    Plan {
        space: space("work"),
        scope: ForgetScope::Thing(thing("org.quire.Mail", "7f3a")),
        events: vec![Seq(3), Seq(1)],
        facts: vec![FactId::mint(1, [1; 10]), FactId::mint(2, [2; 10])],
        pending: vec![FactId::mint(3, [3; 10])],
        procedures: vec![],
        index_docs: vec!["f:a".into(), "e:r:1".into()],
    }
}

#[test]
fn the_plan_digest_covers_the_closure_not_the_walk_order() {
    let base = plan();
    let mut shuffled = plan();
    shuffled.events.reverse();
    shuffled.facts.reverse();
    shuffled.index_docs.reverse();
    assert_eq!(base.digest(), shuffled.digest());
    let mut grown = plan();
    grown.facts.push(FactId::mint(4, [4; 10]));
    assert_ne!(
        base.digest(),
        grown.digest(),
        "a new derivation changes the digest"
    );
    let mut other_space = plan();
    other_space.space = space("home");
    assert_ne!(base.digest(), other_space.digest());
    let mut moved = plan();
    moved.pending = vec![];
    moved.facts.push(FactId::mint(3, [3; 10]));
    assert_ne!(
        base.digest(),
        moved.digest(),
        "the same ids in another list are another closure"
    );
}

#[test]
fn counts_equal_the_preview_and_tokens_are_ids() {
    let c = plan().counts();
    assert_eq!(
        (c.events, c.facts, c.pending, c.procedures, c.index_docs),
        (Count(2), Count(2), Count(1), Count(0), Count(2))
    );
    let token = token_for(&plan().digest()).expect("token");
    assert!(token.as_str().starts_with("p-") && token.as_str().len() == 18);
}

#[test]
fn apply_runs_index_then_files_then_bodies_then_audit_then_wal() {
    assert_eq!(
        ApplyStep::ORDER,
        [
            ApplyStep::Index,
            ApplyStep::Memfiles,
            ApplyStep::EventBodies,
            ApplyStep::AuditEntry,
            ApplyStep::WalTruncate
        ]
    );
}

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
