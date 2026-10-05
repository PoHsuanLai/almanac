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
use almanac_core::{Caller, Refusal};
use porter_dbus::{Callers, ProcCallers};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

/// Who a bus connection is.
pub trait Peers: Send + Sync + 'static {
    /// The caller behind the connection `sender` (its unique name), or why it is nobody.
    fn caller_of(&self, sender: &str) -> impl Future<Output = Result<Caller, Refusal>> + Send;
}

/// Peers by process: porter's [`ProcCallers`] over the connection, mapped onto almanac's callers.
#[derive(Debug)]
pub struct ProcPeers(ProcCallers);

impl ProcPeers {
    /// Resolves senders on `connection` through `table`, reading the system's `/proc`.
    pub fn new(connection: zbus::Connection, table: porter_dbus::CallerTable) -> Self {
        Self(ProcCallers::new(connection, table))
    }

    /// As [`ProcPeers::new`], reading the `/proc` tree at `proc_root` (a test build's fixture).
    pub fn with_proc_root(
        connection: zbus::Connection,
        table: porter_dbus::CallerTable,
        proc_root: PathBuf,
    ) -> Self {
        Self(ProcCallers::with_proc_root(connection, table, proc_root))
    }
}

impl Peers for ProcPeers {
    async fn caller_of(&self, sender: &str) -> Result<Caller, Refusal> {
        self.0
            .caller_of(sender)
            .await
            .map(caller_for)
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
