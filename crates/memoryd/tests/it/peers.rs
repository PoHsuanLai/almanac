//! Who is calling: the callers file, the identification of a process by its cgroup over a fake
//! `/proc`, and the real resolution of a bus connection to a process on a private bus where the
//! caller is this test binary.

use crate::support::bus::PrivateBus;
use crate::support::{SharedKeys, TestBackend, connect, dirs_in, service, space};
use almanac_client::{ClientError, DbusTransport, Memory};
use almanac_core::*;
use almanac_dbus_serve::{Signal, serve_on};
use almanac_tidy::Change;
use memoryd::{
    Daemon, Delivery, ProcPeers, caller_for, delivery_of, load_callers, table_from_toml,
};
use porter_dbus::callers::{CallerTable, ProcCallers};
use std::path::Path;
use std::sync::Arc;

const TABLE: &str = r#"
[[caller]]
app = "org.quire.Intents"
unit = "intentd.service"
role = "agent"

[[caller]]
app = "org.quire.Cua"
unit = "cuad.service"
role = "cua"

[[caller]]
app = "org.quire.Shell"
unit = "sill.service"
role = "sheet_host"
exe = ["/usr/bin/sill", "/usr/local/bin/sill", "/opt/quire/releases/*/bin/sill"]

[[caller]]
app = "org.quire.Settings"
unit = "org.quire.Settings.service"
role = "settings"
exe = ["/usr/bin/detent", "/usr/local/bin/detent", "/opt/quire/releases/*/bin/detent"]

# A scope never carries a fixed role: this row makes the shell's old scope a plain app.
[[caller]]
app = "org.quire.Shell"
unit = "sill-shell.scope"
role = "sheet_host"
exe = ["/usr/bin/sill", "/usr/local/bin/sill", "/opt/quire/releases/*/bin/sill"]

[[caller]]
app = "org.quire.Companion"
unit = "companiond.service"
role = "agent"
"#;

fn app(name: &str, isolation: Isolation) -> Option<Caller> {
    Some(Caller::App(AppId {
        name: AppName::parse(name).expect("app"),
        isolation,
    }))
}

/// Writes `<root>/<pid>/cgroup` with the unit `leaf` under a user slice. A service's process is
/// also its main process (`<root>/units/<leaf>`, the fixture's stand-in for the manager's
/// `MainPID`).
fn put_cgroup(root: &Path, pid: u32, leaf: &str) {
    put_member(root, pid, leaf);
    if leaf.ends_with(".service") {
        let units = root.join("units");
        std::fs::create_dir_all(&units).expect("units dir");
        std::fs::write(units.join(leaf), pid.to_string()).expect("main pid");
    }
}

/// Writes `<root>/<pid>/cgroup` alone: a process in `leaf` that is not its main process.
fn put_member(root: &Path, pid: u32, leaf: &str) {
    let dir = root.join(pid.to_string());
    std::fs::create_dir_all(&dir).expect("pid dir");
    let path = format!("0::/user.slice/user-1000.slice/user@1000.service/app.slice/{leaf}\n");
    std::fs::write(dir.join("cgroup"), path).expect("cgroup");
}

/// Links `<root>/<pid>/exe` to `image`: the program process `pid` runs.
fn put_image(root: &Path, pid: u32, image: &str) {
    let dir = root.join(pid.to_string());
    std::fs::create_dir_all(&dir).expect("pid dir");
    std::os::unix::fs::symlink(image, dir.join("exe")).expect("exe link");
}

/// Writes the Flatpak sandbox metadata of process `pid` (`<pid>/root/.flatpak-info`).
fn put_flatpak_info(root: &Path, pid: u32, app: &str) {
    let dir = root.join(pid.to_string()).join("root");
    std::fs::create_dir_all(&dir).expect("root dir");
    let info = format!("[Application]\nname={app}\n\n[Instance]\ninstance-id=4242\n");
    std::fs::write(dir.join(".flatpak-info"), info).expect("flatpak info");
}

