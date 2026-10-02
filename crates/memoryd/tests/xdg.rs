//! The XDG roots, from a map: tests never read the real environment.

use almanac_core::SpaceId;
use memoryd::*;
use std::collections::BTreeMap;

fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: BTreeMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn explicit_variables_win() {
    let dirs = dirs_from(lookup(&[
        ("XDG_DATA_HOME", "/d"),
        ("XDG_CACHE_HOME", "/c"),
        ("XDG_CONFIG_HOME", "/f"),
        ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ("HOME", "/home/u"),
    ]))
    .expect("dirs");
    assert_eq!(dirs.memory().to_str(), Some("/d/quire/memory"));
    assert_eq!(dirs.memory_toml().to_str(), Some("/f/quire/memory.toml"));
    let work = SpaceId::parse("work").expect("space");
    assert_eq!(
        dirs.index_db(&work).to_str(),
        Some("/c/quire/memory/work/index.db")
    );
    assert_eq!(
        dirs.edit(&work).to_str(),
        Some("/run/user/1000/quire/memory/edit/work")
    );
}

#[test]
fn defaults_come_from_home() {
    let dirs = dirs_from(lookup(&[
        ("HOME", "/home/u"),
        ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ("XDG_DATA_HOME", ""),
    ]))
    .expect("dirs");
    assert_eq!(
        dirs.memory().to_str(),
        Some("/home/u/.local/share/quire/memory")
    );
    assert_eq!(
        dirs.memory_toml().to_str(),
        Some("/home/u/.config/quire/memory.toml")
    );
}

#[test]
fn missing_roots_are_errors_not_guesses() {
    assert_eq!(
        dirs_from(lookup(&[("XDG_RUNTIME_DIR", "/run")])),
        Err(XdgError::NoHome("XDG_DATA_HOME"))
    );
    assert_eq!(
        dirs_from(lookup(&[("HOME", "/home/u")])),
        Err(XdgError::NoRuntimeDir)
    );
}

#[test]
fn the_system_clock_is_after_the_fixture_epoch() {
    use almanac_service::Clock;
    assert!(SystemClock.now().0 > 1_790_000_000 - 100_000_000);
}
