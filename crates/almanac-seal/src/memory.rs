//! `MemoryKeys`: an in-memory key store for tests.

use crate::keys::SpaceKey;
use crate::store::{KeyError, KeyStore};
use almanac_core::SpaceId;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Whether the store answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Lock {
    #[default]
    Unlocked,
    Locked,
}

#[derive(Debug, Default)]
struct Inner {
    keys: BTreeMap<SpaceId, SpaceKey>,
    lock: Lock,
}

/// Keys in a map, with a lock a test can turn.
#[derive(Debug, Default)]
pub struct MemoryKeys {
    inner: Mutex<Inner>,
}

impl MemoryKeys {
    /// An empty, unlocked store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every call answer [`KeyError::Locked`].
    pub fn lock(&self) {
        self.with(|inner| inner.lock = Lock::Locked);
    }

    /// Answers again.
    pub fn unlock(&self) {
        self.with(|inner| inner.lock = Lock::Unlocked);
    }

    fn with<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    fn unlocked<T>(
        &self,
        f: impl FnOnce(&mut BTreeMap<SpaceId, SpaceKey>) -> Result<T, KeyError>,
    ) -> Result<T, KeyError> {
        self.with(|inner| match inner.lock {
            Lock::Locked => Err(KeyError::Locked),
            Lock::Unlocked => f(&mut inner.keys),
        })
    }
}

impl KeyStore for MemoryKeys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.unlocked(|keys| keys.get(space).cloned().ok_or(KeyError::Missing))
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        self.unlocked(|keys| {
            if keys.contains_key(space) {
                return Err(KeyError::Exists);
            }
            let key = SpaceKey::generate().map_err(|e| KeyError::Store(e.to_string()))?;
            keys.insert(space.clone(), key.clone());
            Ok(key)
        })
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        self.unlocked(|keys| keys.remove(space).map(drop).ok_or(KeyError::Missing))
    }
}
