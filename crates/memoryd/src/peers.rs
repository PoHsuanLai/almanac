//! Who is calling: the bus connection's unique name to a `Caller`.
//!
//! The caller class is derived by the transport, never sent (memory.md section 3.9): the bus
//! says which process owns a connection and `/proc/<pid>/cgroup` says which unit or app scope
//! it runs in (porter's `ProcCallers`; the executable is not read, because a Landlock domain
//! may not read another process's `exe`). `memory-callers.toml` gives the unit or app its role
//! and `callers::caller_for` maps that role onto almanac's callers. A process in no named unit
//! and no app scope (a terminal's child) is refused. This is advisory for unsandboxed
//! processes (porter R12).

use crate::callers::caller_for;
use almanac_core::Refusal;
use almanac_service::Caller;
use porter_callers::{Callers, ProcCallers};
use std::collections::BTreeMap;
use std::future::Future;

use std::sync::{Mutex, PoisonError};

/// How long the bus and `/proc` have to say who a connection is.
const LOOKUP_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Who a bus connection is.
pub trait Peers: Send + Sync + 'static {
    /// The caller behind the connection `sender` (its unique name), or why it is nobody.
    fn caller_of(&self, sender: &str) -> impl Future<Output = Result<Caller, Refusal>> + Send;

    /// The unique names of the connections that may hear memory's signals: the router's and the
    /// shell's, as far as they are known. Signals name Spaces and what happened in them, so an
    /// app never hears them. The default knows nobody.
    fn listeners(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Whether the signals about memory go to `caller`.
fn hears_signals(caller: &Caller) -> bool {
    matches!(caller, Caller::Router | Caller::ShellUi)
}

/// Peers by process: porter's [`ProcCallers`] over the connection, mapped onto almanac's callers.
#[derive(Debug)]
pub struct ProcPeers {
    callers: ProcCallers,
    /// The connections identified so far, by unique name.
    seen: Mutex<BTreeMap<String, Caller>>,
}

impl ProcPeers {
    /// Resolves senders on `connection` through `table`, reading the system's `/proc`.
    pub fn new(connection: zbus::Connection, table: porter_callers::CallerTable) -> Self {
        Self::over(ProcCallers::new(connection, table))
    }

    /// As [`ProcPeers::new`], reading the fixture tree at `proc_root` instead of `/proc` and the
    /// user manager. Only in a test build (feature `test-proc-root`).
    #[cfg(feature = "test-proc-root")]
    pub fn with_proc_root(
        connection: zbus::Connection,
        table: porter_callers::CallerTable,
        proc_root: std::path::PathBuf,
    ) -> Self {
        Self::over(ProcCallers::with_proc_root(connection, table, proc_root))
    }

    fn over(callers: ProcCallers) -> Self {
        Self {
            callers,
            seen: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Peers for ProcPeers {
    async fn caller_of(&self, sender: &str) -> Result<Caller, Refusal> {
        // An identity lookup that does not finish is a refusal, never a wait.
        let caller = tokio::time::timeout(LOOKUP_WAIT, self.callers.caller_of(sender))
            .await
            .map_err(|_| Refusal::NotAllowed)?
            .map(caller_for)
            .ok_or(Refusal::NotAllowed)?;
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(sender.to_owned(), caller.clone());
        Ok(caller)
    }

    fn listeners(&self) -> Vec<String> {
        names_hearing(&self.seen)
    }
}

fn names_hearing(known: &Mutex<BTreeMap<String, Caller>>) -> Vec<String> {
    known
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(_, caller)| hears_signals(caller))
        .map(|(name, _)| name.clone())
        .collect()
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

    fn listeners(&self) -> Vec<String> {
        names_hearing(&self.0)
    }
}
