//! The packaged `memoryd` binary, started the way the acceptance run starts it: on a private bus,
//! with `env_clear`, scratch HOME and XDG directories and no Secret Service.
//!
//! The Landlock sandbox is ON: callers are identified through a fake `/proc` (`MEMORYD_PROC_ROOT`, feature `test-proc-root`)
//! that puts this test process in `intentd.service`.
//!
//! With the `test-keys` feature `MEMORYD_KEYS=file:<path>` gives it file keys, it claims
//! `org.quire.Memory1` and takes a `Record` into `desktop` (a Space nobody registered). Without
//! the feature the same variable is ignored, and the daemon says so.

use crate::support::bus::PrivateBus;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStderr, Command, Stdio};

/// The daemon process; killed by pid when dropped.
struct Daemon {
    child: Child,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

/// Scratch HOME and XDG roots, a callers file with the router's unit row, and a fake `/proc` in
/// which this test process runs in `intentd.service` as its main process.
fn scratch_home(root: &Path) {
    let config = root.join("config").join("quire");
    std::fs::create_dir_all(&config).expect("config dir");
    std::fs::create_dir_all(root.join("run")).expect("runtime dir");
    std::fs::write(
        config.join("memory-callers.toml"),
        "[[caller]]\napp = \"org.quire.Intents\"\nunit = \"intentd.service\"\nrole = \"agent\"\n",
    )
    .expect("callers file");
    let me = root.join("proc").join(std::process::id().to_string());
    std::fs::create_dir_all(&me).expect("fake proc");
    std::fs::write(
        me.join("cgroup"),
        "0::/user.slice/user-1000.slice/user@1000.service/app.slice/intentd.service\n",
    )
    .expect("cgroup");
    // ...as that unit's main process, which is all a service row names.
    let units = root.join("proc").join("units");
    std::fs::create_dir_all(&units).expect("units dir");
    std::fs::write(
        units.join("intentd.service"),
        std::process::id().to_string(),
    )
    .expect("main pid");
}

fn spawn(root: &Path, bus: &PrivateBus, keys: &str) -> (Daemon, ChildStderr) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_memoryd"))
        .env_clear()
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_RUNTIME_DIR", root.join("run"))
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .env("MEMORYD_KEYS", keys)
        .env("MEMORYD_PROC_ROOT", root.join("proc"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn memoryd");
    let stderr = child.stderr.take().expect("stderr");
    (Daemon { child }, stderr)
}

fn first_line(stderr: ChildStderr) -> String {
    let mut line = String::new();
    BufReader::new(stderr)
        .read_line(&mut line)
        .expect("a startup line");
    line
}

#[cfg(all(feature = "test-keys", feature = "test-proc-root"))]
#[tokio::test]
async fn the_test_keys_binary_starts_with_file_keys_and_takes_a_desktop_record() {
    use crate::support::connect;
    use almanac_client::{DbusTransport, Memory, Recorded};
    use almanac_core::{Record, SpaceId};
    use almanac_dbus::MEMORY_BUS;
    use almanac_fake::mail_thread_archived;
    use zbus::export::futures_core::Stream;
    let scratch = tempfile::tempdir().expect("scratch");
    let root = scratch.path();
    scratch_home(root);
    let bus = PrivateBus::start(root);
    let client = connect(&bus.address).await;
    // Watch for the name before the daemon starts: no sleep, no polling.
    let dbus = zbus::fdo::DBusProxy::new(&client)
        .await
        .expect("dbus proxy");
    let mut owners = dbus.receive_name_owner_changed().await.expect("signal");
    let keys_file = root.join("keys").join("memoryd.keys");
    let (_daemon, stderr) = spawn(root, &bus, &format!("file:{}", keys_file.display()));
    loop {
        let item = std::future::poll_fn(|cx| std::pin::Pin::new(&mut owners).poll_next(cx))
            .await
            .expect("the bus signals");
        let args = item.args().expect("args");
        if args.name().as_str() == MEMORY_BUS && args.new_owner().is_some() {
            break;
        }
    }
    let memory = Memory::over(DbusTransport::new(client));
    let record = Record {
        space: SpaceId::desktop(),
        ..mail_thread_archived().expect("fixture")
    };
    let recorded = memory.record(record).await;
    let recorded = recorded.expect("record into desktop");
    let Recorded::Stored(event) = recorded else {
        panic!("{recorded:?}")
    };
    assert_eq!(event.space, SpaceId::desktop());
    assert!(keys_file.exists(), "the key went to the file");
    assert!(
        root.join("data/quire/memory/desktop/events.db").exists(),
        "desktop was provisioned"
    );
    assert!(first_line(stderr).contains("TEST BUILD"));
}

#[cfg(not(feature = "test-keys"))]
#[test]
fn the_default_binary_ignores_the_variable_and_says_so() {
    let scratch = tempfile::tempdir().expect("scratch");
    let root = scratch.path();
    scratch_home(root);
    let bus = PrivateBus::start(root);
    let keys_file = root.join("keys").join("memoryd.keys");
    let (_daemon, stderr) = spawn(root, &bus, &format!("file:{}", keys_file.display()));
    assert!(first_line(stderr).contains("ignoring it"));
    assert!(!keys_file.exists());
}
