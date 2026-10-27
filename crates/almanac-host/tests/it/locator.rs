//! The locator over scratch configuration directories: which file wins, and that a value looser
//! than the shipped default waits for the person's confirmation.

use almanac_core::{DayCount, VaultKind};
use almanac_host::Locator;
use almanac_settings::{CONFIRMED_FILE, Fallback, Loaded, MemorySettings, SETTINGS_FILE, Why};

#[test]
fn the_locator_reads_the_first_file_of_the_configuration_directories() {
    let home = tempfile::tempdir().unwrap();
    let etc = tempfile::tempdir().unwrap();
    for (dir, text) in [
        (&home, "[memory.retention]\nsearch_days = 5\n"),
        (&etc, "[memory.retention]\nsession_days = 3\n"),
    ] {
        std::fs::create_dir_all(dir.path().join("almanac")).unwrap();
        std::fs::write(dir.path().join(SETTINGS_FILE), text).unwrap();
    }
    let env = |k: &str| match k {
        "XDG_CONFIG_HOME" => Some(home.path().display().to_string()),
        "XDG_CONFIG_DIRS" => Some(etc.path().display().to_string()),
        _ => None,
    };
    let store = home.path().join(CONFIRMED_FILE);
    let locator = Locator::from_env(&env).confirming(store);
    locator
        .confirm(&pairs(&[("memory.retention.search_days", "5")]))
        .unwrap();
    assert_eq!(locator.watch_dir(), Some(home.path().join("almanac")));
    let value = locator.read(MemorySettings::default()).value;
    assert_eq!(value.retention.search, DayCount(5));
    // The first file wins whole: the later directory's other keys are not merged in.
    assert_eq!(value.retention.session, DayCount(30));
    let none = Locator::from_env(&|_: &str| None::<String>);
    assert_eq!(
        none.read(MemorySettings::default()),
        Loaded::of(MemorySettings::default())
    );
}

fn pairs(of: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    of.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// A locator over a scratch configuration directory holding `text`, with its confirmation store.
fn locator_with(dir: &std::path::Path, text: &str) -> Locator {
    std::fs::create_dir_all(dir.join("almanac")).unwrap();
    std::fs::write(dir.join(SETTINGS_FILE), text).unwrap();
    let home = dir.display().to_string();
    let env = move |k: &str| match k {
        "XDG_CONFIG_HOME" => Some(home.clone()),
        "XDG_CONFIG_DIRS" => Some(String::new()),
        _ => None,
    };
    Locator::from_env(&env).confirming(dir.join(CONFIRMED_FILE))
}

#[test]
fn a_loosened_value_applies_only_once_the_person_confirmed_exactly_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = "[memory.retention]\nsearch_days = 3\n[memory.files]\nat_rest = \"plain\"\n";
    let locator = locator_with(dir.path(), file);
    let shipped = MemorySettings::default();

    // Nothing confirmed: both fall back to the shipped value, each with its line.
    let loaded = locator.read(shipped);
    assert_eq!(loaded.value, shipped);
    assert_eq!(
        loaded.fallbacks,
        [
            Fallback {
                key: "memory.retention.search_days".to_owned(),
                why: Why::NotConfirmed {
                    value: "3".to_owned()
                },
            },
            Fallback {
                key: "memory.files.at_rest".to_owned(),
                why: Why::NotConfirmed {
                    value: "plain".to_owned()
                },
            },
        ]
    );
    assert!(
        loaded.lines("memoryd").contains(
            &"memoryd: settings: memory.retention.search_days: 3 waits for you to confirm it in Settings"
                .to_owned()
        ),
        "{:?}",
        loaded.lines("memoryd")
    );

    // One confirmed: that one applies (and is still audited), the other still waits.
    locator
        .confirm(&pairs(&[("memory.retention.search_days", "3")]))
        .unwrap();
    let loaded = locator.read(shipped);
    assert_eq!(loaded.value.retention.search, DayCount(3));
    assert_eq!(loaded.value.at_rest, VaultKind::Sealed);
    assert_eq!(loaded.fallbacks.len(), 1);
    assert_eq!(
        loaded
            .lines("memoryd")
            .iter()
            .filter(|l| l.contains("loosened"))
            .count(),
        1
    );

    // The file changes to another loose value: the old confirmation does not carry over.
    std::fs::write(
        dir.path().join(SETTINGS_FILE),
        "[memory.retention]\nsearch_days = 2\n",
    )
    .unwrap();
    assert_eq!(locator.read(shipped).value, shipped);

    // A value that is not looser needs nothing, and a confirmation the file no longer uses is kept.
    std::fs::write(
        dir.path().join(SETTINGS_FILE),
        "[memory.retention]\nsearch_days = 40\n",
    )
    .unwrap();
    let loaded = locator.read(shipped);
    assert_eq!(loaded.value.retention.search, DayCount(40));
    assert_eq!(loaded.fallbacks, vec![]);
    assert!(
        std::fs::read_to_string(dir.path().join(CONFIRMED_FILE))
            .unwrap()
            .contains("memory.retention.search_days")
    );
}

