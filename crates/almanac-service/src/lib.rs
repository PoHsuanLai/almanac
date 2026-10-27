//! memoryd's core over seams: the authorisation matrix, the pure state machines (Space
//! lifecycle, facts, forget plans, consolidation runs), retention, timeline assembly, the
//! export writer, the per-Space request queue, the config files, and `MemoryService` over a `Backend`.
//!
//! Pure over its seams (`Clock`, `KeyStore`, `Vault`, `LogWrite`, `SearchIndex`, `Embedder`,
//! `Consolidator`): the store seams come from `almanac-store`, so the service links no SQLite,
//! SQLCipher or OpenSSL, and builds wherever the stores do. Whoever builds a service picks the
//! concrete stores by implementing [`Backend`] (`almanac-local`, `almanac-fake`, `memoryd`). The
//! machines return effects as values.
//!
//! ```
//! use almanac_core::{Caller, MemoryReply, MemoryRequest, RuleSet};
//! use almanac_fake::{FakeBackend, ScriptedConsolidator};
//! use almanac_service::{Backend, MemoryService};
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
mod change;
mod class;
mod clock;
mod config;
pub mod consolidation;
mod control;
mod dispatch;
mod docs;
mod durable;
mod edits;
mod entries;
mod erase;
mod events;
mod export;
pub mod fact;
mod facts;
pub mod forget;
mod grounds;
mod hunks;
mod key_refusal;
mod marks;
mod open;
mod prompt;
mod proposals;
mod queue;
mod quota;
mod record;
mod registry;
mod rehome;
mod relocate;
mod relocate_events;
mod removals;
mod retention;
mod revert_guard;
mod run;
mod runfile;
mod runtime;
mod search;
mod service;
mod settings;
pub mod space;
mod sweep;
mod timeline;

pub use auth::{Allowed, allowed};
pub use backend::{Backend, BackendError};
pub use change::{Change, changes};
pub use class::{class_from_tag, class_of, class_tag};
pub use clock::Clock;
pub use config::{
    ConfigError, SpacesFile, read_config, rules_from_toml, rules_to_toml, spaces_from_toml,
    spaces_to_toml, write_private_atomic,
};
pub use consolidation::{
    CheckedDraft, ConsolidateError, ConsolidationInput, Consolidator, Desktop, Draft, HunkFault,
    InputEvent, InputTopic, Power, check_draft, cited_label,
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
pub use hunks::tidy_is_acceptable;
pub use prompt::{parse_draft, render_prompt};
pub use queue::{Claim, claim_for, claim_of};
pub use quota::{ByteCap, EventCap, FieldLimits, Quota};
pub use registry::{Listing, RegistryError};
pub use removals::{LedgerError, RemovalLedger, Removals, removals_from_toml, removals_to_toml};
pub use retention::{SourceState, Sweep, header_expired, sweep_body};
pub use runtime::SpaceRuntime;
pub use service::MemoryService;
pub use settings::{
    CONFIRMED_FILE, ConfirmError, Confirmations, ConsolidateApply, ConsolidateWhen, Fallback,
    Loaded, Locator, MemorySettings, SCHEMA, SETTINGS_FILE, Why, loosenings, read as read_settings,
};
pub use space::{BUFFER_LIMIT, SpaceEffect, SpaceEvent};
pub use timeline::timeline_entry;