fn identify(root: &Path, pid: u32, table: &CallerTable) -> Option<Caller> {
    ProcCallers::caller_of_pid(root, pid, table).map(caller_for)
}

#[test]
fn a_process_is_identified_by_its_cgroup() {
    let table = table_from_toml(TABLE).expect("table");
    let scratch = tempfile::tempdir().expect("scratch");
    let root = scratch.path();
    for (pid, leaf) in [
        (10, "intentd.service"),
        (11, "cuad.service"),
        (12, "sill.service"),
        (13, "companiond.service"),
        (14, "curl.service"),
        (15, "app-flatpak-org.example.Mail-4242.scope"),
        (16, "app-org.example.Notes-77.scope"),
        (17, "app-gnome-org.example.Notes-78.scope"),
        // A terminal's child: the terminal's app scope names the terminal, not the program.
        (18, "vte-spawn-3f1e.scope"),
        // An app scope named for the router's app is only an app: the unit row gives its role
        // to that unit alone.
        (19, "app-org.quire.Intents-5.scope"),
        // The shell's old transient scope matches its row exactly, but as a plain app only;
        // near misses are refused.
        (21, "sill-shell.scope"),
        (22, "sill-shell-2.scope"),
        (23, "xsill-shell.scope"),
        // The Settings app is a unit row; its app scope is only an app.
        (26, "org.quire.Settings.service"),
        (27, "app-org.quire.Settings-3.scope"),
    ] {
        put_cgroup(root, pid, leaf);
    }
    // A Flatpak app is named by its sandbox's metadata, never by its scope's name: pid 15 has
    // the metadata, pid 24 only the scope.
    put_flatpak_info(root, 15, "org.example.Mail");
    put_member(root, 24, "app-flatpak-org.example.Mail-4243.scope");
    // Another process in the router's unit is not the router: only the main process is.
    put_member(root, 25, "intentd.service");
    // A scope has no main process: the shell's old scope is matched by the program it runs.
    put_image(root, 21, "/usr/bin/sill");
    let flatpak = app("org.example.Mail", Isolation::Flatpak);
    for (pid, expected) in [
        (10, Some(Caller::Router)),
        (11, Some(Caller::Cuad)),
        (12, Some(Caller::ShellUi)),
        (13, app("org.quire.Companion", Isolation::Unsandboxed)),
        (14, None),
        (15, flatpak),
        (16, app("org.example.Notes", Isolation::Unsandboxed)),
        (17, app("org.example.Notes", Isolation::Unsandboxed)),
        (18, None),
        (19, app("org.quire.Intents", Isolation::Unsandboxed)),
        (20, None),
        (21, app("org.quire.Shell", Isolation::Unsandboxed)),
        (22, None),
        (23, None),
        (24, None),
        (25, None),
        (26, Some(Caller::Settings)),
        (27, app("org.quire.Settings", Isolation::Unsandboxed)),
    ] {
        assert_eq!(identify(root, pid, &table), expected, "pid {pid}");
    }
}

#[test]
fn an_empty_table_names_no_unit_and_an_app_is_still_an_app() {
    let scratch = tempfile::tempdir().expect("scratch");
    put_cgroup(scratch.path(), 1, "intentd.service");
    put_cgroup(scratch.path(), 2, "app-org.example.Notes-7.scope");
    let none = CallerTable::default();
    assert_eq!(identify(scratch.path(), 1, &none), None);
    assert_eq!(
        identify(scratch.path(), 2, &none),
        app("org.example.Notes", Isolation::Unsandboxed)
    );
}

