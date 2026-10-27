//! What memory learns and keeps tidy, as pure values: the fact lifecycle machine ([`fact`]), the
//! consolidation run machine, its model seam and the draft check ([`consolidation`]), the
//! consolidation prompt and the parse of the model's answer, the check of a tidied topic file,
//! what a reply changed ([`changes`]) and the Space removal notes ([`RemovalLedger`]).
//!
//! Nothing here reads a file, a clock or the environment, and nothing names a store: the
//! service (`almanac-service`) drives these machines over its seams, and an app that only wants
//! to draft a consolidation (the inferd-backed `almanac-infer`) depends on this crate alone.
//!
//! ```
//! use almanac_tidy::fact::{self, FactEffect, FactEvent, FactLife};
//! use almanac_core::Lands;
//!
//! let (life, effects) = fact::step(FactLife::Unborn, FactEvent::Propose(Lands::Active));
//! assert_eq!(life, FactLife::Active);
//! assert_eq!(effects, vec![FactEffect::AppendToTopic, FactEffect::LogAdded]);
//! ```

mod change;
pub mod consolidation;
pub mod fact;
mod prompt;
mod removals;
mod tidy;

pub use change::{Change, changes};
pub use consolidation::{
    CheckedDraft, ConsolidateError, ConsolidationInput, Consolidator, Desktop, Draft, HunkFault,
    InputEvent, InputTopic, Power, check_draft, cited_label,
};
pub use prompt::{parse_draft, render_prompt};
pub use removals::{
    LedgerError, NotesFileError, RemovalLedger, Removals, removals_from_toml, removals_to_toml,
};
pub use tidy::{tidy_accepts, tidy_is_acceptable};
