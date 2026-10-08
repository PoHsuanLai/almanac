//! The shared "involves app" and "kind covers event" rules that admission and forgetting use.

use crate::support::*;
use almanac_core::*;

#[test]
fn an_event_involves_its_things_owners_and_its_acting_app_but_not_a_terminal() {
    let body = archived_body();
    let mail = app("org.quire.Mail");
    let other = app("org.quire.Other");
    assert!(body.involves_app(&companion(), &mail));
    assert!(!body.involves_app(&companion(), &other));
    assert!(body.involves_app(&Actor::App { app: other.clone() }, &other));
    assert!(!body.involves_app(&Actor::Cli, &other));
}

#[test]
fn a_kind_pattern_covers_the_event_kind_or_a_thing_kind() {
    let body = archived_body();
    let covers = |p: &str| KindPattern::parse(p).expect("pattern").covers_event(&body);
    assert!(covers("mail.*"));
    assert!(covers("thing.*"));
    assert!(!covers("calendar.*"));
}
