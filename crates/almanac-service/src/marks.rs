//! "Do not remember" marks, kept in the Space's vault (`meta/marks.json`: sealed like every
//! other file of a sealed Space) so they outlive the daemon.

use almanac_core::Marks;
use almanac_store::{Vault, VaultError, VaultPath};

fn path() -> Option<VaultPath> {
    VaultPath::parse("meta/marks.json")
}

/// The marks the vault holds; none when the file is absent or unreadable (an unreadable file
/// must not stop the Space opening, and a mark is only a request to remember less).
pub(crate) fn load(vault: &impl Vault) -> Marks {
    path()
        .and_then(|p| vault.read(&p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes the marks.
pub(crate) fn save(vault: &impl Vault, marks: &Marks) -> Result<(), VaultError> {
    let bytes = serde_json::to_vec(marks).map_err(|e| VaultError::Io(e.to_string()))?;
    match path() {
        Some(p) => vault.write_atomic(&p, &bytes),
        None => Err(VaultError::Io("marks path".into())),
    }
}
