//! The two TOML files memoryd writes: `memory.toml` (the rule set) and `spaces.toml`.

use almanac_core::{RuleSet, SpaceMeta};
use serde::{Deserialize, Serialize};

/// Why a config file could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The value could not be written as TOML.
    #[error("encode: {0}")]
    Encode(String),
    /// The text is not a valid file.
    #[error("decode: {0}")]
    Decode(String),
}

/// `spaces.toml`: one `[[spaces]]` table per Space.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SpacesFile {
    /// The Spaces memory knows.
    pub spaces: Vec<SpaceMeta>,
}

/// `memory.toml` as text.
pub fn rules_to_toml(rules: &RuleSet) -> Result<String, ConfigError> {
    toml::to_string(rules).map_err(|e| ConfigError::Encode(e.to_string()))
}

/// The rule set in `memory.toml` text.
pub fn rules_from_toml(text: &str) -> Result<RuleSet, ConfigError> {
    toml::from_str(text).map_err(|e| ConfigError::Decode(e.to_string()))
}

/// `spaces.toml` as text.
pub fn spaces_to_toml(file: &SpacesFile) -> Result<String, ConfigError> {
    toml::to_string(file).map_err(|e| ConfigError::Encode(e.to_string()))
}

/// The Spaces in `spaces.toml` text.
pub fn spaces_from_toml(text: &str) -> Result<SpacesFile, ConfigError> {
    toml::from_str(text).map_err(|e| ConfigError::Decode(e.to_string()))
}
