//! The daemon's start-up: environment to sandbox to bus. The binary is `main.rs` (arguments and a
//! call to [`start`]); the environment is injected as a lookup closure, so a test can start the
//! daemon over a private bus and a scratch home.

use crate::keysel::{AnyKeys, KEYS_VAR, SANDBOX_VAR, Sandbox, TestKeys, sandbox_choice, select};
use crate::{
    CallerFileError, Consent, Daemon, Enforcement, InferdConsolidator, InferdEmbedder, KeysError,
    LockChanges, ProcGate, ProcPeers, SHELL_APP, SandboxError, SettingsWatch, SpaceRegistry,
    SystemBackend, WatchState, XdgError, apply, apply_next, confinement_of, default_card,
    dirs_from, enforce, inferd_link, load_callers, policy_for, prepare,
};
use almanac_core::{Dirs, RuleSet};
use almanac_dbus::{Confinement, serve_on};
use almanac_service::{
    ConfigError, ConsolidateWhen, Locator, MemoryService, MemorySettings, read_config,
    rules_from_toml, spaces_from_toml,
};
use porter_client::DbusTransport;
use porter_daemon::ProcRoot;
use porter_dbus::CallerTable;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The environment, as a lookup by variable name.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The environment variable the test-only fixture `/proc` switch reads.
pub const PROC_ROOT_VAR: &str = "MEMORYD_PROC_ROOT";

/// Whether this build honours [`PROC_ROOT_VAR`]: only one built with the `test-proc-root` feature.
/// A release build ignores the variable and says so.
pub const PROC_GATE: ProcGate = if cfg!(feature = "test-proc-root") {
    ProcGate::Honour
} else {
    ProcGate::Ignore
};

