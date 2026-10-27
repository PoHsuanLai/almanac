//! memoryd's library half: the real seams (`SystemBackend`, the inferd-backed
//! `InferdEmbedder` and `InferdConsolidator`), the XDG roots, the caller
//! identities and the bus handler (`Daemon`), so every module is testable on a private bus. `start` builds
//! these over the real system and serves the session bus; the binary (`main.rs`) is arguments and
//! a call to it.

mod backend;
mod callers;
mod consent;
mod daemon;
mod infer;
mod keyring;
mod keysel;
mod ledger;
mod peers;
mod registry;
mod run;
mod sandbox;
mod settings_watch;
mod signals;
mod xdg;

pub use almanac_watch::InotifyWatch as Watcher;
pub use backend::{SpaceVault, SystemBackend};
pub use callers::{
    CallerFileError, ROUTER_APP, SETTINGS_APP, SHELL_APP, caller_for, load_callers,
    table_from_file, table_from_toml,
};
pub use consent::Consent;
pub use daemon::Daemon;
pub use infer::{Deadlines, InferdConsolidator, InferdEmbedder, class_of};
pub use keyring::{LockChanges, is_lock_change};
pub use keysel::KeysError;
pub use ledger::FileLedger;
pub use peers::{Peers, ProcPeers, TablePeers};
pub use porter_daemon::{ProcGate, ProcRoot};
pub use registry::{REGISTRY_WAIT, SpaceRegistry};
pub use run::{Env, PROC_GATE, PROC_ROOT_VAR, SYSTEM_CALLERS_VAR, StartError, start};
pub use sandbox::{
    Enforcement, Policy, SandboxError, bus_socket, confinement_of, enforce, policy_for, prepare,
};
pub use settings_watch::{DEBOUNCE, SettingsWatch, WatchState, apply, apply_next};
pub use signals::{Delivery, delivery_of};
pub use xdg::{XdgError, dirs_from};

use porter_client::DbusTransport;
use std::sync::Arc;

/// The link to inferd: the session bus connection `connection`, as porter-client's D-Bus
/// transport. Nothing is called here: inferd is found (and started by activation) at the first
/// `open`, so a daemon that starts before inferd, or without it, still starts and degrades as
/// designed (lexical-only recall, no consolidation) until inferd answers.
pub fn inferd_link(connection: &zbus::Connection) -> Arc<DbusTransport> {
    Arc::new(DbusTransport::over(connection.clone()))
}

/// The card of the embedding model the default inferd tier maps (nomic-embed-text v1.5): 768
/// numbers, asymmetric prefixes. It must match what inferd serves, or the vector index refuses
/// the answers (their length differs) and search stays lexical.
pub(crate) fn default_card() -> almanac_store::EmbedderCard {
    almanac_store::EmbedderCard {
        model: "nomic-embed-text-v1.5".to_owned(),
        dims: 768,
        max_tokens: 2048,
        max_batch: almanac_store::MaxBatch(32),
        prompts: almanac_store::PromptPrefixes {
            query: "search_query: ".to_owned(),
            document: "search_document: ".to_owned(),
        },
        metric: almanac_store::Metric::Cosine,
    }
}
