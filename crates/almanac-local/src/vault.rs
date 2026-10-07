//! A Space's vault: sealed or plain, by the Space's choice.

use memfiles::{PlainDir, SealedDir, Vault, VaultError, VaultPath};

/// Sealed per file (the default) or plain markdown. A closed set, so an enum.
#[derive(Debug)]
pub enum LocalVault {
    /// Sealed per file.
    Sealed(SealedDir),
    /// Plain markdown.
    Plain(PlainDir),
}

impl Vault for LocalVault {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        match self {
            LocalVault::Sealed(v) => v.list(dir),
            LocalVault::Plain(v) => v.list(dir),
        }
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        match self {
            LocalVault::Sealed(v) => v.read(p),
            LocalVault::Plain(v) => v.read(p),
        }
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        match self {
            LocalVault::Sealed(v) => v.write_atomic(p, bytes),
            LocalVault::Plain(v) => v.write_atomic(p, bytes),
        }
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        match self {
            LocalVault::Sealed(v) => v.remove(p),
            LocalVault::Plain(v) => v.remove(p),
        }
    }
}