#[test]
fn the_roles_map_onto_almanacs_callers() {
    let table = table_from_toml(TABLE).expect("table");
    let named = |unit: &str| table.resolve_unit(unit).map(caller_for);
    assert_eq!(named("intentd.service"), Some(Caller::Router));
    assert_eq!(named("sill.service"), Some(Caller::ShellUi));
    assert_eq!(named("cuad.service"), Some(Caller::Cuad));
    assert_eq!(named("org.quire.Settings.service"), Some(Caller::Settings));
    // The same shell app with the `app` role is an app: the role decides, not the name alone.
    let demoted = table_from_toml(
        "[[caller]]\napp = \"org.quire.Shell\"\nunit = \"sill.service\"\nrole = \"app\"\n",
    )
    .expect("table");
    assert_eq!(
        demoted.resolve_unit("sill.service").map(caller_for),
        app("org.quire.Shell", Isolation::Unsandboxed)
    );
    // The `settings` role of any other app is that app, not the Settings app.
    let other = table_from_toml(
        "[[caller]]\napp = \"org.example.Other\"\nunit = \"other.service\"\nrole = \"settings\"\n",
    )
    .expect("table");
    assert_eq!(
        other.resolve_unit("other.service").map(caller_for),
        app("org.example.Other", Isolation::Unsandboxed)
    );
}

#[test]
fn the_user_file_wins_a_missing_one_is_empty_and_a_bad_one_is_an_error() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dir = scratch.path();
    let (system, user, none) = (
        dir.join("system.toml"),
        dir.join("user.toml"),
        dir.join("none.toml"),
    );
    std::fs::write(&system, TABLE).expect("system");
    std::fs::write(
        &user,
        "[[caller]]\napp = \"org.example.Mine\"\nunit = \"intentd.service\"\nrole = \"app\"\n",
    )
    .expect("user");
    let merged = load_callers(&system, &user).expect("merged");
    assert_eq!(
        merged.resolve_unit("intentd.service").map(caller_for),
        app("org.example.Mine", Isolation::Unsandboxed)
    );
    assert_eq!(
        load_callers(&none, &none).expect("empty"),
        CallerTable::default()
    );
    std::fs::write(&user, "[[caller]]\napp = \"org.x.A\"\nrole = \"root\"\n").expect("bad");
    assert!(load_callers(&system, &user).is_err());
    assert!(table_from_toml("[[caller]]\napp = \"Not An App!\"\nrole = \"app\"\n").is_err());
}

async fn world(
    table: CallerTable,
    proc_root: &Path,
) -> (
    tempfile::TempDir,
    PrivateBus,
    Arc<Daemon<TestBackend, ProcPeers>>,
    String,
) {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let dirs = dirs_in(&scratch.path().join("home"));
    let server = connect(&bus.address).await;
    let peers = ProcPeers::with_proc_root(server.clone(), table, proc_root.to_owned());
    let daemon = Arc::new(Daemon::new(
        service(&dirs, &SharedKeys::default()),
        peers,
        dirs,
    ));
    serve_on(&server, daemon.clone()).await.expect("serve");
    daemon.attach(server);
    let address = bus.address.clone();
    (scratch, bus, daemon, address)
}

fn search() -> RecallQuery {
    RecallQuery {
        space: space("work"),
        text: "budget".into(),
        limit: Count(3),
        over: RecallOver::Both,
    }
}

/// A fake `/proc` in which this test binary runs in `leaf`.
fn proc_with_me_in(leaf: &str) -> tempfile::TempDir {
    let proc = tempfile::tempdir().expect("proc");
    put_cgroup(proc.path(), std::process::id(), leaf);
    proc
}

#[tokio::test]
async fn a_connection_is_resolved_through_its_process_to_its_cgroup() {
    let table = table_from_toml(TABLE).expect("table");
    // This test binary is intentd: its search is allowed.
    let proc = proc_with_me_in("intentd.service");
    let (_scratch, _bus, _daemon, address) = world(table.clone(), proc.path()).await;
    let router = Memory::over(DbusTransport::new(connect(&address).await));
    assert!(router.search(search()).await.is_ok());
    // And the router may not do what only the shell may.
    assert_eq!(
        router
            .forget(PlanToken::parse("p-1").expect("token"))
            .await
            .expect_err("the router cannot forget"),
        ClientError::Refused(Refusal::NotAllowed)
    );

    // The same binary as the shell may forget (a made-up plan is `Invalid`, not `NotAllowed`).
    let proc = proc_with_me_in("sill.service");
    let (_scratch2, _bus2, _daemon2, address) = world(table, proc.path()).await;
    let shell = Memory::over(DbusTransport::new(connect(&address).await));
    let refused = shell
        .forget(PlanToken::parse("p-1").expect("token"))
        .await
        .expect_err("no such plan");
    assert!(matches!(refused, ClientError::Refused(Refusal::Invalid(_))));
}

