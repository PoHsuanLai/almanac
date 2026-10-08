//! Who is calling: the callers file, the identification of a process by its cgroup over a fake
//! `/proc`, and the real resolution of a bus connection to a process on a private bus where the
//! caller is this test binary.

use crate::support::bus::PrivateBus;
use crate::support::{SharedKeys, TestBackend, connect, dirs_in, service, space};
use almanac_client::{ClientError, DbusTransport, Memory};
use almanac_core::*;
use almanac_dbus::serve_on;
use memoryd::{Daemon, FollowUp, ProcPeers, caller_for, follow_ups, load_callers, table_from_toml};
use porter_dbus::{CallerTable, ProcCallers};
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

[[caller]]
app = "org.quire.Shell"
unit = "sill-shell.scope"
role = "sheet_host"

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
        // The shell's transient scope matches its row exactly; near misses are refused.
        (21, "sill-shell.scope"),
        (22, "sill-shell-2.scope"),
        (23, "xsill-shell.scope"),
    ] {
        put_cgroup(root, pid, leaf);
    }
    // A Flatpak app is named by its sandbox's metadata, never by its scope's name: pid 15 has
    // the metadata, pid 24 only the scope.
    put_flatpak_info(root, 15, "org.example.Mail");
    put_member(root, 24, "app-flatpak-org.example.Mail-4243.scope");
    // Another process in the router's unit is not the router: only the main process is.
    put_member(root, 25, "intentd.service");
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
        (21, Some(Caller::ShellUi)),
        (22, None),
        (23, None),
        (24, None),
        (25, None),
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
    // The same shell app with the `app` role is an app: the role decides, not the name alone.
    let demoted = table_from_toml(
        "[[caller]]\napp = \"org.quire.Shell\"\nunit = \"sill.service\"\nrole = \"app\"\n",
    )
    .expect("table");
    assert_eq!(
        demoted.resolve_unit("sill.service").map(caller_for),
        app("org.quire.Shell", Isolation::Unsandboxed)
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
async fn the_shell_in_its_transient_scope_may_forget() {
    let table = table_from_toml(TABLE).expect("table");
    let proc = proc_with_me_in("sill-shell.scope");
    let (_scratch, _bus, _daemon, address) = world(table, proc.path()).await;
    let shell = Memory::over(DbusTransport::new(connect(&address).await));
    let refused = shell
        .forget(PlanToken::parse("p-1").expect("token"))
        .await
        .expect_err("no such plan");
    assert!(matches!(refused, ClientError::Refused(Refusal::Invalid(_))));
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

fn recorded() -> (MemoryRequest, EventRef) {
    let record = almanac_fake::mail_thread_archived().expect("fixture");
    let event = EventRef {
        space: space("work"),
        replica: ReplicaId([1; 16]),
        seq: Seq(1),
    };
    (MemoryRequest::Record(record), event)
}

#[test]
fn what_a_reply_makes_the_bus_say() {
    let (request, event) = recorded();
    let follow = follow_ups(&request, &MemoryReply::Recorded(event.clone()));
    assert_eq!(follow.len(), 1);
    assert!(matches!(
        &follow[0],
        FollowUp::Emit(almanac_dbus::Signal::Recorded { space, kind, .. })
            if space == "work" && kind == "thing.archived"
    ));
    assert!(
        follow_ups(&request, &MemoryReply::Ok).is_empty(),
        "nothing stored, nothing to say"
    );
    assert!(follow_ups(&request, &MemoryReply::Refused(Refusal::Busy)).is_empty());
    let pause = MemoryRequest::Pause(space("work"), UnixSeconds(1));
    assert_eq!(
        follow_ups(&pause, &MemoryReply::Ok),
        vec![FollowUp::StatusChanged(space("work"))]
    );
    let propose = MemoryRequest::Propose(
        space("work"),
        FactDraft {
            topic: TopicPath::parse("a").expect("topic"),
            text: FactText::parse("x").expect("text"),
            links: vec![],
            supersedes: vec![],
        },
    );
    let fact = FactId::mint(1, [1; 10]);
    assert_eq!(
        follow_ups(
            &propose,
            &MemoryReply::Proposed(fact.clone(), FactState::Pending)
        ),
        vec![FollowUp::PendingChanged(space("work"))]
    );
    assert!(follow_ups(&propose, &MemoryReply::Proposed(fact, FactState::Active)).is_empty());
}

#[test]
fn the_proc_root_knob_works_in_a_test_build_only() {
    use memoryd::{ProcRoot, TestProcRoot, proc_root_choice};
    use std::path::PathBuf;
    for (var, build, expected) in [
        (None, TestProcRoot::Built, ProcRoot::System),
        (None, TestProcRoot::NotBuilt, ProcRoot::System),
        (Some(""), TestProcRoot::Built, ProcRoot::System),
        (
            Some("/x/proc"),
            TestProcRoot::Built,
            ProcRoot::Fixture(PathBuf::from("/x/proc")),
        ),
        (
            Some("/x/proc"),
            TestProcRoot::NotBuilt,
            ProcRoot::SystemIgnoring("/x/proc".to_owned()),
        ),
    ] {
        assert_eq!(proc_root_choice(var, build), expected, "{var:?} {build:?}");
    }
    let ignored = proc_root_choice(Some("/x/proc"), TestProcRoot::NotBuilt);
    assert_eq!(ignored.path(), PathBuf::from("/proc"));
    assert!(ignored.said().is_some_and(|l| l.contains("ignoring it")));
    assert!(ProcRoot::System.said().is_none());
}
