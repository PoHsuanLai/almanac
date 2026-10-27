//! The person's settings file reaches a running service: a change in the file applies to the next
//! request without a restart. The wait is the watch's own event, never a sleep.

use crate::support::bus::PrivateBus;
use crate::support::{SharedKeys, World, dirs_in};
use almanac_core::{
    AppId, Caller, DayCount, Isolation, MemoryReply, MemoryRequest, Refusal, VaultKind,
    WaitingSetting,
};
use almanac_dbus::{decode_reply, encode_request, invoke};
use almanac_fake::{ScriptedConsolidator, fake_service, mail};
use almanac_host::Locator;
use almanac_settings::{ConsolidateWhen, MemorySettings};
use memoryd::{Consent, SettingsWatch, WatchState, apply, apply_next};
use std::collections::BTreeMap;
use std::path::Path;

fn pairs(of: &[(&str, &str)]) -> BTreeMap<String, String> {
    of.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

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
    // The values the file is about to hold are looser than the shipped ones: the person has
    // confirmed exactly them.
    let store = tempfile::tempdir().unwrap();
    let locator = Locator::from_env(&env).confirming(store.path().join("confirmed-settings.toml"));
    locator
        .confirm(&pairs(&[
            ("memory.files.at_rest", "plain"),
            ("memory.retention.search_days", "3"),
        ]))
        .unwrap();
    let mut watch = SettingsWatch::start(locator, MemorySettings::default());
    assert_eq!(*watch.state(), WatchState::Live);
    apply(&service, &watch.current());
    assert_eq!(service.settings(), MemorySettings::default());

    write_atomically(
        home.path(),
        "[memory.files]\nat_rest = \"plain\"\n[memory.retention]\nsearch_days = 3\n[memory.consolidation]\nwhen = \"never\"\n",
    );
    while service.settings().consolidate == ConsolidateWhen::Nightly {
        let loaded = apply_next(&service, &mut watch)
            .await
            .expect("the watch is live");
        assert_eq!(loaded.fallbacks, vec![]);
    }
    let now = service.settings();
    assert_eq!(now.at_rest, VaultKind::Plain);
    assert_eq!(now.retention.search, DayCount(3));

    // A bad value falls back to the value in force, and the good one beside it still applies.
    write_atomically(
        home.path(),
        "[memory.retention]\nsearch_days = 0\n[memory.consolidation]\nwhen = \"manual\"\n",
    );
    while service.settings().consolidate != ConsolidateWhen::Manual {
        apply_next(&service, &mut watch)
            .await
            .expect("the watch is live");
    }
    let now = service.settings();
    assert_eq!(
        now.retention.search,
        DayCount(30),
        "the bad value fell back to the base, not the old file"
    );
    assert_eq!(
        now.at_rest,
        VaultKind::Sealed,
        "a key the file no longer sets is the base again"
    );
}

/// What the Settings app does after the person confirms: `ConfirmSettings` over the bus. The
/// answer is the refusal, or `None` for an accepted call.
async fn confirm_over_bus(client: &zbus::Connection, values: &[(&str, &str)]) -> Option<Refusal> {
    let call = encode_request(&MemoryRequest::ConfirmSettings(pairs(values))).expect("encode");
    match invoke(client, &call, None).await {
        Ok(_) => None,
        Err(error) => Some(error.refusal().expect("a refusal, not a bus failure")),
    }
}

/// What the Settings app does to show "waits for you to confirm": `PendingSettings` over the bus.
async fn pending_over_bus(client: &zbus::Connection) -> Result<Vec<WaitingSetting>, Refusal> {
    let call = encode_request(&MemoryRequest::PendingSettings).expect("encode");
    let outputs = invoke(client, &call, None)
        .await
        .map_err(|error| error.refusal().expect("a refusal, not a bus failure"))?;
    match decode_reply(&call, &outputs).expect("decode") {
        MemoryReply::PendingSettings(waiting) => Ok(waiting),
        other => panic!("not a waiting list: {other:?}"),
    }
}

#[tokio::test]
async fn a_loosened_value_waits_until_the_settings_app_confirms_it_over_the_bus() {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let dirs = dirs_in(&scratch.path().join("home"));
    let config = scratch.path().join("config");
    write_atomically(&config, "[memory.retention]\nsearch_days = 3\n");
    let nothing = scratch.path().join("no-xdg-dirs");
    let env = |key: &str| match key {
        "XDG_CONFIG_HOME" => Some(config.display().to_string()),
        "XDG_CONFIG_DIRS" => Some(nothing.display().to_string()),
        _ => None,
    };
    let locator = Locator::from_env(&env).confirming(dirs.confirmed_settings_toml());
    let world = World::start_with(
        &bus,
        &dirs,
        &SharedKeys::default(),
        Some(Consent::new(locator.clone())),
    )
    .await;
    let service = world.daemon.service();
    apply(service, &locator.read(MemorySettings::default()));
    assert_eq!(
        service.settings().retention.search,
        MemorySettings::default().retention.search,
        "unconfirmed, the shipped wait stays"
    );

    // Nobody but the Settings app may confirm: not an app, not the router, not even the shell.
    let one = [("memory.retention.search_days", "3")];
    let app = Caller::App(AppId {
        name: mail(),
        isolation: Isolation::Unsandboxed,
    });
    for caller in [app.clone(), Caller::Router, Caller::Cuad, Caller::ShellUi] {
        let client = world.client(caller.clone()).await;
        assert_eq!(
            confirm_over_bus(&client, &one).await,
            Some(Refusal::NotAllowed),
            "{caller:?}"
        );
    }
    assert!(!dirs.confirmed_settings_toml().exists());

    // The waiting list is read by the Settings app and the shell, and by no one else.
    for caller in [app, Caller::Router, Caller::Cuad] {
        let client = world.client(caller.clone()).await;
        assert_eq!(
            pending_over_bus(&client).await,
            Err(Refusal::NotAllowed),
            "{caller:?}"
        );
    }
    let waiting = vec![WaitingSetting {
        key: "memory.retention.search_days".to_owned(),
        waiting: "3".to_owned(),
        shipped: MemorySettings::default().retention.search.0.to_string(),
    }];
    let shell = world.client(Caller::ShellUi).await;
    assert_eq!(pending_over_bus(&shell).await, Ok(waiting.clone()));

    // From the Settings app, a key that cannot loosen anything and a value the key does not take
    // are refused, and a refused pair keeps the good one beside it out too.
    let settings = world.client(Caller::Settings).await;
    for bad in [
        vec![("memory.nonsense", "1")],
        vec![("memory.consolidation.when", "never")],
        vec![("memory.retention.search_days", "0")],
        vec![
            ("memory.retention.search_days", "3"),
            ("memory.files.at_rest", "open"),
        ],
    ] {
        let refused = confirm_over_bus(&settings, &bad).await;
        assert!(
            matches!(refused, Some(Refusal::Invalid(_))),
            "{bad:?}: {refused:?}"
        );
    }
    assert!(!dirs.confirmed_settings_toml().exists());
    assert_eq!(
        service.settings().retention.search,
        MemorySettings::default().retention.search
    );

    // The Settings app confirms: the very next request sees the value, and it is on disk.
    assert_eq!(pending_over_bus(&settings).await, Ok(waiting));
    assert_eq!(confirm_over_bus(&settings, &one).await, None);
    assert_eq!(pending_over_bus(&settings).await, Ok(vec![]));
    assert_eq!(service.settings().retention.search, DayCount(3));
    assert!(
        std::fs::read_to_string(dirs.confirmed_settings_toml())
            .expect("the store")
            .contains("memory.retention.search_days")
    );
}
