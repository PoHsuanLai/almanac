//! The person's memory settings (design/22 section 3.28): the `memory.*` keys the Settings app
//! writes to `$XDG_CONFIG_HOME/almanac/settings.toml`, the schema it draws them from, the lenient
//! reader (a bad value falls back, per key, and is reported) and where the file is found. Reading
//! is a pure function of text; only [`Locator::read`] touches the disk. The live watch is memoryd's.
//!
//! The service holds a [`MemorySettings`] and every request reads it afresh, so a change applies
//! to the next request (`MemoryService::apply_settings`).

mod keys;
mod locate;
mod read;
#[cfg(test)]
mod tests;

use almanac_core::{DayCount, RetentionDays, VaultKind};

pub use locate::Locator;
pub use read::{Fallback, Loaded, Why, read};

/// The schema almanac ships for its settings (design/22 section 9.2).
pub const SCHEMA: &str = include_str!("../../../../dist/settings/almanac.settings.toml");

/// The file the settings live in, under the configuration directory.
pub const SETTINGS_FILE: &str = "almanac/settings.toml";

/// `memory.consolidation.when`: when memory tidies itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsolidateWhen {
    /// Every night.
    Nightly,
    /// Only when the person asks.
    Manual,
    /// Not at all: a request to consolidate is refused.
    Never,
}

/// `memory.consolidation.apply`: whether a drafted run is applied at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsolidateApply {
    /// The service applies a run's hunks the moment it has drafted and checked them (revertible).
    Auto,
    /// A run stops at `Proposed`; nothing changes until the person sends `ApplyConsolidation`.
    Review,
}

/// Every value of the file, typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySettings {
    /// `memory.files.at_rest`: how a new Space's files are held. A Space that exists keeps the
    /// way it was made (its entry in `spaces.toml`).
    pub at_rest: VaultKind,
    /// `memory.retention.*`.
    pub retention: RetentionDays,
    /// `memory.pending_ttl_days`: how long a pending fact waits for the person.
    pub pending_ttl: DayCount,
    /// `memory.consolidation.when`.
    pub consolidate: ConsolidateWhen,
    /// `memory.consolidation.apply`.
    pub apply: ConsolidateApply,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            at_rest: VaultKind::Sealed,
            retention: RetentionDays::default(),
            pending_ttl: almanac_core::PENDING_TTL_DAYS,
            consolidate: ConsolidateWhen::Nightly,
            apply: ConsolidateApply::Auto,
        }
    }
}
