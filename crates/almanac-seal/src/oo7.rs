//! `Oo7Keys`: Space keys as Secret Service items, through oo7.

use crate::keys::SpaceKey;
use crate::store::{KeyError, KeyStore};
use almanac_core::SpaceId;

/// The item schema of a Space key.
pub const SCHEMA: &str = "org.quire.Memory.SpaceKey";

/// The Secret Service store. A stub until the daemon is built.
#[derive(Debug, Default)]
pub struct Oo7Keys;

/// The attributes that find one Space's item: `xdg:schema` and `space`.
pub fn attributes(space: &SpaceId) -> [(&'static str, String); 2] {
    [
        ("xdg:schema", SCHEMA.to_owned()),
        ("space", space.as_str().to_owned()),
    ]
}

impl KeyStore for Oo7Keys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        let _ = attributes(space);
        todo!(
            "search the Secret Service by `attributes(space)` through oo7 and decode the 32 bytes"
        )
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        let _ = attributes(space);
        todo!("generate a key and store it as an item with `attributes(space)`; Exists if found")
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        let _ = attributes(space);
        todo!("delete the item with `attributes(space)`")
    }
}
