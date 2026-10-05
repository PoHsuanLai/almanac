//! `FileKeys`: Space keys in one sealed file, for a jailed test that has no Secret Service.
//!
//! TEST ONLY (feature `test-keys`, off by default, never in a release or dist build). The file is
//! sealed with a wrapping key that is a constant in this source, so it only keeps the keys from
//! sitting as plain text; it protects nothing from anyone who can read the file and this code.
//! A production daemon must use the Secret Service (`Oo7Keys`).

use crate::keys::{Purpose, SpaceKey, derive};
use crate::seal::{Aad, Nonce, seal, unseal};
use crate::store::{KeyError, KeyStore};
use almanac_core::{SpaceId, from_hex, hex_of};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The constant the file is wrapped with (see the module note: this is not secrecy).
const WRAP: [u8; 32] = *b"almanac test-keys: not a secret.";

/// Keys in a sealed file at one path, read and rewritten on every call, so two runs over the same
/// path share keys as successive runs share the Secret Service.
#[derive(Debug)]
pub struct FileKeys {
    path: PathBuf,
    /// Calls are serialised: the file is read, changed and written back.
    gate: Mutex<()>,
}

fn aad() -> Aad {
    Aad::file(&SpaceId::desktop(), "test-keys")
}

fn wrap_key() -> crate::keys::SubKey {
    derive(
        &SpaceKey::from_bytes(WRAP),
        &SpaceId::desktop(),
        Purpose::Files,
    )
}

fn store_error(e: impl std::fmt::Display) -> KeyError {
    KeyError::Store(e.to_string())
}

type Table = BTreeMap<SpaceId, SpaceKey>;

fn parse(text: &str) -> Result<Table, KeyError> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (id, hex) = line
                .split_once(' ')
                .ok_or_else(|| store_error("bad line"))?;
            let id = SpaceId::parse(id).map_err(store_error)?;
            let bytes = from_hex::<32>(hex).ok_or_else(|| store_error("bad key"))?;
            Ok((id, SpaceKey::from_bytes(bytes)))
        })
        .collect()
}

fn render(table: &Table) -> String {
    table
        .iter()
        .map(|(id, key)| format!("{} {}\n", id.as_str(), hex_of(key.expose())))
        .collect()
}

impl FileKeys {
    /// A store at `path`; the file is made at the first key.
    pub fn at(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            gate: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<Table, KeyError> {
        match std::fs::read(&self.path) {
            Ok(sealed) => {
                let plain = unseal(&wrap_key(), &aad(), &sealed).map_err(store_error)?;
                parse(std::str::from_utf8(&plain).map_err(store_error)?)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Table::new()),
            Err(e) => Err(store_error(e)),
        }
    }

    fn write(&self, table: &Table) -> Result<(), KeyError> {
        let nonce = Nonce::random().map_err(store_error)?;
        let sealed =
            seal(&wrap_key(), &aad(), render(table).as_bytes(), nonce).map_err(store_error)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(store_error)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, sealed).map_err(store_error)?;
        std::fs::rename(&tmp, &self.path).map_err(store_error)
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut Table) -> Result<(T, bool), KeyError>,
    ) -> Result<T, KeyError> {
        let _gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut table = self.read()?;
        let (out, changed) = f(&mut table)?;
        if changed {
            self.write(&table)?;
        }
        Ok(out)
    }
}

impl KeyStore for FileKeys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.with(|t| Ok((t.get(space).cloned().ok_or(KeyError::Missing)?, false)))
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.with(|t| {
            if t.contains_key(space) {
                return Err(KeyError::Exists);
            }
            let key = SpaceKey::generate().map_err(store_error)?;
            t.insert(space.clone(), key.clone());
            Ok((key, true))
        })
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        self.with(|t| t.remove(space).map(|_| ((), true)).ok_or(KeyError::Missing))
    }
}
