//! memoryd's core over seams: the authorisation matrix, the Space and forget-plan machines,
//! retention, timeline assembly, the export writer, the per-Space request queue, the config
//! files, and `MemoryService` over a `Backend`. It drives the fact and consolidation machines,
//! the prompt and the removal notes of `almanac-tidy` over its seams.
//!
//! Pure over its seams (`Clock`, `KeyStore`, `Vault`, `LogWrite`, `SearchIndex`, `Embedder`,
//! `almanac_tidy::Consolidator`): the store seams come from `almanac-store`, so the service links no SQLite,
//! SQLCipher or OpenSSL, and builds wherever the stores do. Whoever builds a service picks the
//! concrete stores by implementing [`Backend`] (`almanac-local`, `almanac-fake`, `memoryd`). The
//! machines return effects as values.
//!
//! ```
//! use almanac_core::{MemoryReply, MemoryRequest, RuleSet};
//! use almanac_fake::{FakeBackend, ScriptedConsolidator};
//! use almanac_service::{Backend, Caller, MemoryService};
//!
//! // Generic over the backend: nothing here names a store.
//! fn serve<B: Backend>(backend: B) -> MemoryService<B> {
//!     MemoryService::new(backend, RuleSet::standard())
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # tokio::runtime::Builder::new_current_thread().build()?.block_on(async {
//! let service = serve(FakeBackend::new(ScriptedConsolidator::default()));
//! let reply = service.handle(&Caller::ShellUi, MemoryRequest::Spaces).await;
//! assert!(matches!(reply, MemoryReply::Spaces(_)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! # })
//! # }
//! ```

mod auth;
mod backend;
mod baseline;
mod caller;
mod clock;
mod config;
mod control;
mod dispatch;
mod docs;
mod durable;
mod edits;
mod entries;
mod erase;
mod events;
mod export;
mod facts;
pub mod forget;
mod grounds;
mod hunks;
mod key_refusal;
mod marks;
mod open;
mod proposals;
mod queue;
mod quota;
mod readers;
mod record;
mod registry;
mod rehome;
mod relocate;
mod relocate_events;
mod retention;
mod revert_guard;
mod run;
mod runfile;
mod runtime;
mod service;
pub mod space;
mod sweep;
mod timeline;

pub use auth::allowed;
pub use backend::{Backend, BackendError};
pub use caller::Caller;
pub use clock::Clock;
pub use config::{
    ConfigError, SpacesFile, rules_from_toml, rules_to_toml, spaces_from_toml, spaces_to_toml,
};
pub use events::{ServiceEvent, locked_status};
pub use export::{
    ErasedTag, EventLine, ExportWriter, ExportedBody, events_path, file_path, manifest_path,
    rules_path,
};
pub use forget::{
    ApplyStep, FactGraph, FactNode, PLAN_TTL_SECONDS, Plan, PlanEffect, PlanEvent, PlanState,
    plan_forget, plan_step, token_for,
};
pub use queue::{Claim, claim_for, claim_of};
pub use quota::{ByteCap, EventCap, FieldLimits, Quota};
pub use registry::{Listing, RegistryError};
pub use retention::{SourceState, Sweep, header_expired, sweep_body};
pub use runtime::SpaceRuntime;
pub use service::MemoryService;
pub use space::{BUFFER_LIMIT, SpaceEffect, SpaceEvent};
pub use timeline::timeline_entry;
