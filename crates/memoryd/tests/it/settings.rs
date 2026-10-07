//! The person's settings file reaches a running service: a change in the file applies to the next
//! request without a restart. The wait is the watch's own event, never a sleep.

use almanac_core::{DayCount, VaultKind};
use almanac_fake::{ScriptedConsolidator, fake_service};
use almanac_service::{ConsolidateWhen, Locator, MemorySettings};
use memoryd::{SettingsWatch, WatchState, apply, apply_next};
use std::path::Path;

/// What the Settings app does: a temp file, then a rename over the settings file.
fn write_atomically(home: &Path, text: &str) {
    let dir = home.join("almanac");
    std::fs::create_dir_all(&dir).unwrap();
    let temp = dir.join("settings.toml.tmp");
    std::fs::write(&temp, text).unwrap();
    std::fs::rename(&temp, dir.join("settings.toml")).unwrap();
}

#[tokio::test]
async fn a_change_in_the_file_applies_to_the_service_without_a_restart() {
    let home = tempfile::tempdir().unwrap();
    let nothing = tempfile::tempdir().unwrap();
    let env = |key: &str| match key {
        "XDG_CONFIG_HOME" => Some(home.path().display().to_string()),
        "XDG_CONFIG_DIRS" => Some(nothing.path().display().to_string()),
        _ => None,
    };
    let service = fake_service(ScriptedConsolidator::default());
    let mut watch = SettingsWatch::start(Locator::from_env(&env), MemorySettings::default());
    assert_eq!(*watch.state(), WatchState::Live);
    apply(&service, &watch.current());
    assert_eq!(service.settings(), MemorySettings::default());

    write_atomically(
        home.path(),
        "[memory.files]\nat_rest = \"plain\"\n[memory]\npending_ttl_days = 3\n[memory.consolidation]\nwhen = \"never\"\n",
    );
    while service.settings().consolidate == ConsolidateWhen::Nightly {
        let loaded = apply_next(&service, &mut watch)
            .await
            .expect("the watch is live");
        assert_eq!(loaded.fallbacks, vec![]);
    }
    let now = service.settings();
    assert_eq!(now.at_rest, VaultKind::Plain);
    assert_eq!(now.pending_ttl, DayCount(3));

    // A bad value falls back to the value in force, and the good one beside it still applies.
    write_atomically(
        home.path(),
        "[memory]\npending_ttl_days = 0\n[memory.consolidation]\nwhen = \"manual\"\n",
    );
    while service.settings().consolidate != ConsolidateWhen::Manual {
        apply_next(&service, &mut watch)
            .await
            .expect("the watch is live");
    }
    let now = service.settings();
    assert_eq!(
        now.pending_ttl,
        DayCount(14),
        "the bad value fell back to the base, not the old file"
    );
    assert_eq!(
        now.at_rest,
        VaultKind::Sealed,
        "a key the file no longer sets is the base again"
    );
}