#[test]
fn the_waiting_list_is_the_unconfirmed_loose_values_in_key_order() {
    let dir = tempfile::tempdir().unwrap();
    let file = "[memory.retention]\nsearch_days = 3\n[memory.files]\nat_rest = \"plain\"\n";
    let locator = locator_with(dir.path(), file);
    let waiting = |key: &str, waiting: &str, shipped: &str| almanac_core::WaitingSetting {
        key: key.to_owned(),
        waiting: waiting.to_owned(),
        shipped: shipped.to_owned(),
    };
    assert_eq!(
        locator.waiting(),
        [
            waiting("memory.files.at_rest", "plain", "sealed"),
            waiting("memory.retention.search_days", "3", "30"),
        ]
    );

    // A confirmed value no longer waits; the other still does.
    locator
        .confirm(&pairs(&[("memory.files.at_rest", "plain")]))
        .unwrap();
    assert_eq!(
        locator.waiting(),
        [waiting("memory.retention.search_days", "3", "30")]
    );
    locator
        .confirm(&pairs(&[("memory.retention.search_days", "3")]))
        .unwrap();
    assert_eq!(locator.waiting(), vec![]);

    // A value that is not looser, or not valid, never waits.
    std::fs::write(
        dir.path().join(SETTINGS_FILE),
        "[memory.retention]\nsearch_days = 40\nsession_days = 0\n",
    )
    .unwrap();
    assert_eq!(locator.waiting(), vec![]);
}

#[test]
fn a_confirmation_names_a_loosenable_key_and_a_value_it_takes_or_none_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let locator = locator_with(dir.path(), "");
    let table: [(&[(&str, &str)], bool); 6] = [
        (&[("memory.retention.search_days", "3")], true),
        (&[("memory.consolidation.apply", "review")], true),
        (&[("memory.consolidation.when", "never")], false),
        (&[("memory.nonsense", "1")], false),
        (&[("memory.retention.search_days", "0")], false),
        (
            &[
                ("memory.files.at_rest", "plain"),
                ("memory.retention.search_days", "many"),
            ],
            false,
        ),
    ];
    for (given, accepted) in table {
        let before = std::fs::read_to_string(dir.path().join(CONFIRMED_FILE)).ok();
        let result = locator.confirm(&pairs(given));
        assert_eq!(result.is_ok(), accepted, "{given:?}: {result:?}");
        if !accepted {
            let after = std::fs::read_to_string(dir.path().join(CONFIRMED_FILE)).ok();
            assert_eq!(after, before, "{given:?} wrote something");
        }
    }
    let nowhere = Locator::from_env(&|_: &str| None::<String>);
    assert!(
        nowhere
            .confirm(&pairs(&[("memory.retention.search_days", "3")]))
            .is_err()
    );
}
