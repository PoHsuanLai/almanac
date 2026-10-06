//! memoryd's core over seams: the authorisation matrix, the pure state machines (Space
//! lifecycle, facts, forget plans, consolidation runs), retention, timeline assembly, the
//! export writer, the config files, and `MemoryService` over a `Backend`.
//!
//! Pure over its seams (`Clock`, `KeyStore`, `Vault`, `LogWrite`, `Embedder`, `Consolidator`):
//! it builds wherever the stores do. The machines return effects as values.

mod auth;
mod backend;
mod baseline;
mod class;
mod clock;
mod config;
pub mod consolidation;
mod control;
mod dispatch;
mod docs;
mod edits;
mod erase;
mod events;
mod export;
pub mod fact;
mod facts;
pub mod forget;
mod grounds;
mod hunks;
mod marks;
mod open;
mod proposals;
mod record;
mod retention;
mod run;
mod runfile;
mod search;
mod service;
mod settings;
pub mod space;
mod sweep;
mod timeline;

pub use auth::{Allowed, allowed};
pub use backend::{Backend, BackendError};
pub use class::{class_from_tag, class_of, class_tag};
pub use clock::Clock;
pub use config::{
    ConfigError, SpacesFile, rules_from_toml, rules_to_toml, spaces_from_toml, spaces_to_toml,
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
pub use retention::{SourceState, Sweep, header_expired, header_expired_after, sweep_body};
pub use service::{MemoryService, SpaceRuntime};
pub use settings::{
    ConsolidateApply, ConsolidateWhen, Fallback, Loaded, Locator, MemorySettings, SCHEMA,
    SETTINGS_FILE, Why, read as read_settings,
};
pub use space::{BUFFER_LIMIT, SpaceEffect, SpaceEvent};
pub use timeline::timeline_entry;
