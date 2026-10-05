//! memoryd's Landlock sandbox: after it starts, the daemon may write only the memory directories
//! (data, cache, its two config files, the runtime edit copies), read the system's libraries and
//! configuration, the files under `/proc` (the caller's cgroup), talk to the session bus socket and nothing else: no other file, no other unix
//! socket, no TCP. It is the kernel-enforced second layer behind the unit file's
//! `ProtectSystem`/`ReadWritePaths` (the daemon can run outside systemd), and it is applied by
//! the `landlock` crate: nothing here is `unsafe`.
//!
//! Landlock restricts the calling thread and the threads it creates afterwards, so [`enforce`]
//! runs in `main` before the async runtime builds its workers. The policy is a pure value
//! ([`policy_for`]) so it is a table test; [`enforce`] is the only effect.

use almanac_core::Dirs;
use landlock::{
    ABI, Access, AccessFs, AccessNet, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetError,
    RulesetStatus, path_beneath_rules,
};
use std::path::{Path, PathBuf};

/// The newest Landlock ABI the crate knows; the kernel gives what it has (best effort).
const ABI_TOP: ABI = ABI::V9;

/// What the daemon may touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Directories read and written (the memory directories). They must exist when it is applied.
    pub writable: Vec<PathBuf>,
    /// Read-only trees (libraries, the system's configuration). One that does not exist on this
    /// system is skipped.
    pub readable: Vec<PathBuf>,
    /// Trees whose files may be read and nothing else: no listing, no execution. `/proc`, for
    /// `/proc/<pid>/cgroup` (who is calling); Landlock names trees, not file patterns, and the
    /// kernel's own ptrace check still guards `exe` and `root` of other domains' processes.
    pub read_files: Vec<PathBuf>,
    /// Unix sockets it may connect to: the session bus.
    pub sockets: Vec<PathBuf>,
}

/// How far the kernel enforced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    /// Every restriction asked for is in force.
    Full,
    /// This kernel's Landlock is older than the policy: what it can restrict is restricted.
    Partial,
    /// This kernel has no Landlock (or it is not enabled): nothing is restricted.
    Unsupported,
}

/// Why the sandbox could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    /// The ruleset was refused (a bug in the policy or an unexpected kernel answer).
    #[error("landlock: {0}")]
    Ruleset(String),
}

impl From<RulesetError> for SandboxError {
    fn from(e: RulesetError) -> Self {
        SandboxError::Ruleset(e.to_string())
    }
}

/// The system trees a process reads to run at all: the dynamic loader's libraries, certificates
/// and the like, the kernel's views of itself.
const SYSTEM_READ: [&str; 7] = [
    "/usr",
    "/lib",
    "/lib64",
    "/bin",
    "/etc",
    "/sys",
    "/dev/urandom",
];

/// The socket the session bus is on: `unix:path=...` in `address` (a `DBUS_SESSION_BUS_ADDRESS`),
/// else `<runtime>/bus`, which is where the bus listens by default.
pub fn bus_socket(address: Option<&str>, runtime_dir: &Path) -> PathBuf {
    address
        .into_iter()
        .flat_map(|a| a.split(';'))
        .find_map(|one| {
            let rest = one.strip_prefix("unix:")?;
            rest.split(',')
                .find_map(|kv| kv.strip_prefix("path="))
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| runtime_dir.join("bus"))
}

/// The policy for a daemon over `dirs` talking to the bus at `bus_address`.
pub fn policy_for(dirs: &Dirs, bus_address: Option<&str>) -> Policy {
    Policy {
        writable: vec![
            dirs.memory(),
            dirs.cache_memory(),
            dirs.config(),
            dirs.runtime_memory(),
        ],
        readable: SYSTEM_READ.iter().map(PathBuf::from).collect(),
        read_files: vec![PathBuf::from("/proc")],
        sockets: vec![bus_socket(bus_address, dirs.runtime_dir())],
    }
}

/// Makes the directories the daemon writes, so the policy can name them (Landlock rules name
/// existing paths) and a first run needs nothing from outside the sandbox.
pub fn prepare(policy: &Policy) -> std::io::Result<()> {
    policy.writable.iter().try_for_each(std::fs::create_dir_all)
}

/// Applies `policy` to the calling thread and the threads it creates later.
pub fn enforce(policy: &Policy) -> Result<Enforcement, SandboxError> {
    let all_files = AccessFs::from_all(ABI_TOP);
    let status = Ruleset::default()
        .handle_access(all_files)?
        // No TCP at all: nothing here listens or connects.
        .handle_access(AccessNet::from_all(ABI_TOP))?
        .create()?
        .add_rules(path_beneath_rules(&policy.writable, all_files))?
        .add_rules(path_beneath_rules(
            &policy.readable,
            AccessFs::from_read(ABI_TOP),
        ))?
        .add_rules(path_beneath_rules(&policy.read_files, AccessFs::ReadFile))?
        .add_rules(path_beneath_rules(&policy.sockets, AccessFs::ResolveUnix))?
        .restrict_self()?;
    Ok(match status.ruleset {
        RulesetStatus::FullyEnforced => Enforcement::Full,
        RulesetStatus::PartiallyEnforced => Enforcement::Partial,
        RulesetStatus::NotEnforced => Enforcement::Unsupported,
    })
}
