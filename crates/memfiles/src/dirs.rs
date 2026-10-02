//! `PlainDir` and `SealedDir`: the vaults on disk. Writes are a temporary file beside the
//! target, fsync, then rename, so a crash never leaves half a file.

use crate::vault::{Vault, VaultError, VaultPath};
use almanac_core::SpaceId;
use almanac_seal::{Aad, Nonce, SubKey, seal, unseal};
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

fn io(err: std::io::Error) -> VaultError {
    VaultError::Io(err.to_string())
}

fn not_found_or_io(p: &VaultPath, err: std::io::Error) -> VaultError {
    match err.kind() {
        ErrorKind::NotFound => VaultError::NotFound(p.clone()),
        _ => io(err),
    }
}

impl PlainDir {
    fn on_disk(&self, p: &VaultPath) -> PathBuf {
        p.as_str()
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    /// Every file under `dir` (relative to the root), depth first, unsorted.
    fn walk(&self, dir: &Path, rel: &str, out: &mut Vec<VaultPath>) -> Result<(), VaultError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(io(err)),
        };
        for entry in entries {
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            // Names that are not UTF-8, or are not vault paths, or are our own temporaries,
            // are not part of the vault.
            let Some(name) = name.to_str().filter(|n| !n.starts_with(TEMP_PREFIX)) else {
                continue;
            };
            let child = format!("{rel}/{name}");
            let kind = entry.file_type().map_err(io)?;
            if kind.is_dir() {
                self.walk(&entry.path(), &child, out)?;
            } else if kind.is_file() {
                out.extend(VaultPath::parse(&child));
            }
        }
        Ok(())
    }
}

/// Temporary files are named with this prefix beside their target, and never listed.
const TEMP_PREFIX: &str = ".almanac-tmp-";

/// A name for a temporary file that two writers will not share.
fn temp_name(target: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let file = target
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default();
    target.with_file_name(format!("{TEMP_PREFIX}{}-{n}-{file}", std::process::id()))
}

impl Vault for PlainDir {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        let mut out = Vec::new();
        self.walk(&self.on_disk(dir), dir.as_str(), &mut out)?;
        out.sort();
        Ok(out)
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        std::fs::read(self.on_disk(p)).map_err(|err| not_found_or_io(p, err))
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        let target = self.on_disk(p);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let temp = temp_name(&target);
        let written = File::create(&temp).and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
        written
            .and_then(|()| std::fs::rename(&temp, &target))
            .inspect_err(|_| drop(std::fs::remove_file(&temp)))
            .map_err(io)?;
        // Persist the rename itself; a directory that cannot be synced is not an error.
        if let Some(dir) = target.parent().and_then(|d| File::open(d).ok()) {
            drop(dir.sync_all());
        }
        Ok(())
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        std::fs::remove_file(self.on_disk(p)).map_err(|err| not_found_or_io(p, err))
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

impl SealedDir {
    fn aad(&self, p: &VaultPath) -> Aad {
        Aad::file(&self.space, p.as_str())
    }
}

impl Vault for SealedDir {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        self.inner.list(dir)
    }

    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        let sealed = self.inner.read(p)?;
        unseal(&self.key, &self.aad(p), &sealed).map_err(VaultError::Sealed)
    }

    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        let nonce = Nonce::random().map_err(|err| VaultError::Io(err.to_string()))?;
        let sealed = seal(&self.key, &self.aad(p), bytes, nonce).map_err(VaultError::Sealed)?;
        self.inner.write_atomic(p, &sealed)
    }

    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        self.inner.remove(p)
    }
}
