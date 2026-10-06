//! memoryd: serves `org.quire.Memory1` on the session bus over almanac-service. Keys are in the
//! Secret Service, the logs and the index in SQLCipher, the files sealed per Space; who is
//! calling comes from the pid's cgroup and `memory-callers.toml`; inferd is reached over the same session bus
//! (`AnyTransport::Dbus`), and while it is not running recall is lexical-only and consolidation is
//! off, as the embedder and the consolidator answer `Unavailable`. A Landlock sandbox (`sandbox.rs`)
//! is applied before anything else runs.

use almanac_core::{Dirs, RuleSet};
use almanac_dbus::serve_on;
use almanac_service::{
    ConsolidateWhen, Locator, MemoryService, MemorySettings, rules_from_toml, spaces_from_toml,
};
use clap::Parser;
use memoryd::{
    AnyKeys, Daemon, Enforcement, InferdConsolidator, InferdEmbedder, LockChanges, ProcPeers,
    ProcRoot, Sandbox, SettingsWatch, SystemBackend, TestKeys, TestProcRoot, WatchState, apply,
    apply_next, default_card, dirs_from_env, enforce, inferd_link, load_callers, policy_for,
    prepare, proc_root_choice, sandbox_choice, select,
};
use porter_dbus::CallerTable;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

/// almanac's memory daemon (`org.quire.Memory1`).
#[derive(Debug, Parser)]
#[command(name = "memoryd", version)]
struct Args {
    /// Write the settings schema (`almanac.settings.toml`) into this directory and stop.
    #[arg(long, value_name = "DIR")]
    write_schema: Option<std::path::PathBuf>,
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

fn read_callers(dirs: &Dirs) -> Result<CallerTable, String> {
    load_callers(Path::new(SYSTEM_CALLERS), &user_callers(dirs)).map_err(|e| e.to_string())
}

async fn run(dirs: Dirs, keys: AnyKeys, proc_root: ProcRoot) -> Result<(), String> {
    let connection = zbus::connection::Builder::session()
        .map_err(|e| e.to_string())?
        .build()
        .await
        .map_err(|e| e.to_string())?;
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
        Locator::from_env(&|key| std::env::var(key).ok()),
        MemorySettings::default(),
    );
    if let WatchState::Blind { reason } = watched.state() {
        eprintln!("memoryd: settings are read once, not watched: {reason}");
    }
    apply(&service, &watched.current());
    if let Ok(text) = std::fs::read_to_string(dirs.spaces_toml()) {
        let file = spaces_from_toml(&text).map_err(|e| format!("spaces.toml: {e}"))?;
        file.spaces
            .into_iter()
            .for_each(|meta| service.register(meta));
    }
    let peers =
        ProcPeers::with_proc_root(connection.clone(), read_callers(&dirs)?, proc_root.path());
    let daemon = Arc::new(Daemon::new(service, peers, dirs));
    serve_on(&connection, daemon.clone())
        .await
        .map_err(|e| e.to_string())?;
    // The keyring says when it locks or unlocks; no timer asks.
    let changes = LockChanges::on(&connection)
        .await
        .map_err(|e| e.to_string())?;
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

/// The sandbox first (Landlock restricts the calling thread and the threads made after it, so it
/// comes before the runtime builds its workers), then the daemon.
fn start() -> Result<(), String> {
    let dirs = dirs_from_env().map_err(|e| e.to_string())?;
    let selection = select(
        std::env::var(memoryd::KEYS_VAR).ok().as_deref(),
        TestKeys::THIS_BUILD,
    );
    let key_dir = selection.writable_dir();
    let (keys, said) = AnyKeys::from_selection(selection)?;
    if let Some(line) = said {
        eprintln!("memoryd: {line}");
    }
    let proc_root = proc_root_choice(
        std::env::var(memoryd::PROC_ROOT_VAR).ok().as_deref(),
        TestProcRoot::THIS_BUILD,
    );
    if let Some(line) = proc_root.said() {
        eprintln!("memoryd: {line}");
    }
    let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    let mut policy = policy_for(&dirs, bus.as_deref());
    // A fixture /proc is read like /proc is: files only.
    if let ProcRoot::Fixture(dir) = &proc_root {
        policy.read_files.push(dir.clone());
    }
    // Only a test build's file key store has a directory of its own to write.
    policy.writable.extend(key_dir);
    prepare(&policy).map_err(|e| format!("cannot make the memory directories: {e}"))?;
    let sandbox = sandbox_choice(
        std::env::var(memoryd::SANDBOX_VAR).ok().as_deref(),
        TestKeys::THIS_BUILD,
    );
    let enforcement = match sandbox {
        Sandbox::On => enforce(&policy).map_err(|e| e.to_string())?,
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
        .map_err(|e| e.to_string())?
        .block_on(run(dirs, keys, proc_root))
}

fn main() -> ExitCode {
    let Args { write_schema } = Args::parse();
    if let Some(dir) = write_schema {
        let written = std::fs::create_dir_all(&dir).and_then(|()| {
            std::fs::write(dir.join("almanac.settings.toml"), almanac_service::SCHEMA)
        });
        return match written {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("memoryd: {}: {why}", dir.display());
                ExitCode::from(1)
            }
        };
    }
    // The daemon's one log path is standard error, prefixed with its name.
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("memoryd: {why}");
            ExitCode::from(1)
        }
    }
}