/// Why the daemon did not start, or stopped.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The XDG directories could not be found.
    #[error(transparent)]
    Dirs(#[from] XdgError),
    /// The key store could not be chosen.
    #[error(transparent)]
    Keys(#[from] KeysError),
    /// The memory directories could not be made.
    #[error("cannot make the memory directories: {0}")]
    Prepare(std::io::Error),
    /// The Landlock sandbox could not be applied.
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    /// The async runtime could not be built.
    #[error("cannot build the runtime: {0}")]
    Runtime(std::io::Error),
    /// `spaces.toml` does not parse.
    #[error("spaces.toml: {0}")]
    Spaces(ConfigError),
    /// `memory.toml` or `spaces.toml` exists but cannot be read.
    #[error("{file}: {reason}")]
    Unreadable {
        /// Which file.
        file: &'static str,
        /// Why.
        reason: String,
    },
    /// `memory.toml` does not parse: starting without the person's rules would forget them.
    #[error("memory.toml: {0}")]
    Rules(ConfigError),
    /// The callers files do not read.
    #[error(transparent)]
    Callers(#[from] CallerFileError),
    /// The session bus refused something.
    #[error("session bus: {0}")]
    Bus(#[from] zbus::Error),
}

/// The retention sweep runs this often (and once a minute after the daemon starts).
const SWEEP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_SWEEP: Duration = Duration::from_secs(60);

/// Nightly consolidation (`memory.consolidation.when = "nightly"`) runs this often, the first time
/// an hour after the daemon starts. The desktop's idle and power state are not asked yet
/// (FINDINGS), so it is a daily timer, like the sweep.
const CONSOLIDATE_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_CONSOLIDATION: Duration = Duration::from_secs(60 * 60);

/// The system's callers file; the user's is next to `memory.toml` and wins.
const SYSTEM_CALLERS: &str = "/etc/quire/memory-callers.toml";

/// The test-only stand-in for [`SYSTEM_CALLERS`], honoured under the same gate as
/// [`PROC_ROOT_VAR`]: a harness that fakes `/proc` must also say which units are the system's,
/// because only the system's table gives a fixed role. A release build ignores it.
pub const SYSTEM_CALLERS_VAR: &str = "MEMORYD_SYSTEM_CALLERS";

/// The system callers file this build reads: [`SYSTEM_CALLERS`], or the variable's file in a
/// `test-proc-root` build.
fn system_callers(gate: ProcGate, env: Env<'_>) -> PathBuf {
    match (gate, env(SYSTEM_CALLERS_VAR)) {
        (ProcGate::Honour, Some(file)) if !file.is_empty() => PathBuf::from(file),
        _ => PathBuf::from(SYSTEM_CALLERS),
    }
}

fn user_callers(dirs: &Dirs) -> std::path::PathBuf {
    dirs.memory_toml().with_file_name("memory-callers.toml")
}

/// The person's rules. No file is a fresh install (the standard rules); a file that cannot be
/// read or parsed stops the start, so a damaged file never loosens the do-not-remember rules.
fn read_rules(dirs: &Dirs) -> Result<RuleSet, StartError> {
    let text = read_config(&dirs.memory_toml()).map_err(|e| StartError::Unreadable {
        file: "memory.toml",
        reason: e.to_string(),
    })?;
    match text {
        None => Ok(RuleSet::standard()),
        Some(text) => rules_from_toml(&text).map_err(|e| {
            eprintln!("memoryd: memory.toml is unreadable ({e}); not starting with other rules");
            StartError::Rules(e)
        }),
    }
}

fn read_callers(system: &Path, dirs: &Dirs) -> Result<CallerTable, CallerFileError> {
    load_callers(system, &user_callers(dirs))
}

async fn run(
    env: Env<'_>,
    dirs: Dirs,
    keys: AnyKeys,
    proc_root: ProcRoot,
    system: PathBuf,
    confinement: Confinement,
) -> Result<(), StartError> {
    // The bus is the injected environment's too: a test starts the daemon on a private bus.
    let builder = match env("DBUS_SESSION_BUS_ADDRESS") {
        Some(address) => zbus::connection::Builder::address(address.as_str())?,
        None => zbus::connection::Builder::session()?,
    };
    let connection = builder.build().await?;
    let inferd = inferd_link(&connection);
    let spaces = DbusTransport::over(connection.clone()).spaces().await;
    let registry = match &spaces {
        Ok(spaces) => SpaceRegistry::Porter(spaces.clone()),
        Err(_) => SpaceRegistry::Closed,
    };
    let backend = SystemBackend::with(
        dirs.clone(),
        keys,
        InferdEmbedder::new(inferd.clone(), default_card()),
        InferdConsolidator::new(inferd),
    )
    .with_registry(registry);
    let service = MemoryService::new(backend, read_rules(&dirs)?);
    // The person's settings (`almanac/settings.toml`) now, and whenever the file changes.
    // A value looser than the shipped default waits for the person's confirmation, kept in our
    // own data directory (ALM-16).
    let locator = Locator::from_env(&|key| env(key)).confirming(dirs.confirmed_settings_toml());
    let mut watched = SettingsWatch::start(locator.clone(), MemorySettings::default());
    if let WatchState::Blind { reason } = watched.state() {
        eprintln!("memoryd: settings are read once, not watched: {reason}");
    }
    apply(&service, &watched.current());
    let spaces_text = read_config(&dirs.spaces_toml()).map_err(|e| StartError::Unreadable {
        file: "spaces.toml",
        reason: e.to_string(),
    })?;
    if let Some(text) = spaces_text {
        let file = spaces_from_toml(&text).map_err(StartError::Spaces)?;
        file.spaces
            .into_iter()
            .for_each(|meta| service.register(meta));
    }
    // porter reads a fixture tree only in a test build; everywhere else it asks /proc and the
    // user manager, which a fixture tree has no stand-in for.
    let table = read_callers(&system, &dirs)?;
    let peers = match proc_root.fixture() {
        #[cfg(feature = "test-proc-root")]
        Some(dir) => ProcPeers::with_proc_root(connection.clone(), table, dir.to_path_buf()),
        _ => ProcPeers::new(connection.clone(), table),
    };
    if let Ok(shell) = almanac_core::AppName::parse(SHELL_APP) {
        service.set_fallback_owner(shell);
    }
    let daemon = Arc::new(
        Daemon::new(service, peers, dirs)
            .with_consent(Consent::new(locator))
            .with_confinement(confinement),
    );
    serve_on(&connection, daemon.clone()).await?;
    // The keyring says when it locks or unlocks; no timer asks.
    let changes = LockChanges::on(&connection).await?;
    daemon.attach(connection);
    let watcher = daemon.clone();
    tokio::spawn(async move { watcher.follow_keyring(changes).await });
    let following = daemon.clone();
    tokio::spawn(async move {
        while apply_next(following.queue().service(), &mut watched)
            .await
            .is_some()
        {}
    });
    if let Ok(spaces) = spaces {
        let removed = daemon.clone();
        tokio::spawn(async move { removed.follow_spaces(spaces).await });
    }
    let consolidator = daemon.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_CONSOLIDATION).await;
        loop {
            // The setting is read afresh each night, so turning it off takes effect tonight.
            if consolidator.queue().service().settings().consolidate == ConsolidateWhen::Nightly {
                for (space, reply) in consolidator.consolidate_all().await {
                    if let almanac_core::MemoryReply::Refused(why) = reply {
                        eprintln!("memoryd: consolidation of {space}: {why:?}");
                    }
                }
            }
            tokio::time::sleep(CONSOLIDATE_EVERY).await;
        }
    });
    let sweeper = daemon.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_SWEEP).await;
        loop {
            for (space, swept) in sweeper.sweep_all().await {
                if let Err(why) = swept {
                    eprintln!("memoryd: sweep of {space}: {why:?}");
                }
            }
            tokio::time::sleep(SWEEP_EVERY).await;
        }
    });
    std::future::pending::<()>().await;
    Ok(())
}

