//! `MemoryVault`: files in a map, for tests and almanac-fake.

use crate::vault::{Vault, VaultError, VaultPath};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// A vault in memory with `Vault`'s contract.
#[derive(Debug, Default)]
pub struct MemoryVault {
    files: Mutex<BTreeMap<VaultPath, Vec<u8>>>,
}

impl MemoryVault {
    /// An empty vault.
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut BTreeMap<VaultPath, Vec<u8>>) -> T) -> T {
        f(&mut self
            .files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

impl Vault for MemoryVault {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        Ok(self.with(|files| files.keys().filter(|p| p.is_under(dir)).cloned().collect()))
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        self.with(|files| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| VaultError::NotFound(p.clone()))
        })
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        self.with(|files| files.insert(p.clone(), bytes.to_vec()));
        Ok(())
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        self.with(|files| {
            files
                .remove(p)
                .map(drop)
                .ok_or_else(|| VaultError::NotFound(p.clone()))
        })
    }
}