#[tokio::test]
async fn the_shell_in_its_old_transient_scope_may_not_forget() {
    let table = table_from_toml(TABLE).expect("table");
    let proc = proc_with_me_in("sill-shell.scope");
    let (_scratch, _bus, _daemon, address) = world(table, proc.path()).await;
    let shell = Memory::over(DbusTransport::new(connect(&address).await));
    let refused = shell
        .forget(PlanToken::parse("p-1").expect("token"))
        .await
        .expect_err("only the shell's service may forget");
    assert_eq!(refused, ClientError::Refused(Refusal::NotAllowed));
}

#[tokio::test]
async fn a_process_nothing_names_is_nobody() {
    let table = table_from_toml(TABLE).expect("table");
    let refused = |leaf: &'static str, table: CallerTable| async move {
        let proc = proc_with_me_in(leaf);
        let (_scratch, _bus, _daemon, address) = world(table, proc.path()).await;
        let stranger = Memory::over(DbusTransport::new(connect(&address).await));
        stranger.search(search()).await.expect_err(leaf)
    };
    let not_allowed = ClientError::Refused(Refusal::NotAllowed);
    // A unit the table does not name, a terminal's child, an empty table.
    assert_eq!(refused("curl.service", table.clone()).await, not_allowed);
    assert_eq!(refused("vte-spawn-3.scope", table).await, not_allowed);
    assert_eq!(
        refused("intentd.service", CallerTable::default()).await,
        not_allowed
    );
    // No cgroup file for the process at all.
    let empty = tempfile::tempdir().expect("proc");
    let (_scratch, _bus, _daemon, address) =
        world(table_from_toml(TABLE).expect("table"), empty.path()).await;
    let stranger = Memory::over(DbusTransport::new(connect(&address).await));
    assert_eq!(
        stranger.search(search()).await.expect_err("nobody"),
        not_allowed
    );
}

#[test]
fn what_a_change_makes_the_bus_say() {
    let event = EventRef {
        space: space("work"),
        replica: ReplicaId([1; 16]),
        seq: Seq(1),
    };
    let recorded = Change::Recorded {
        event: event.clone(),
        kind: "thing.archived".to_owned(),
    };
    assert!(matches!(
        delivery_of(recorded),
        Some(Delivery::Emit(Signal::Recorded { space, kind, event_ref }))
            if space == "work"
                && kind == "thing.archived"
                && event_ref == serde_json::to_string(&event).expect("json")
    ));
    let run = RunId::parse("c-0123456789abcdef0123").expect("run");
    assert_eq!(
        delivery_of(Change::ConsolidationReady {
            space: space("work"),
            run
        }),
        Some(Delivery::Emit(Signal::ConsolidationReady {
            space: "work".to_owned(),
            run: "c-0123456789abcdef0123".to_owned()
        }))
    );
    assert_eq!(
        delivery_of(Change::PendingChanged(space("work"))),
        Some(Delivery::PendingChanged(space("work")))
    );
    assert_eq!(
        delivery_of(Change::StatusChanged(space("work"))),
        Some(Delivery::StatusChanged(space("work")))
    );
}

#[test]
fn the_proc_gate_follows_the_test_proc_root_feature() {
    let expected = if cfg!(feature = "test-proc-root") {
        memoryd::ProcGate::Honour
    } else {
        memoryd::ProcGate::Ignore
    };
    assert_eq!(memoryd::PROC_GATE, expected);
}