/// Starts the daemon over the environment `env` reads (the process environment in the binary, a
/// map in a test): the sandbox first (Landlock restricts the calling thread and the threads made after it, so it
/// comes before the runtime builds its workers), then the daemon.
pub fn start(env: Env<'_>) -> Result<(), StartError> {
    let dirs = dirs_from(env)?;
    let selection = select(env(KEYS_VAR).as_deref(), TestKeys::THIS_BUILD);
    let key_dir = selection.writable_dir();
    let (keys, said) = AnyKeys::from_selection(selection)?;
    if let Some(line) = said {
        eprintln!("memoryd: {line}");
    }
    let proc_root = ProcRoot::choose(PROC_GATE, PROC_ROOT_VAR, |name| {
        env(name).map(OsString::from)
    });
    if let Some(line) = proc_root.notice("memoryd", PROC_ROOT_VAR) {
        eprintln!("{line}");
    }
    let bus = env("DBUS_SESSION_BUS_ADDRESS");
    let mut policy = policy_for(&dirs, bus.as_deref());
    // A fixture /proc is read like /proc is: files only.
    if let Some(dir) = proc_root.fixture() {
        policy.read_files.push(dir.to_path_buf());
    }
    let system = system_callers(PROC_GATE, env);
    if system != Path::new(SYSTEM_CALLERS) {
        eprintln!(
            "memoryd: TEST BUILD: the system callers file is {}",
            system.display()
        );
        policy.read_files.push(system.clone());
    }
    // Only a test build's file key store has a directory of its own to write.
    policy.writable.extend(key_dir);
    prepare(&policy).map_err(StartError::Prepare)?;
    let sandbox = sandbox_choice(env(SANDBOX_VAR).as_deref(), TestKeys::THIS_BUILD);
    let enforcement = match sandbox {
        Sandbox::On => Some(enforce(&policy)?),
        Sandbox::Off => {
            eprintln!("memoryd: TEST BUILD: the Landlock sandbox is off");
            None
        }
    };
    match enforcement {
        Some(Enforcement::Full) | None => {}
        Some(Enforcement::Partial) => eprintln!(
            "memoryd: this kernel's Landlock is older than the policy; part of it is enforced"
        ),
        Some(Enforcement::Unsupported) => eprintln!(
            "memoryd: this kernel has no Landlock; the unit file's restrictions are all there is"
        ),
    }
    let confinement = confinement_of(enforcement);
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(StartError::Runtime)?
        .block_on(run(env, dirs, keys, proc_root, system, confinement))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a `test-proc-root` build reads the variable, and an empty value is no file.
    #[test]
    fn the_system_callers_file_moves_only_in_a_test_build() {
        let set =
            |name: &str| (name == SYSTEM_CALLERS_VAR).then(|| "/scratch/system.toml".to_owned());
        let empty = |name: &str| (name == SYSTEM_CALLERS_VAR).then(String::new);
        let unset = |_: &str| None;
        let rows: [(ProcGate, Env<'_>, &str); 4] = [
            (ProcGate::Honour, &set, "/scratch/system.toml"),
            (ProcGate::Ignore, &set, SYSTEM_CALLERS),
            (ProcGate::Honour, &empty, SYSTEM_CALLERS),
            (ProcGate::Honour, &unset, SYSTEM_CALLERS),
        ];
        for (gate, env, want) in rows {
            assert_eq!(system_callers(gate, env), PathBuf::from(want), "{gate:?}");
        }
    }
}
