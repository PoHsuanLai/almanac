//! memoryd's library half: the real seams (`SystemBackend`, `SystemClock`, the inferd-backed
//! `InferdEmbedder` and `InferdConsolidator`), the XDG roots, the per-Space queues, the caller
//! identities and the bus handler (`Daemon`), so every module is testable on a private bus. The
//! binary is `main.rs`: it builds these over the real system and serves the session bus.

mod backend;
mod clock;
mod daemon;
mod infer;
mod keyring;
mod peers;
mod queue;
mod sandbox;
mod signals;
mod xdg;

pub use almanac_dbus::{MEMORY_BUS, MEMORY_PATH};
pub use almanac_watch::InotifyWatch as Watcher;
pub use backend::{SpaceVault, SystemBackend};
pub use clock::SystemClock;
pub use daemon::Daemon;
pub use infer::{InferdConsolidator, InferdEmbedder, class_of, parse_draft, render_prompt};
pub use keyring::{LockChanges, is_lock_change};
pub use peers::{CallerTable, Peers, ProcPeers, TablePeers};
pub use queue::{Claim, Serialised, claim_of};
pub use sandbox::{Enforcement, Policy, SandboxError, bus_socket, enforce, policy_for, prepare};
pub use signals::{FollowUp, follow_ups};
pub use xdg::{XdgError, dirs_from, dirs_from_env};

use porter_client::{AnyTransport, DbusTransport};
use std::sync::Arc;

/// The link to inferd: the session bus connection `connection`, as porter-client's D-Bus
/// transport. Nothing is called here: inferd is found (and started by activation) at the first
/// `open`, so a daemon that starts before inferd, or without it, still starts and degrades as
/// designed (lexical-only recall, no consolidation) until inferd answers.
pub fn inferd_link(connection: &zbus::Connection) -> Arc<AnyTransport> {
    Arc::new(AnyTransport::Dbus(DbusTransport::over(connection.clone())))
}

/// The card of the embedding model the default inferd tier maps (nomic-embed-text v1.5): 768
/// numbers, asymmetric prefixes. It must match what inferd serves, or the vector index refuses
/// the answers (their length differs) and search stays lexical.
pub fn default_card() -> recall::EmbedderCard {
    recall::EmbedderCard {
        model: "nomic-embed-text-v1.5".to_owned(),
        dims: 768,
        max_tokens: 2048,
        max_batch: recall::MaxBatch(32),
        prompts: recall::PromptPrefixes {
            query: "search_query: ".to_owned(),
            document: "search_document: ".to_owned(),
        },
        metric: recall::Metric::Cosine,
    }
}
