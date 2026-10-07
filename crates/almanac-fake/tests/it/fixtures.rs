//! The fixtures are valid values of the shapes the specs name.

use almanac_core::*;
use almanac_fake::*;

#[test]
fn fixtures_are_what_their_names_say() {
    let cases: Vec<(&str, Option<Record>, &str, ActorKind)> = vec![
        (
            "mail_thread_archived",
            mail_thread_archived(),
            "thing.archived",
            ActorKind::User,
        ),
        (
            "file_saved_from_attachment",
            file_saved_from_attachment(),
            "file.created",
            ActorKind::User,
        ),
        (
            "companion_forwarded",
            companion_forwarded(),
            "thing.forwarded",
            ActorKind::Companion,
        ),
        ("cua_run_step", cua_run_step(), "cua.step", ActorKind::Cua),
        (
            "policy_ask",
            policy_ask(),
            "policy.ruled",
            ActorKind::System,
        ),
    ];
    for (name, record, kind, actor) in cases {
        let record = record.unwrap_or_else(|| panic!("{name} builds"));
        assert_eq!(record.body.kind().as_str(), kind, "{name}");
        assert_eq!(record.actor.kind(), actor, "{name}");
        assert_eq!(record.occurred, NOW, "{name}");
        let json = serde_json::to_string(&record).expect("json");
        assert_eq!(
            serde_json::from_str::<Record>(&json).expect("back"),
            record,
            "{name}"
        );
    }
}

#[test]
fn audit_fixtures_are_never_dropped_even_when_paused() {
    for record in [companion_forwarded(), cua_run_step(), policy_ask()] {
        let record = record.expect("fixture");
        let paused = SpaceState::Paused {
            until: UnixSeconds(NOW.0 + 60),
        };
        let verdict = admit(&record, &RuleSet::standard(), &paused, &Marks::default());
        assert!(
            matches!(verdict, Admission::HeaderOnly { .. }),
            "{:?}",
            record.body.kind()
        );
    }
    let plain = admit(
        &mail_thread_archived().expect("fixture"),
        &RuleSet::standard(),
        &SpaceState::Paused { until: NOW },
        &Marks::default(),
    );
    assert_eq!(plain, Admission::Drop(DropReason::Paused));
}

#[test]
fn scratch_directories_are_private_and_laid_out() {
    let scratch = Scratch::new().expect("scratch");
    let work = SpaceId::parse("work").expect("space");
    assert!(scratch.dirs().events_db(&work).starts_with(scratch.root()));
    assert!(scratch.dirs().index_db(&work).starts_with(scratch.root()));
}
