//! The plan machine, the plan digest and the apply order.

use crate::support::*;
use almanac_core::*;
use almanac_service::forget::PLAN_TTL_SECONDS;
use almanac_service::*;

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
