//! `plan_forget` over a `MemoryLog`, and `check_draft`.

mod common;

use almanac_core::*;
use almanac_seal::{Purpose, SpaceKey, derive};
use almanac_service::consolidation::{
    CheckedDraft, ConsolidationInput, Draft, HunkFault, InputEvent, check_draft,
};
use almanac_service::{FactGraph, FactNode, plan_forget};
use common::*;
use eventlog::{LogWrite, MemoryLog, NewHeader};

fn log_of(records: &[Record]) -> MemoryLog {
    let key = SpaceKey::from_bytes([9; 32]);
    let digest = derive(&key, &space("work"), Purpose::Digest);
    let mut log = MemoryLog::new(&space("work"), ReplicaId([1; 16]), digest.clone());
    for r in records {
        let header = NewHeader::of(r, NOW, &digest);
        log.append(header, Some(r.body.clone())).expect("append");
    }
    log
}

fn fact(n: u8, links: Vec<Link>) -> FactNode {
    FactNode {
        id: FactId::mint(u64::from(n), [n; 10]),
        links,
        state: FactState::Active,
    }
}

#[test]
fn the_closure_follows_things_events_and_fact_links() {
    let log = log_of(&[
        record(thing_body("org.quire.Mail", "a"), user("org.quire.Mail")),
        record(thing_body("org.quire.Mail", "b"), user("org.quire.Mail")),
        record(area_body(AreaTag::Cua), planner()),
    ]);
    let event = |seq| {
        Link::Event(EventRef {
            space: space("work"),
            replica: ReplicaId([1; 16]),
            seq: Seq(seq),
        })
    };
    let direct = fact(1, vec![event(1)]);
    let derived = fact(2, vec![Link::Fact(direct.id.clone())]);
    let unrelated = fact(3, vec![event(2)]);
    let mut pending = fact(4, vec![Link::Thing(thing("org.quire.Mail", "a"))]);
    pending.state = FactState::Pending;
    let graph = FactGraph {
        nodes: vec![direct.clone(), derived.clone(), unrelated, pending.clone()],
        procedures: vec![],
    };
    let plan = plan_forget(
        &space("work"),
        &ForgetScope::Thing(thing("org.quire.Mail", "a")),
        &log,
        &graph,
    );
    assert_eq!(plan.events, vec![Seq(1)]);
    assert_eq!(plan.facts, vec![direct.id, derived.id]);
    assert_eq!(plan.pending, vec![pending.id]);
    assert!(
        plan.index_docs
            .iter()
            .all(|d| d.starts_with("f:") || d.starts_with("e:"))
    );
    let all = plan_forget(&space("work"), &ForgetScope::Space, &log, &graph);
    assert_eq!(all.events.len(), 3);
    assert_eq!(all.facts.len() + all.pending.len(), 4);
    let same = plan_forget(
        &space("work"),
        &ForgetScope::Thing(thing("org.quire.Mail", "a")),
        &log,
        &graph,
    );
    assert_eq!(plan.digest(), same.digest());
}

#[test]
fn a_terminal_is_no_app_so_forgetting_an_app_reaches_its_events_only_by_their_things() {
    // seq 1: the terminal on a Notes thing; seq 2: the terminal on a Mail thing; seq 3: Mail's
    // own user action on a Notes thing.
    let log = log_of(&[
        record(thing_body("org.quire.Notes", "n"), Actor::Cli),
        record(thing_body("org.quire.Mail", "a"), Actor::Cli),
        record(thing_body("org.quire.Notes", "n"), user("org.quire.Mail")),
    ]);
    let graph = FactGraph {
        nodes: vec![],
        procedures: vec![],
    };
    let events = |scope: ForgetScope| plan_forget(&space("work"), &scope, &log, &graph).events;
    assert_eq!(
        events(ForgetScope::App(app("org.quire.Mail"))),
        vec![Seq(2), Seq(3)]
    );
    assert_eq!(
        events(ForgetScope::App(app("org.quire.Notes"))),
        vec![Seq(1), Seq(3)],
        "Notes' things, whoever touched them"
    );
    assert_eq!(
        events(ForgetScope::Space),
        vec![Seq(1), Seq(2), Seq(3)],
        "the Space reaches the terminal's events"
    );
}

fn untrusted() -> Label {
    label(Integrity::Untrusted)
}

fn promote(links: Vec<Link>, label: Label) -> Hunk {
    Hunk::Promote {
        fact: Fact {
            id: FactId::mint(9, [9; 10]),
            text: FactText::parse("x is y").expect("t"),
            recorded: NOW,
            by: planner(),
            label,
            links,
            supersedes: vec![],
            valid: Validity::Unstated,
        },
        to: Lands::Active,
    }
}

#[test]
fn a_draft_cannot_launder_cite_outside_or_remove() {
    let inside = EventRef {
        space: space("work"),
        replica: ReplicaId([1; 16]),
        seq: Seq(1),
    };
    let input = ConsolidationInput {
        now: NOW,
        space: space("work"),
        run: RunId::parse("r-1").expect("r"),
        facts: vec![],
        events: vec![InputEvent {
            event: inside.clone(),
            kind: KindTag::parse("thing.archived").expect("k"),
            things: vec![],
            label: untrusted(),
        }],
        topics: vec![],
    };
    let foreign = EventRef {
        space: space("home"),
        ..inside.clone()
    };
    let missing = EventRef {
        seq: Seq(77),
        ..inside.clone()
    };
    let tidy_away = Hunk::Tidy(TidyHunk {
        topic: TopicPath::parse("a").expect("t"),
        before: "something".into(),
        after: "".into(),
    });
    let draft = Draft {
        hunks: vec![
            promote(vec![Link::Event(inside.clone())], untrusted()),
            promote(vec![Link::Event(inside.clone())], label(Integrity::Trusted)),
            promote(vec![Link::Event(missing)], untrusted()),
            promote(vec![Link::Event(foreign)], untrusted()),
            promote(vec![], untrusted()),
            tidy_away,
        ],
    };
    let CheckedDraft { kept, dropped } = check_draft(&input, draft);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        dropped,
        vec![
            (1, HunkFault::LabelLaundered),
            (2, HunkFault::CitesOutsideInput),
            (3, HunkFault::OutsideSpace),
            (4, HunkFault::CitesOutsideInput),
            (5, HunkFault::RemovesFact),
        ]
    );
}
