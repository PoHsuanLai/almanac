//! The schema, the key table and the reader hold to one another.

use super::keys::{Rule, table};
use super::*;
use almanac_core::{DayCount, VaultKind};
use std::collections::BTreeSet;

fn schema() -> toml::Table {
    SCHEMA.parse().expect("the schema is TOML")
}

fn rows() -> Vec<toml::Table> {
    schema()["key"]
        .as_array()
        .expect("key tables")
        .iter()
        .map(|k| k.as_table().expect("a table").clone())
        .collect()
}

fn path(row: &toml::Table) -> &str {
    row["path"].as_str().expect("path")
}

fn kind_of(row: &toml::Table) -> (&str, &toml::Table) {
    let kind = row["kind"].as_table().expect("kind");
    (
        kind["kind"].as_str().expect("kind name"),
        kind["v"].as_table().expect("kind v"),
    )
}

fn words(v: &toml::Table) -> Vec<&str> {
    v["variants"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(toml::Value::as_str)
        .collect()
}

/// A document with one key set to `value`.
fn doc(path: &str, value: toml::Value) -> toml::Table {
    let mut table = toml::Table::new();
    let mut parts: Vec<&str> = path.split('.').collect();
    let leaf = parts.pop().expect("a leaf");
    let mut cursor = &mut table;
    for part in parts {
        cursor = cursor
            .entry(part)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .expect("a table");
    }
    cursor.insert(leaf.to_owned(), value);
    table
}

fn merge(into: &mut toml::Table, key: String, value: toml::Value) {
    match (into.get_mut(&key), value) {
        (Some(toml::Value::Table(have)), toml::Value::Table(more)) => {
            for (k, v) in more {
                merge(have, k, v);
            }
        }
        (_, value) => {
            into.insert(key, value);
        }
    }
}

fn merged(parts: &[(&str, toml::Value)]) -> String {
    let mut text = toml::Table::new();
    for (p, v) in parts {
        for (k, v) in doc(p, v.clone()) {
            merge(&mut text, k, v);
        }
    }
    text.to_string()
}

fn one(p: &str, v: toml::Value) -> String {
    merged(&[(p, v)])
}

#[test]
fn the_schema_has_the_shape_the_settings_app_loads() {
    let schema = schema();
    assert_eq!(schema["app"].as_str(), Some("almanac"));
    assert_eq!(schema["file"].as_str(), Some(SETTINGS_FILE));
    assert_eq!(schema["version"].as_integer(), Some(1));
    let mut seen = BTreeSet::new();
    for row in rows() {
        let p = path(&row);
        assert!(seen.insert(p.to_owned()), "duplicate {p}");
        assert!(p.starts_with("memory."), "{p}");
        for field in ["label", "help", "section"] {
            assert!(!row[field].as_str().expect(field).is_empty(), "{p} {field}");
        }
        assert!(
            matches!(row["exposure"].as_str(), Some("basic" | "advanced")),
            "{p}"
        );
        assert_eq!(row["page"]["kind"].as_str(), Some("intelligence"), "{p}");
        assert!(row.get("agent").is_none(), "{p} is never agent-settable");
        match kind_of(&row) {
            ("bounded", v) => {
                let (min, max) = (
                    v["min"].as_integer().unwrap(),
                    v["max"].as_integer().unwrap(),
                );
                let default = row["default"].as_integer().expect("a number default");
                assert!((min..=max).contains(&default), "{p}");
                assert!(!v["unit"].as_str().unwrap_or("").is_empty(), "{p} unit");
            }
            (kind @ ("toggle" | "segmented"), v) => {
                let words = words(v);
                assert_eq!(words.len() == 2, kind == "toggle", "{p}");
                assert!(
                    words.contains(&row["default"].as_str().expect("a word default")),
                    "{p}"
                );
                let labels = row["labels"].as_table().expect("labels for words");
                assert!(words.iter().all(|w| labels.contains_key(*w)), "{p}");
            }
            other => panic!("{p}: kind {other:?}"),
        }
    }
}

#[test]
fn the_intelligence_page_shows_two_memory_rows_and_the_rest_is_advanced() {
    let shown: Vec<String> = rows()
        .iter()
        .filter(|r| r["exposure"].as_str() == Some("basic"))
        .map(|r| path(r).to_owned())
        .collect();
    assert_eq!(shown, ["memory.files.at_rest", "memory.consolidation.when"]);
}

#[test]
fn the_schema_rows_and_the_key_table_are_the_same_keys_with_the_same_ranges() {
    let keys = table();
    let rows = rows();
    assert_eq!(
        rows.iter().map(path).collect::<Vec<_>>(),
        keys.iter().map(|k| k.path).collect::<Vec<_>>()
    );
    for (row, key) in rows.iter().zip(&keys) {
        let (_, v) = kind_of(row);
        match &key.rule {
            Rule::Number { range, .. } => {
                assert_eq!(v["min"].as_integer(), Some(*range.start()), "{}", key.path);
                assert_eq!(v["max"].as_integer(), Some(*range.end()), "{}", key.path);
            }
            Rule::Word { words: w, .. } => assert_eq!(&words(v), w, "{}", key.path),
        }
    }
}

#[test]
fn a_file_of_every_schema_default_changes_nothing() {
    let parts: Vec<(String, toml::Value)> = rows()
        .iter()
        .map(|r| (path(r).to_owned(), r["default"].clone()))
        .collect();
    let refs: Vec<(&str, toml::Value)> =
        parts.iter().map(|(p, v)| (p.as_str(), v.clone())).collect();
    let loaded = read(&merged(&refs), MemorySettings::default());
    assert_eq!(loaded.value, MemorySettings::default());
    assert_eq!((loaded.fallbacks, loaded.unknown), (vec![], vec![]));
}

/// A valid value for the row that is not its default.
fn other_than_default(row: &toml::Table) -> toml::Value {
    match kind_of(row) {
        (_, v) if v.contains_key("min") => {
            let default = row["default"].as_integer().unwrap();
            let (min, max) = (
                v["min"].as_integer().unwrap(),
                v["max"].as_integer().unwrap(),
            );
            toml::Value::Integer(if default == max { min } else { max })
        }
        (_, v) => {
            let default = row["default"].as_str().unwrap();
            let word = words(v).into_iter().find(|w| *w != default).unwrap();
            toml::Value::String(word.to_owned())
        }
    }
}

#[test]
fn every_schema_key_is_read_by_the_daemon() {
    for row in rows() {
        let p = path(&row);
        let loaded = read(&one(p, other_than_default(&row)), MemorySettings::default());
        assert_eq!(loaded.fallbacks, vec![], "{p}");
        assert_ne!(
            loaded.value,
            MemorySettings::default(),
            "{p} changed nothing"
        );
    }
}

#[test]
fn each_key_lands_in_the_typed_value_the_design_names() {
    let read_one = |p: &str, v: toml::Value| read(&one(p, v), MemorySettings::default()).value;
    let int = toml::Value::Integer;
    let word = |w: &str| toml::Value::String(w.to_owned());
    assert_eq!(
        read_one("memory.files.at_rest", word("plain")).at_rest,
        VaultKind::Plain
    );
    assert_eq!(
        read_one("memory.consolidation.apply", word("review")).apply,
        ConsolidateApply::Review
    );
    assert_eq!(
        read_one("memory.consolidation.when", word("never")).consolidate,
        ConsolidateWhen::Never
    );
    assert_eq!(
        read_one("memory.consolidation.when", word("manual")).consolidate,
        ConsolidateWhen::Manual
    );
    let r = |p| read_one(p, int(11)).retention;
    assert_eq!(r("memory.retention.search_days").search, DayCount(11));
    assert_eq!(r("memory.retention.session_days").session, DayCount(11));
}

#[test]
fn a_bad_value_falls_back_for_that_key_alone() {
    for row in rows() {
        let p = path(&row);
        let bad: Vec<toml::Value> = match kind_of(&row) {
            (_, v) if v.contains_key("min") => vec![
                toml::Value::Integer(v["min"].as_integer().unwrap() - 1),
                toml::Value::Integer(v["max"].as_integer().unwrap() + 1),
                toml::Value::String("many".into()),
                toml::Value::Boolean(true),
            ],
            _ => vec![
                toml::Value::String("reckless".into()),
                toml::Value::Integer(1),
                toml::Value::Boolean(true),
            ],
        };
        for value in bad {
            // A second, good key in the same file still applies.
            let other = if p == "memory.retention.session_days" {
                "memory.retention.search_days"
            } else {
                "memory.retention.session_days"
            };
            let text = merged(&[(p, value.clone()), (other, toml::Value::Integer(3))]);
            let loaded = read(&text, MemorySettings::default());
            let mut want = MemorySettings::default();
            if other == "memory.retention.session_days" {
                want.retention.session = DayCount(3);
            } else {
                want.retention.search = DayCount(3);
            }
            assert_eq!(loaded.value, want, "{p} = {value}");
            assert_eq!(loaded.fallbacks.len(), 1, "{p} = {value}");
            assert_eq!(loaded.fallbacks[0].key, p);
        }
    }
}

#[test]
fn a_fallback_is_the_base_value_not_the_default() {
    let mut base = MemorySettings::default();
    base.retention.search = DayCount(5);
    let loaded = read("[memory.retention]\nsearch_days = 0\n", base);
    assert_eq!(loaded.value, base);
    assert_eq!(
        loaded.fallbacks,
        vec![Fallback {
            key: "memory.retention.search_days".into(),
            why: Why::OutOfRange { min: 1, max: 3650 },
        }]
    );
}

#[test]
fn a_file_that_is_not_toml_keeps_every_base_value_and_says_so() {
    let loaded = read("[memory\nfiles = 1", MemorySettings::default());
    assert_eq!(loaded.value, MemorySettings::default());
    assert!(matches!(loaded.fallbacks[0].why, Why::NotToml(_)));
}

#[test]
fn unknown_keys_are_reported_and_the_unread_ones_are_among_them() {
    let loaded = read(
        "version = 1\n[memory]\njoin_window_ms = 500\n[memory.consolidation]\napply = \"review\"\nwhen = \"manual\"\n",
        MemorySettings::default(),
    );
    assert_eq!(loaded.value.consolidate, ConsolidateWhen::Manual);
    assert_eq!(loaded.value.apply, ConsolidateApply::Review);
    assert_eq!(loaded.unknown, ["memory.join_window_ms"]);
}

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

#[test]
fn what_the_file_loosens_below_the_shipped_defaults_is_named() {
    let table: [(&str, usize); 4] = [
        ("", 0),
        ("[memory.retention]\nsearch_days = 30\n", 0),
        ("[memory.retention]\nsearch_days = 1\n", 1),
        (
            "[memory.retention]\nsearch_days = 1\nsession_days = 2\n[memory.files]\nat_rest = \"plain\"\n",
            3,
        ),
    ];
    for (text, count) in table {
        let loaded = read(text, MemorySettings::default());
        assert_eq!(loosenings(&loaded.value).len(), count, "{text}");
        let lines = loaded.lines("memoryd");
        assert_eq!(
            lines.iter().filter(|l| l.contains("loosened")).count(),
            count,
            "{text}"
        );
    }
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
