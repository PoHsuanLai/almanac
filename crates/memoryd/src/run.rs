//! The daemon's start-up: environment to sandbox to bus. The binary is `main.rs` (arguments and a
//! call to [`start`]); the environment is injected as a lookup closure, so a test can start the
//! daemon over a private bus and a scratch home.

use crate::keysel::{AnyKeys, KEYS_VAR, SANDBOX_VAR, Sandbox, TestKeys, sandbox_choice, select};
use crate::{
    CallerFileError, Daemon, Enforcement, InferdConsolidator, InferdEmbedder, KeysError,
    LockChanges, ProcGate, ProcPeers, SHELL_APP, SandboxError, SettingsWatch, SystemBackend,
    WatchState, XdgError, apply, apply_next, default_card, dirs_from, enforce, inferd_link,
    load_callers, policy_for, prepare,
};
use almanac_core::{Dirs, RuleSet};
use almanac_dbus::serve_on;
use almanac_service::{
    ConfigError, ConsolidateWhen, Locator, MemoryService, MemorySettings, rules_from_toml,
    spaces_from_toml,
};
use porter_client::DbusTransport;
use porter_daemon::ProcRoot;
use porter_dbus::CallerTable;
use std::ffi::OsString;
use std::path::Path;
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

fn user_callers(dirs: &Dirs) -> std::path::PathBuf {
    dirs.memory_toml().with_file_name("memory-callers.toml")
}

fn read_rules(dirs: &Dirs) -> RuleSet {
    match std::fs::read_to_string(dirs.memory_toml()) {
        Ok(text) => rules_from_toml(&text).unwrap_or_else(|e| {
            eprintln!("memoryd: memory.toml is unreadable ({e}); using the standard rules");
            RuleSet::standard()
        }),
        Err(_) => RuleSet::standard(),
    }
}

fn read_callers(dirs: &Dirs) -> Result<CallerTable, CallerFileError> {
    load_callers(Path::new(SYSTEM_CALLERS), &user_callers(dirs))
}

async fn run(
    env: Env<'_>,
    dirs: Dirs,
    keys: AnyKeys,
    proc_root: ProcRoot,
) -> Result<(), StartError> {
    // The bus is the injected environment's too: a test starts the daemon on a private bus.
    let builder = match env("DBUS_SESSION_BUS_ADDRESS") {
        Some(address) => zbus::connection::Builder::address(address.as_str())?,
        None => zbus::connection::Builder::session()?,
    };
    let connection = builder.build().await?;
    let inferd = inferd_link(&connection);
    let backend = SystemBackend::with(
        dirs.clone(),
        keys,
        InferdEmbedder::new(inferd.clone(), default_card()),
        InferdConsolidator::new(inferd),
    );
    let service = MemoryService::new(backend, read_rules(&dirs));
    // The person's settings (`almanac/settings.toml`) now, and whenever the file changes.
    let mut watched = SettingsWatch::start(
        Locator::from_env(&|key| env(key)),
        MemorySettings::default(),
    );
    if let WatchState::Blind { reason } = watched.state() {
        eprintln!("memoryd: settings are read once, not watched: {reason}");
    }
    apply(&service, &watched.current());
    if let Ok(text) = std::fs::read_to_string(dirs.spaces_toml()) {
        let file = spaces_from_toml(&text).map_err(StartError::Spaces)?;
        file.spaces
            .into_iter()
            .for_each(|meta| service.register(meta));
    }
    let peers = ProcPeers::with_proc_root(
        connection.clone(),
        read_callers(&dirs)?,
        proc_root.path().to_path_buf(),
    );
    if let Ok(shell) = almanac_core::AppName::parse(SHELL_APP) {
        service.set_fallback_owner(shell);
    }
    let spaces = DbusTransport::over(connection.clone()).spaces().await;
    let daemon = Arc::new(Daemon::new(service, peers, dirs));
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
    // Only a test build's file key store has a directory of its own to write.
    policy.writable.extend(key_dir);
    prepare(&policy).map_err(StartError::Prepare)?;
    let sandbox = sandbox_choice(env(SANDBOX_VAR).as_deref(), TestKeys::THIS_BUILD);
    let enforcement = match sandbox {
        Sandbox::On => enforce(&policy)?,
        Sandbox::Off => {
            eprintln!("memoryd: TEST BUILD: the Landlock sandbox is off");
            Enforcement::Full
        }
    };
    match enforcement {
        Enforcement::Full => {}
        Enforcement::Partial => eprintln!(
            "memoryd: this kernel's Landlock is older than the policy; part of it is enforced"
        ),
        Enforcement::Unsupported => eprintln!(
            "memoryd: this kernel has no Landlock; the unit file's restrictions are all there is"
        ),
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(StartError::Runtime)?
        .block_on(run(env, dirs, keys, proc_root))
}
