//! The key store seam: where Space keys live (the Secret Service, or memory in tests).

use crate::keys::SpaceKey;
use almanac_core::SpaceId;
use std::future::Future;

/// Why a key could not be had.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The store is locked (the keyring is, or the session has not unlocked it).
    #[error("the key store is locked")]
    Locked,
    /// There is no key for the Space.
    #[error("no key for the Space")]
    Missing,
    /// There already is a key for the Space.
    #[error("a key for the Space already exists")]
    Exists,
    /// The store failed.
    #[error("key store: {0}")]
    Store(String),
}

/// Where Space keys are kept. Implementations: `MemoryKeys` (feature `testing`), `Oo7Keys`
/// (feature `oo7`; Secret Service items with attributes `xdg:schema` =
/// `org.quire.Memory.SpaceKey` and `space` = the id).
pub trait KeyStore: Send + Sync {
    /// The Space's key.
    fn get(&self, space: &SpaceId) -> impl Future<Output = Result<SpaceKey, KeyError>> + Send;
    /// Makes and files a new key; fails if one exists.
    fn create(&self, space: &SpaceId) -> impl Future<Output = Result<SpaceKey, KeyError>> + Send;
    /// Destroys the key: after this the Space's sealed data cannot be read by anyone.
    fn destroy(&self, space: &SpaceId) -> impl Future<Output = Result<(), KeyError>> + Send;
}
