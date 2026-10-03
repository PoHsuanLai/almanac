//! Who is calling: the bus connection's unique name to a `Caller`.
//!
//! The caller class is derived by the transport, never sent (memory.md section 3.9): the bus
//! says which process owns a connection, and that process's executable decides the class
//! through `callers.toml`. This is advisory for unsandboxed processes (porter R12): a process
//! that can run an allowed executable can be that caller. The router, the shell and cuad are
//! fixed executables; an app is whichever executable the table names for it.

use almanac_core::{AppId, AppName, Caller, Isolation, Refusal};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// Which executable is which caller: the contents of `callers.toml`.
///
/// ```toml
/// router = ["/usr/libexec/quire/intentd"]
/// shell  = ["/usr/bin/sill"]
/// cuad   = ["/usr/libexec/quire/cuad"]
/// [apps]
/// "org.quire.Mail" = ["/usr/bin/mailo"]
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct CallerTable {
    #[serde(default)]
    router: BTreeSet<PathBuf>,
    #[serde(default)]
    shell: BTreeSet<PathBuf>,
    #[serde(default)]
    cuad: BTreeSet<PathBuf>,
    #[serde(default)]
    apps: BTreeMap<AppName, BTreeSet<PathBuf>>,
}

impl CallerTable {
    /// The table in `callers.toml` text.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The caller whose executable is `exe`, if the table names it. The fixed roles are checked
    /// before the apps, so an app entry cannot claim the router's executable.
    pub fn resolve(&self, exe: &Path) -> Option<Caller> {
        let named = |set: &BTreeSet<PathBuf>| set.contains(exe);
        if named(&self.router) {
            return Some(Caller::Router);
        }
        if named(&self.shell) {
            return Some(Caller::ShellUi);
        }
        if named(&self.cuad) {
            return Some(Caller::Cuad);
        }
        self.apps
            .iter()
            .find(|(_, exes)| named(exes))
            .map(|(name, _)| {
                Caller::App(AppId {
                    name: name.clone(),
                    isolation: Isolation::Unsandboxed,
                })
            })
    }
}

/// Who a bus connection is.
pub trait Peers: Send + Sync + 'static {
    /// The caller behind the connection `sender` (its unique name), or why it is nobody.
    fn caller_of(&self, sender: &str) -> impl Future<Output = Result<Caller, Refusal>> + Send;
}

/// Peers by process: the bus names the connection's pid, `/proc/<pid>/exe` names the program,
/// the [`CallerTable`] names the caller.
#[derive(Debug)]
pub struct ProcPeers {
    connection: zbus::Connection,
    table: CallerTable,
}

impl ProcPeers {
    /// Resolves senders on `connection` through `table`.
    pub fn new(connection: zbus::Connection, table: CallerTable) -> Self {
        Self { connection, table }
    }
}

/// The program a process runs. A replaced binary reads back with `" (deleted)"` appended, which
/// is not the program that was allowed.
fn exe_of(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

impl Peers for ProcPeers {
    async fn caller_of(&self, sender: &str) -> Result<Caller, Refusal> {
        let name = zbus::names::BusName::try_from(sender).map_err(|_| Refusal::NotAllowed)?;
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| Refusal::NotAllowed)?;
        let pid = bus
            .get_connection_unix_process_id(name)
            .await
            .map_err(|_| Refusal::NotAllowed)?;
        exe_of(pid)
            .and_then(|exe| self.table.resolve(&exe))
            .ok_or(Refusal::NotAllowed)
    }
}

/// Peers from a map of unique names, for tests and for hosts that know their clients.
#[derive(Debug, Default)]
pub struct TablePeers(Mutex<BTreeMap<String, Caller>>);

impl TablePeers {
    /// Nobody is known.
    pub fn new() -> Self {
        Self::default()
    }

    /// Says that the connection `unique_name` is `caller`.
    pub fn introduce(&self, unique_name: &str, caller: Caller) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(unique_name.to_owned(), caller);
    }
}

impl Peers for TablePeers {
    async fn caller_of(&self, sender: &str) -> Result<Caller, Refusal> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(sender)
            .cloned()
            .ok_or(Refusal::NotAllowed)
    }
}
