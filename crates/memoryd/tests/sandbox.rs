//! The Landlock policy as a table, and the sandbox itself on a scratch tree: a thread applies the
//! policy (Landlock restricts the calling thread, and this thread ends with the test) and then
//! reaches for what it may and may not have. A kernel without Landlock skips the second half.

use almanac_core::Dirs;
use memoryd::{Enforcement, Policy, bus_socket, enforce, policy_for, prepare};
use std::io::{ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

fn dirs() -> Dirs {
    Dirs::new(
        "/d".into(),
        "/c".into(),
        "/cfg".into(),
        "/run/user/7".into(),
    )
}

#[test]
fn the_bus_socket_is_the_address_or_the_runtime_default() {
    let run = Path::new("/run/user/7");
    let cases = [
        (None, "/run/user/7/bus"),
        (Some("unix:path=/run/user/7/bus"), "/run/user/7/bus"),
        (
            Some("unix:path=/tmp/private/bus,guid=abc"),
            "/tmp/private/bus",
        ),
        (Some("unix:guid=abc,path=/x/bus"), "/x/bus"),
        (Some("unix:abstract=/tmp/dbus-x"), "/run/user/7/bus"),
        (Some("tcp:host=h,port=1;unix:path=/y/bus"), "/y/bus"),
        (Some(""), "/run/user/7/bus"),
    ];
    for (address, want) in cases {
        assert_eq!(bus_socket(address, run), PathBuf::from(want), "{address:?}");
    }
}

#[test]
fn the_policy_writes_the_memory_directories_and_nothing_else() {
    let policy = policy_for(&dirs(), None);
    let writable: Vec<&str> = policy.writable.iter().filter_map(|p| p.to_str()).collect();
    assert_eq!(
        writable,
        [
            "/d/quire/memory",
            "/c/quire/memory",
            "/cfg/quire",
            "/run/user/7/quire/memory"
        ]
    );
    assert_eq!(policy.sockets, vec![PathBuf::from("/run/user/7/bus")]);
    for tree in &policy.readable {
        assert!(
            !policy
                .writable
                .iter()
                .any(|w| w.starts_with(tree) || tree.starts_with(w)),
            "{tree:?} is read only and apart from the memory directories"
        );
    }
    assert!(policy.readable.contains(&PathBuf::from("/usr")));
    // `/proc` is files only (the caller's cgroup), never a readable tree.
    assert_eq!(policy.read_files, vec![PathBuf::from("/proc")]);
    assert!(!policy.readable.contains(&PathBuf::from("/proc")));
}

#[test]
fn prepare_makes_the_directories_the_policy_names() {
    let scratch = tempfile::tempdir().expect("scratch");
    let policy = Policy {
        writable: vec![scratch.path().join("a/b"), scratch.path().join("c")],
        readable: vec![],
        read_files: vec![],
        sockets: vec![],
    };
    prepare(&policy).expect("prepare");
    assert!(scratch.path().join("a/b").is_dir() && scratch.path().join("c").is_dir());
    prepare(&policy).expect("and again");
}

/// What the restricted thread found.
#[derive(Debug)]
struct Found {
    enforcement: Enforcement,
    write_inside: std::io::Result<()>,
    read_inside: std::io::Result<Vec<u8>>,
    write_outside: std::io::Result<()>,
    read_outside: std::io::Result<Vec<u8>>,
    read_system: std::io::Result<usize>,
    proc_cgroup: std::io::Result<String>,
    proc_exe: std::io::Result<PathBuf>,
    proc_listing: std::io::Result<usize>,
    bus: std::io::Result<UnixStream>,
    other_socket: std::io::Result<UnixStream>,
    tcp: std::io::Result<TcpStream>,
}

#[test]
fn the_sandbox_confines_the_thread_that_applies_it() {
    let scratch = tempfile::tempdir().expect("scratch");
    let root = scratch.path().to_owned();
    let inside = root.join("memory");
    let outside = root.join("elsewhere");
    for dir in [&inside, &outside] {
        std::fs::create_dir_all(dir).expect("dir");
    }
    std::fs::write(outside.join("secret"), b"not yours").expect("secret");
    std::fs::write(inside.join("mine"), b"yours").expect("mine");
    // Sockets and a TCP listener made before the restriction, to be reached after it.
    let bus_path = root.join("bus");
    let _bus = UnixListener::bind(&bus_path).expect("bus");
    let other_path = outside.join("other.sock");
    let _other = UnixListener::bind(&other_path).expect("other");
    let tcp = TcpListener::bind("127.0.0.1:0").expect("tcp");
    let tcp_addr = tcp.local_addr().expect("addr");

    let policy = Policy {
        writable: vec![inside.clone()],
        readable: ["/usr", "/lib", "/lib64", "/etc"]
            .iter()
            .map(PathBuf::from)
            .collect(),
        read_files: vec![PathBuf::from("/proc")],
        sockets: vec![bus_path.clone()],
    };
    let found = std::thread::spawn(move || {
        let enforcement = enforce(&policy).expect("the ruleset is accepted");
        Found {
            enforcement,
            write_inside: std::fs::File::create(inside.join("new"))
                .and_then(|mut f| f.write_all(b"ok")),
            read_inside: std::fs::read(inside.join("mine")),
            write_outside: std::fs::write(outside.join("planted"), b"x"),
            read_outside: std::fs::read(outside.join("secret")),
            read_system: std::fs::read_dir("/usr").map(|d| d.count()),
            proc_cgroup: std::fs::read_to_string("/proc/1/cgroup"),
            proc_exe: std::fs::read_link("/proc/1/exe"),
            proc_listing: std::fs::read_dir("/proc").map(|d| d.count()),
            bus: UnixStream::connect(&bus_path),
            other_socket: UnixStream::connect(&other_path),
            tcp: TcpStream::connect(tcp_addr),
        }
    })
    .join()
    .expect("thread");

    eprintln!(
        "landlock enforcement on this kernel: {:?}",
        found.enforcement
    );
    if found.enforcement == Enforcement::Unsupported {
        eprintln!("skipped: this kernel has no Landlock");
        assert!(found.read_outside.is_ok(), "and nothing was restricted");
        return;
    }
    assert!(found.write_inside.is_ok(), "{found:?}");
    assert_eq!(found.read_inside.as_deref().ok(), Some(&b"yours"[..]));
    assert_eq!(
        found.write_outside.as_ref().err().map(std::io::Error::kind),
        Some(ErrorKind::PermissionDenied),
        "{found:?}"
    );
    assert_eq!(
        found.read_outside.as_ref().err().map(std::io::Error::kind),
        Some(ErrorKind::PermissionDenied),
        "{found:?}"
    );
    assert!(
        found.read_system.as_ref().is_ok_and(|n| *n > 0),
        "{found:?}"
    );
    // Who is calling is read from `/proc/<pid>/cgroup`: files only, no listing, and the
    // ptrace-gated `exe` of a process outside this domain stays closed.
    assert!(found.proc_cgroup.is_ok(), "{found:?}");
    assert_eq!(
        found.proc_listing.as_ref().err().map(std::io::Error::kind),
        Some(ErrorKind::PermissionDenied),
        "{found:?}"
    );
    assert!(found.proc_exe.is_err(), "{found:?}");
    // The rights that need a newer kernel are asserted when this kernel is full strength.
    if found.enforcement == Enforcement::Full {
        assert!(
            found.bus.is_ok(),
            "the session bus stays reachable: {found:?}"
        );
        assert_eq!(
            found.other_socket.as_ref().err().map(std::io::Error::kind),
            Some(ErrorKind::PermissionDenied),
            "{found:?}"
        );
        assert_eq!(
            found.tcp.as_ref().err().map(std::io::Error::kind),
            Some(ErrorKind::PermissionDenied),
            "{found:?}"
        );
    }
    // This thread was never restricted.
    assert!(std::fs::read(root.join("elsewhere/secret")).is_ok());
}
