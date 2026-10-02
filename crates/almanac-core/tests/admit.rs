//! `admit` against its table: pause, marks, rules, specificity and the audit class.

mod common;

use almanac_core::*;
use common::*;

fn rules(rs: Vec<RememberRule>) -> RuleSet {
    RuleSet {
        rules: rs,
        defaults: RuleSet::standard().defaults,
    }
}

fn named(id: &str, mut r: RememberRule) -> RememberRule {
    r.id = RuleId::parse(id).expect("id");
    r
}

#[test]
fn admit_table() {
    let mail = record(archived_body(), user());
    let audit = record(area_body(AreaTag::Docket, "policy.ruled"), companion());
    let paused = SpaceState::Paused {
        until: UnixSeconds(NOW.0 + 600),
    };
    let none = Marks::default();
    let default_thing = Retention::WhileSourceExists;
    let never_mail = rules(vec![named(
        "r-never",
        rule(RuleScope::App(app("org.quire.Mail")), RememberMode::Never),
    )]);

    // (name, record, rules, state, marks, expected)
    let cases: Vec<(&str, &Record, RuleSet, SpaceState, Marks, Admission)> = vec![
        (
            "open, no rules: default retention",
            &mail,
            rules(vec![]),
            SpaceState::Open,
            none.clone(),
            Admission::Keep {
                retention: default_thing,
            },
        ),
        (
            "paused user event drops",
            &mail,
            rules(vec![]),
            paused,
            none.clone(),
            Admission::Drop(DropReason::Paused),
        ),
        (
            "paused companion audit keeps its header",
            &audit,
            rules(vec![]),
            paused,
            none.clone(),
            Admission::HeaderOnly {
                retention: Retention::Days(DayCount(90)),
            },
        ),
        (
            "a Never rule on the app drops its events",
            &mail,
            never_mail.clone(),
            SpaceState::Open,
            none.clone(),
            Admission::Drop(DropReason::Never(RuleId::parse("r-never").expect("id"))),
        ),
        (
            "a Never rule cannot drop audit events",
            &audit,
            rules(vec![named(
                "r-all",
                rule(RuleScope::Actor(ActorKind::Companion), RememberMode::Never),
            )]),
            SpaceState::Open,
            none.clone(),
            Admission::HeaderOnly {
                retention: Retention::Days(DayCount(90)),
            },
        ),
        (
            "locked drops",
            &mail,
            rules(vec![]),
            SpaceState::Locked,
            none.clone(),
            Admission::Drop(DropReason::SpaceLocked),
        ),
        (
            "deleting is unknown",
            &mail,
            rules(vec![]),
            SpaceState::Deleting,
            none.clone(),
            Admission::Drop(DropReason::SpaceUnknown),
        ),
        (
            "a marked thing drops",
            &mail,
            rules(vec![]),
            SpaceState::Open,
            Marks {
                things: [thing("org.quire.Mail", "mail.thread", "7f3a")].into(),
            },
            Admission::Drop(DropReason::ThingMarked),
        ),
    ];
    for (name, rec, rs, state, marks, want) in cases {
        assert_eq!(admit(rec, &rs, &state, &marks), want, "{name}");
    }
}

#[test]
fn the_most_specific_scope_wins_and_ties_go_to_the_strictest_mode() {
    let mail = record(archived_body(), user());
    let this_thing = RuleScope::Thing(thing("org.quire.Mail", "mail.thread", "7f3a"));
    let kind = RuleScope::Kind(KindPattern::parse("mail.*").expect("pattern"));
    let app_scope = RuleScope::App(app("org.quire.Mail"));
    let open = SpaceState::Open;
    let marks = Marks::default();

    let thing_beats_kind = rules(vec![
        named("r-kind", rule(kind.clone(), RememberMode::Never)),
        named("r-thing", rule(this_thing.clone(), RememberMode::Full)),
    ]);
    assert!(matches!(
        admit(&mail, &thing_beats_kind, &open, &marks),
        Admission::Keep { .. }
    ));

    let kind_beats_app = rules(vec![
        named("r-app", rule(app_scope.clone(), RememberMode::Never)),
        named("r-kind", rule(kind.clone(), RememberMode::HeaderOnly)),
    ]);
    assert!(matches!(
        admit(&mail, &kind_beats_app, &open, &marks),
        Admission::HeaderOnly { .. }
    ));

    let tie = rules(vec![
        named("r-a", rule(app_scope.clone(), RememberMode::Full)),
        named(
            "r-b",
            rule(
                RuleScope::App(app("org.quire.Mail")),
                RememberMode::HeaderOnly,
            ),
        ),
    ]);
    assert!(matches!(
        admit(&mail, &tie, &open, &marks),
        Admission::HeaderOnly { .. }
    ));
}

#[test]
fn unexplained_file_changes_are_kept_a_week() {
    let rec = record(file_body(FileWhy::Unexplained), Actor::Unknown);
    assert_eq!(
        admit(
            &rec,
            &RuleSet::standard(),
            &SpaceState::Open,
            &Marks::default()
        ),
        Admission::Keep {
            retention: Retention::Days(UNEXPLAINED_FILE_DAYS)
        }
    );
}

#[test]
fn path_rules_use_globs() {
    let rec = record(file_body(FileWhy::Unexplained), Actor::Unknown);
    let path_rule = |glob: &str| {
        rules(vec![named(
            "r-path",
            rule(
                RuleScope::Path(PathGlob::parse(glob).expect("glob")),
                RememberMode::Never,
            ),
        )])
    };
    let dropped = |glob: &str| {
        matches!(
            admit(&rec, &path_rule(glob), &SpaceState::Open, &Marks::default()),
            Admission::Drop(_)
        )
    };
    assert!(dropped("/home/u/Downloads/*.pdf"));
    assert!(dropped("/home/**"));
    assert!(!dropped("/home/*.pdf"));
    assert!(!dropped("/home/u/Documents/**"));
}

#[test]
fn globs() {
    let cases = [
        ("*.pdf", "a.pdf", true),
        ("*.pdf", "d/a.pdf", false),
        ("**/a.pdf", "d/e/a.pdf", true),
        ("d/*/a", "d/x/a", true),
        ("d/*/a", "d/x/y/a", false),
        ("**", "anything/at/all", true),
        ("a?", "a?", true),
    ];
    for (pattern, text, want) in cases {
        assert_eq!(glob_matches(pattern, text), want, "{pattern} vs {text}");
    }
}
