//! The two TOML files memoryd writes: `memory.toml` (the rule set) and `spaces.toml`.

use almanac_core::{RuleSet, SpaceMeta};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

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

/// Writes `text` to `path` so that a crash leaves the old file or the new one, never a cut one: a
/// temp file next to it (mode 0600), synced, renamed over it, then the directory synced. The
/// parent directory is created.
pub fn write_private_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temp, path));
    written.inspect_err(|_| drop(std::fs::remove_file(&temp)))?;
    std::fs::File::open(dir).and_then(|d| d.sync_all())
}

/// The text of a config file: `None` when there is no such file (a fresh install), an error for
/// any other failure to read it, so a file that cannot be read is never taken for an empty one.
pub fn read_config(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
