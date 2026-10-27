//! `check_draft`: a draft cannot launder a label, cite outside its input, or remove a fact.

use crate::support::*;
use almanac_core::*;
use almanac_tidy::consolidation::{
    CheckedDraft, ConsolidationInput, Draft, HunkFault, InputEvent, check_draft,
};

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
