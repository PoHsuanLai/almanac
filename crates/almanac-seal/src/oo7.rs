//! `Oo7Keys`: Space keys as Secret Service items, through oo7.
//!
//! The logic is in free functions over an [`oo7::Keyring`], so tests run it against oo7's
//! temporary file keyring and never reach the session's real one. [`Oo7Keys`] itself only
//! connects (`Keyring::new`) and delegates; nothing below the daemon calls it.

use crate::keys::SpaceKey;
use crate::store::{KeyError, KeyStore};
use almanac_core::SpaceId;
use oo7::{Item, Keyring};

/// The item schema of a Space key.
pub const SCHEMA: &str = "org.quire.Memory.SpaceKey";

/// The Secret Service store: connects to the session's keyring on each call.
#[derive(Debug, Default)]
pub struct Oo7Keys;

/// The attributes that find one Space's item: `xdg:schema` and `space`.
pub fn attributes(space: &SpaceId) -> [(&'static str, String); 2] {
    [
        ("xdg:schema", SCHEMA.to_owned()),
        ("space", space.as_str().to_owned()),
    ]
}

/// A Secret Service or file backend failure as the store's: a locked collection, or a prompt
/// the person dismissed, is [`KeyError::Locked`].
fn failure(e: oo7::Error) -> KeyError {
    match e {
        oo7::Error::File(oo7::file::Error::Locked)
        | oo7::Error::DBus(oo7::dbus::Error::Dismissed)
        | oo7::Error::DBus(oo7::dbus::Error::Service(oo7::dbus::ServiceError::IsLocked(_))) => {
            KeyError::Locked
        }
        other => KeyError::Store(other.to_string()),
    }
}

async fn found(keyring: &Keyring, space: &SpaceId) -> Result<Option<Item>, KeyError> {
    let items = keyring
        .search_items(&attributes(space))
        .await
        .map_err(failure)?;
    Ok(items.into_iter().next())
}

async fn secret_of(item: &Item) -> Result<SpaceKey, KeyError> {
    if item.is_locked().await.map_err(failure)? {
        item.unlock().await.map_err(failure)?;
    }
    let secret = item.secret().await.map_err(failure)?;
    <[u8; 32]>::try_from(secret.as_bytes())
        .map(SpaceKey::from_bytes)
        .map_err(|_| KeyError::Store("the stored key is not 32 bytes".to_owned()))
}

/// The Space's key in `keyring`.
pub async fn get_in(keyring: &Keyring, space: &SpaceId) -> Result<SpaceKey, KeyError> {
    match found(keyring, space).await? {
        Some(item) => secret_of(&item).await,
        None => Err(KeyError::Missing),
    }
}

/// Files a fresh key for the Space in `keyring`; [`KeyError::Exists`] if one is there.
pub async fn create_in(keyring: &Keyring, space: &SpaceId) -> Result<SpaceKey, KeyError> {
    if found(keyring, space).await?.is_some() {
        return Err(KeyError::Exists);
    }
    let key = SpaceKey::generate().map_err(|e| KeyError::Store(e.to_string()))?;
    keyring
        .create_item(
            &format!("quire memory key for {}", space.as_str()),
            &attributes(space),
            key.expose().as_slice(),
            false,
        )
        .await
        .map_err(failure)?;
    Ok(key)
}

/// Deletes the Space's item from `keyring`; [`KeyError::Missing`] if there is none.
pub async fn destroy_in(keyring: &Keyring, space: &SpaceId) -> Result<(), KeyError> {
    match found(keyring, space).await? {
        Some(item) => item.delete().await.map_err(failure),
        None => Err(KeyError::Missing),
    }
}

async fn connect() -> Result<Keyring, KeyError> {
    Keyring::new().await.map_err(failure)
}

impl KeyStore for Oo7Keys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        get_in(&connect().await?, space).await
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        create_in(&connect().await?, space).await
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        destroy_in(&connect().await?, space).await
    }
}
