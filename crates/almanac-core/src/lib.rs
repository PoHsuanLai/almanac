//! almanac's vocabulary: the things events are about, the typed event bodies, facts, retention
//! rules and the pure admission rule, the memory wire (`MemoryRequest`, `MemoryReply`), the
//! timeline and export shapes, and where memory lives on disk.
//!
//! Pure and portable: serde only. Other areas' payloads (policy, consent, computer use,
//! sessions) are stored as opaque [`AreaPayload`]s in their owners' serde form.

mod admit;
mod area;
mod caller;
mod chain;
mod dirs;
mod event;
mod export;
mod fact;
mod file;
mod ids;
mod op;
mod query;
mod reply;
mod request;
mod rules;
mod slug;
mod space;
mod text;
mod thing;
mod timeline;
mod views;

pub use admit::{admit, default_retention, glob_matches, is_audit_class};
pub use area::{AreaPayload, AreaTag};
pub use caller::Caller;
pub use chain::{Break, ChainReport, Checkpoint, Head};
pub use dirs::Dirs;
pub use event::{Cause, EventBody, EventRef, Record};
pub use export::{EXPORT_FORMAT, EXPORT_ROOT, ExportManifest, ExportedSpace};
pub use fact::{Fact, FactDraft, FactState, Link, MemoryItem, Settlement, Validity};
pub use file::{FileChange, FileView, FileWhy};
pub use ids::{
    Cursor, DayCount, FactId, FactText, KindPattern, KindTag, PathGlob, PlanToken, ReplicaId,
    RuleId, Seq, SpacePath, TopicPath, UseCount,
};
pub use op::{ExportCounts, ForgetCounts, MemoryOp, ReadScope};
pub use query::{
    ExportOptions, FactFilter, FactQuery, FileWhyClaim, MarkKind, MarkRequest, RecallOver,
    RecallQuery, VerificationKey,
};
pub use reply::{ForgetReport, MemoryReply, RecallHit, RecallWhy, Refusal};
pub use request::{ForgetScope, MemoryRequest};
pub use rules::{
    ActorClass, Admission, DropReason, FALLBACK_DAYS, HEADER_DAYS, KindRetention, Marks,
    PENDING_TTL_DAYS, RememberMode, RememberRule, Retention, RuleScope, RuleSet,
    UNEXPLAINED_FILE_DAYS,
};
pub use space::{
    ChainHealth, DegradedWhy, IndexView, SpaceMeta, SpaceState, SpaceStatus, SpaceSummary,
    StaleWhy, VaultKind,
};
pub use text::{ContentDigest, Digest32, JsonText, Link32, PlanDigest, UserText, from_hex, hex_of};
pub use thing::{ThingKey, ThingKind, ThingRef, ThingRole, ThingView, Verb};
pub use timeline::{
    ActorFilter, EntryBody, EraseCause, TimelineEntry, TimelineFilter, TimelinePage, TimelineQuery,
    TrustFilter, UndoRef,
};
pub use views::{
    ConsolidateFailure, DraftView, EventSummary, FactView, FileHistoryEntry, FileProvenance,
    ForgetPlanView, Hunk, Lands, RunState, SourceView, TidyHunk,
};

/// The wire version of `org.quire.Memory1` (its `Version` property).
pub const MEMORY_WIRE_VERSION: u32 = 1;

/// The other crates of this repo reach porter and `prov` through almanac-core, so the allowed
/// edges stay exactly the crate map's.
pub use porter_core::{
    AppId, AppName, Bytes, Count, DataClass, Isolation, SpaceId, SpaceScope, UnixSeconds,
};
pub use prov::{
    Actor, ActorKind, AgentRole, Channel, ClientName, Confidentiality, ConfirmId, ConfirmReceipt,
    Effect, InputProof, Integrity, Label, ModelRole, RunId, SessionId, Source, SystemPart,
};
