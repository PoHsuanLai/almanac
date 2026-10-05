//! memoryd: serves `org.quire.Memory1` on the session bus over almanac-service. Keys are in the
//! Secret Service, the logs and the index in SQLCipher, the files sealed per Space; who is
//! calling comes from `callers.toml`; inferd is reached over the same session bus
//! (`AnyTransport::Dbus`), and while it is not running recall is lexical-only and consolidation is
//! off, as the embedder and the consolidator answer `Unavailable`. A Landlock sandbox (`sandbox.rs`)
//! is applied before anything else runs.

use almanac_core::{Dirs, RuleSet};
use almanac_dbus::serve_on;
use almanac_seal::Oo7Keys;
use almanac_service::{MemoryService, rules_from_toml, spaces_from_toml};
use clap::Parser;
use memoryd::{
    CallerTable, Daemon, Enforcement, InferdConsolidator, InferdEmbedder, LockChanges, ProcPeers,
    SystemBackend, default_card, dirs_from_env, enforce, inferd_link, policy_for, prepare,
};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

/// almanac's memory daemon (`org.quire.Memory1`).
#[derive(Debug, Parser)]
#[command(name = "memoryd", version)]
struct Args {}

/// The retention sweep runs this often (and once a minute after the daemon starts).
const SWEEP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_SWEEP: Duration = Duration::from_secs(60);

fn callers_toml(dirs: &Dirs) -> std::path::PathBuf {
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

fn read_callers(dirs: &Dirs) -> CallerTable {
    let path = callers_toml(dirs);
    match std::fs::read_to_string(&path) {
        Ok(text) => CallerTable::from_toml(&text).unwrap_or_else(|e| {
            eprintln!(
                "memoryd: {} is unreadable ({e}); no caller is allowed",
                path.display()
            );
            CallerTable::default()
        }),
        Err(_) => {
            eprintln!("memoryd: no {}; no caller is allowed", path.display());
            CallerTable::default()
        }
    }
}

async fn run(dirs: Dirs) -> Result<(), String> {
    let connection = zbus::connection::Builder::session()
        .map_err(|e| e.to_string())?
        .build()
        .await
        .map_err(|e| e.to_string())?;
    let inferd = inferd_link(&connection);
    let backend = SystemBackend::with(
        dirs.clone(),
        Oo7Keys,
        InferdEmbedder::new(inferd.clone(), default_card()),
        InferdConsolidator::new(inferd),
    );
    let service = MemoryService::new(backend, read_rules(&dirs));
    if let Ok(text) = std::fs::read_to_string(dirs.spaces_toml()) {
        let file = spaces_from_toml(&text).map_err(|e| format!("spaces.toml: {e}"))?;
        file.spaces
            .into_iter()
            .for_each(|meta| service.register(meta));
    }
    let peers = ProcPeers::new(connection.clone(), read_callers(&dirs));
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
    let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    let policy = policy_for(&dirs, bus.as_deref());
    prepare(&policy).map_err(|e| format!("cannot make the memory directories: {e}"))?;
    match enforce(&policy).map_err(|e| e.to_string())? {
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
        .block_on(run(dirs))
}

fn main() -> ExitCode {
    let Args {} = Args::parse();
    // The daemon's one log path is standard error, prefixed with its name.
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("memoryd: {why}");
            ExitCode::from(1)
        }
    }
}
