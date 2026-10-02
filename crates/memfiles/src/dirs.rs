//! `PlainDir` and `SealedDir`: the vaults on disk. Frozen signatures; the bodies are
//! `todo!()` until the memfiles fill.

use crate::vault::{Vault, VaultError, VaultPath};
use almanac_core::SpaceId;
use almanac_seal::SubKey;
use std::path::PathBuf;

/// Plain markdown files under a directory (the vault of a Space that chose `plain`).
#[derive(Debug, Clone)]
pub struct PlainDir {
    root: PathBuf,
}

impl PlainDir {
    /// A vault rooted at `root` (the Space's directory).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The root directory.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }
}

impl Vault for PlainDir {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        let _ = dir;
        todo!("walk `root/dir` recursively, sorted, as vault paths")
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        let _ = p;
        todo!("std::fs::read, NotFound on ENOENT")
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        let _ = (p, bytes);
        todo!("create parents, write a temporary file beside it, fsync, rename")
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        let _ = p;
        todo!("std::fs::remove_file, NotFound on ENOENT")
    }
}

/// A `PlainDir` whose files are sealed with the Space's files subkey, bound to the Space and
/// the vault-relative path (`almanac_seal::Aad::file`).
#[derive(Debug)]
pub struct SealedDir {
    inner: PlainDir,
    space: SpaceId,
    key: SubKey,
}

impl SealedDir {
    /// A sealed vault for `space`, with the key derived for `Purpose::Files`.
    pub fn new(inner: PlainDir, space: SpaceId, key: SubKey) -> Self {
        Self { inner, space, key }
    }

    /// The Space the files are bound to.
    pub fn space(&self) -> &SpaceId {
        &self.space
    }

    /// The directory beneath.
    pub fn inner(&self) -> &PlainDir {
        &self.inner
    }

    /// The files subkey.
    pub fn key(&self) -> &SubKey {
        &self.key
    }
}

impl Vault for SealedDir {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        let _ = dir;
        todo!("list the inner directory")
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        let _ = p;
        todo!("read the inner file, then almanac_seal::unseal with Aad::file(space, path)")
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        let _ = (p, bytes);
        todo!(
            "seal with a random nonce and Aad::file(space, path), then write the inner file atomically"
        )
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        let _ = p;
        todo!("remove the inner file")
    }
}
