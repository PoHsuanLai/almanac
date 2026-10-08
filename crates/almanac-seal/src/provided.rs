//! `ProvidedKeys`: a key store with no platform behind it. The app provides one 32-byte master
//! key (from its own sign-in, a passphrase KDF, the OS keychain it already uses, a file); each
//! Space's key is derived from it, so there is nothing to file, nothing to lock, and no Secret
//! Service. This is the portable key source for in-process use on macOS, Windows and other
//! desktops; `Oo7Keys` is the desktop's.

use crate::keys::SpaceKey;
use crate::store::{KeyError, KeyStore};
use almanac_core::SpaceId;
use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

/// Derivation context for a Space key from the master key. Pinned by a golden test.
const CONTEXT: &str = "quire-memory 1 provided space key";

/// The master key and the Spaces destroyed in this process.
#[derive(Debug)]
pub struct ProvidedKeys {
    master: SpaceKey,
    destroyed: Mutex<BTreeSet<SpaceId>>,
}

impl ProvidedKeys {
    /// A store whose Space keys derive from `master`.
    pub fn new(master: SpaceKey) -> Self {
        Self {
            master,
            destroyed: Mutex::new(BTreeSet::new()),
        }
    }

    /// The Space's derived key: a pure function of the master key and the id.
    pub fn derived(&self, space: &SpaceId) -> SpaceKey {
        let mut hasher = blake3::Hasher::new_derive_key(CONTEXT);
        hasher.update(self.master.expose());
        hasher.update(space.as_str().as_bytes());
        SpaceKey::from_bytes(*hasher.finalize().as_bytes())
    }

    fn live(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        let destroyed = self
            .destroyed
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if destroyed.contains(space) {
            return Err(KeyError::Destroyed);
        }
        Ok(self.derived(space))
    }
}

/// A derived key exists for every Space, so `get` and `create` answer the same key. `destroy`
/// bars the Space for the life of this store; across restarts, erasing a Space for good means
/// the app discarding the master key (or re-keying), because nothing here is persisted.
impl KeyStore for ProvidedKeys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.live(space)
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.live(space)
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        self.destroyed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(space.clone());
        Ok(())
    }
}
