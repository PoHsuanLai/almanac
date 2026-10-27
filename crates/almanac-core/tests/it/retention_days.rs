//! The person's retention over the shipped defaults: only the classes it names change.

use almanac_core::*;

fn days_of(rules: &RuleSet, pattern: &str) -> Option<Retention> {
    rules
        .defaults
        .iter()
        .find(|d| d.kind.as_str() == pattern)
        .map(|d| d.retention)
}

#[test]
fn the_default_retention_days_are_the_shipped_defaults() {
    let standard = RuleSet::standard();
    assert_eq!(
        standard.clone().with_retention(&RetentionDays::default()),
        standard
    );
}

#[test]
fn each_class_changes_its_own_patterns_and_nothing_else() {
    let days = RetentionDays {
        search: DayCount(1),
        session: DayCount(3),
    };
    let rules = RuleSet::standard().with_retention(&days);
    let keep = |n: u32| Some(Retention::Days(DayCount(n)));
    for (pattern, want) in [
        ("search.*", keep(1)),
        ("session.*", keep(3)),
        ("cua.*", keep(3)),
        ("policy.*", keep(90)),
        ("consent.*", keep(90)),
        ("memory.*", keep(90)),
        // Not part of any class the settings name.
        ("companion.*", keep(30)),
        ("thing.*", Some(Retention::WhileSourceExists)),
        ("file.*", Some(Retention::WhileSourceExists)),
    ] {
        assert_eq!(days_of(&rules, pattern), want, "{pattern}");
    }
    assert_eq!(rules.rules, RuleSet::standard().rules);
    assert_eq!(rules.defaults.len(), RuleSet::standard().defaults.len());
}

#[test]
fn a_class_the_rules_have_no_pattern_for_gets_one() {
    let rules = RuleSet::default().with_retention(&RetentionDays::default());
    assert_eq!(rules.defaults.len(), 6);
    assert_eq!(
        days_of(&rules, "search.*"),
        Some(Retention::Days(DayCount(30)))
    );
}
