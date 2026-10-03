//! Who is calling: the caller table, and the real resolution of a bus connection to a process
//! and its executable, on a private bus where the caller is this test binary.

mod common;

use almanac_client::{ClientError, DbusTransport, Memory};
use almanac_core::*;
use almanac_dbus::serve_on;
use common::bus::PrivateBus;
use common::{SharedKeys, TestBackend, connect, dirs_in, service, space};
use memoryd::{CallerTable, Daemon, FollowUp, ProcPeers, follow_ups};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TABLE: &str = r#"
router = ["/usr/libexec/quire/intentd"]
shell  = ["/usr/bin/sill"]
cuad   = ["/usr/libexec/quire/cuad"]

[apps]
"org.quire.Mail" = ["/usr/bin/mailo", "/opt/mailo/bin/mailo"]
"org.quire.Router" = ["/usr/libexec/quire/intentd"]
"#;

#[test]
fn the_table_names_the_caller_of_each_executable() {
    let table = CallerTable::from_toml(TABLE).expect("table");
    let app = |name: &str| {
        Some(Caller::App(AppId {
            name: AppName::parse(name).expect("app"),
            isolation: Isolation::Unsandboxed,
        }))
    };
    for (exe, expected) in [
        ("/usr/libexec/quire/intentd", Some(Caller::Router)),
        ("/usr/bin/sill", Some(Caller::ShellUi)),
        ("/usr/libexec/quire/cuad", Some(Caller::Cuad)),
        ("/usr/bin/mailo", app("org.quire.Mail")),
        ("/opt/mailo/bin/mailo", app("org.quire.Mail")),
        ("/usr/bin/curl", None),
        ("/usr/bin/sill (deleted)", None),
    ] {
        assert_eq!(table.resolve(Path::new(exe)), expected, "{exe}");
    }
    // An app entry cannot claim a fixed role's executable: the role wins.
    assert_eq!(
        table.resolve(Path::new("/usr/libexec/quire/intentd")),
        Some(Caller::Router)
    );
}

#[test]
fn an_empty_table_allows_nobody_and_a_bad_one_is_refused() {
    assert_eq!(
        CallerTable::default().resolve(Path::new("/usr/bin/sill")),
        None
    );
    assert_eq!(
        CallerTable::from_toml("").expect("empty is fine"),
        CallerTable::default()
    );
    assert!(CallerTable::from_toml("router = 3").is_err());
    assert!(CallerTable::from_toml("[apps]\n\"Not An App!\" = []").is_err());
}

async fn world(
    table: CallerTable,
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
    let peers = ProcPeers::new(server.clone(), table);
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

fn this_binary() -> PathBuf {
    std::env::current_exe().expect("the test binary")
}

fn table_with(role: &str, exe: &Path) -> CallerTable {
    CallerTable::from_toml(&format!("{role} = [{:?}]", exe.display().to_string())).expect("table")
}

fn search() -> RecallQuery {
    RecallQuery {
        space: space("work"),
        text: "budget".into(),
        limit: Count(3),
        over: RecallOver::Both,
    }
}

#[tokio::test]
async fn a_connection_is_resolved_through_its_process_to_its_executable() {
    // This test binary is the router: its search is allowed.
    let (_scratch, _bus, _daemon, address) = world(table_with("router", &this_binary())).await;
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

    // The same binary as the shell may forget (a made-up plan is `Invalid`, not `NotAllowed`) but
    // may not be told apart from a router for reads: shell reads are allowed too.
    let (_scratch2, _bus2, _daemon2, address) = world(table_with("shell", &this_binary())).await;
    let shell = Memory::over(DbusTransport::new(connect(&address).await));
    let refused = shell
        .forget(PlanToken::parse("p-1").expect("token"))
        .await
        .expect_err("no such plan");
    assert!(matches!(refused, ClientError::Refused(Refusal::Invalid(_))));
}

#[tokio::test]
async fn a_process_the_table_does_not_name_is_nobody() {
    let (_scratch, _bus, _daemon, address) = world(CallerTable::default()).await;
    let stranger = Memory::over(DbusTransport::new(connect(&address).await));
    assert_eq!(
        stranger.search(search()).await.expect_err("nobody"),
        ClientError::Refused(Refusal::NotAllowed)
    );
    let elsewhere = table_with("router", Path::new("/usr/libexec/quire/intentd"));
    let (_scratch2, _bus2, _daemon2, address) = world(elsewhere).await;
    let stranger = Memory::over(DbusTransport::new(connect(&address).await));
    assert_eq!(
        stranger
            .search(search())
            .await
            .expect_err("another program"),
        ClientError::Refused(Refusal::NotAllowed)
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
