//! The vault seam: where the bytes of memory files live.

use almanac_core::{FactId, TopicPath};
use almanac_seal::SealError;
use std::fmt;

/// A path inside a vault: relative, `/`-separated, no empty segment, no `.` or `..`, no
/// control characters or backslashes, at most 512 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VaultPath(String);

impl VaultPath {
    /// `text` as a vault path, or `None` if it is not one.
    pub fn parse(text: &str) -> Option<VaultPath> {
        let ok = !text.is_empty()
            && text.len() <= 512
            && !text.chars().any(|c| c.is_control() || c == '\\')
            && text
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != "..");
        ok.then(|| VaultPath(text.to_owned()))
    }

    /// The path's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The directory part, if the path has one.
    pub fn parent(&self) -> Option<&str> {
        self.0.rsplit_once('/').map(|(dir, _)| dir)
    }

    /// Whether the path lies under the directory `dir` (at any depth).
    pub fn is_under(&self, dir: &VaultPath) -> bool {
        self.0
            .strip_prefix(dir.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    }

    /// `facts/<topic>.md`.
    pub fn topic(topic: &TopicPath) -> VaultPath {
        VaultPath(format!("{FACTS_DIR}/{topic}.md"))
    }

    /// `pending/<fact-id>.md`.
    pub fn pending(id: &FactId) -> VaultPath {
        VaultPath(format!("{PENDING_DIR}/{id}.md"))
    }

    /// `facts/INDEX.md`.
    pub fn primer() -> VaultPath {
        VaultPath(format!("{FACTS_DIR}/INDEX.md"))
    }

    /// The `facts` directory.
    pub fn facts_dir() -> VaultPath {
        VaultPath(FACTS_DIR.to_owned())
    }

    /// The `pending` directory.
    pub fn pending_dir() -> VaultPath {
        VaultPath(PENDING_DIR.to_owned())
    }
}

/// The topic files' directory under the Space.
pub const FACTS_DIR: &str = "facts";
/// Facts awaiting confirmation.
pub const PENDING_DIR: &str = "pending";

impl fmt::Display for VaultPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a vault operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// No such file.
    #[error("no such file: {0}")]
    NotFound(VaultPath),
    /// The disk said no.
    #[error("io: {0}")]
    Io(String),
    /// A sealed file did not open.
    #[error("sealed file: {0}")]
    Sealed(SealError),
}

/// Where memory files live: a directory, a sealed directory, or memory in tests. Writes are
/// atomic (temporary file, then rename) so a crash never leaves half a file.
pub trait Vault: Send + Sync {
    /// Every file under `dir`, at any depth, sorted.
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError>;
    /// A file's bytes (unsealed, for a sealed vault).
    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError>;
    /// Replaces or creates a file atomically.
    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError>;
    /// Removes a file.
    fn remove(&self, p: &VaultPath) -> Result<(), VaultError>;
}
