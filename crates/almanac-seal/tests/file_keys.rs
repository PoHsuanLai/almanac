//! `FileKeys` (feature `test-keys`): a sealed file of keys that a second store over the same path
//! shares, that holds no plain key, and that a copy of another store's file cannot be forged into.
#![cfg(feature = "test-keys")]

use almanac_core::{SpaceId, hex_of};
use almanac_seal::{FileKeys, KeyError, KeyStore};

fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space id")
}

#[tokio::test]
async fn keys_are_made_found_shared_and_destroyed() {
    let dir = tempfile::tempdir().expect("scratch");
    let path = dir.path().join("keys").join("memoryd.keys");
    let first = FileKeys::at(&path);
    assert_eq!(first.get(&space("work")).await, Err(KeyError::Missing));
    let made = first.create(&space("work")).await.expect("create");
    assert_eq!(first.create(&space("work")).await, Err(KeyError::Exists));
    // A later run over the same file finds the key, as successive runs find the keyring's.
    let second = FileKeys::at(&path);
    assert_eq!(second.get(&space("work")).await.expect("get"), made);
    assert_eq!(second.destroy(&space("work")).await, Ok(()));
    assert_eq!(first.get(&space("work")).await, Err(KeyError::Missing));
    assert_eq!(second.destroy(&space("work")).await, Err(KeyError::Missing));
}

#[tokio::test]
async fn the_file_holds_no_plain_key_and_a_damaged_one_is_a_store_error() {
    let dir = tempfile::tempdir().expect("scratch");
    let path = dir.path().join("memoryd.keys");
    let keys = FileKeys::at(&path);
    let made = keys.create(&space("desktop")).await.expect("create");
    let bytes = std::fs::read(&path).expect("file");
    let hex = hex_of(made.expose());
    assert!(bytes.starts_with(b"QMEM"));
    assert!(!bytes.windows(hex.len()).any(|w| w == hex.as_bytes()));
    std::fs::write(&path, b"not sealed").expect("damage");
    assert!(matches!(
        keys.get(&space("desktop")).await,
        Err(KeyError::Store(_))
    ));
}
