//! `SpaceVault`: a Space's vault, sealed or plain by the Space's choice.

use crate::dirs::{PlainDir, SealedDir};
use almanac_store::{Vault, VaultError, VaultPath};

/// Sealed per file (the default) or plain markdown. A closed set, so an enum.
#[derive(Debug)]
pub enum SpaceVault {
    /// Sealed per file.
    Sealed(SealedDir),
    /// Plain markdown.
    Plain(PlainDir),
}

impl Vault for SpaceVault {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.list(dir),
            SpaceVault::Plain(v) => v.list(dir),
        }
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.read(p),
            SpaceVault::Plain(v) => v.read(p),
        }
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.write_atomic(p, bytes),
            SpaceVault::Plain(v) => v.write_atomic(p, bytes),
        }
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        match self {
            SpaceVault::Sealed(v) => v.remove(p),
            SpaceVault::Plain(v) => v.remove(p),
        }
    }
}
