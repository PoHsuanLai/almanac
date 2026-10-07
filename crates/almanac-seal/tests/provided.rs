//! `ProvidedKeys`: the portable key store. Keys derive from the app's master key, differ by
//! Space and by master, never need a platform service, and seal files like any other store's.

use almanac_core::SpaceId;
use almanac_seal::{Aad, KeyError, KeyStore, Nonce, ProvidedKeys, SpaceKey, seal, unseal};

fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space id")
}

fn master(byte: u8) -> SpaceKey {
    SpaceKey::from_bytes([byte; 32])
}

#[tokio::test]
async fn keys_derive_from_the_master_and_the_space() {
    let keys = ProvidedKeys::new(master(1));
    let work = keys.get(&space("work")).await.expect("get");
    assert_eq!(keys.create(&space("work")).await, Ok(work.clone()));
    assert_eq!(
        ProvidedKeys::new(master(1)).get(&space("work")).await,
        Ok(work.clone()),
        "a later run with the same master finds the same key"
    );
    assert_ne!(keys.get(&space("home")).await, Ok(work.clone()));
    assert_ne!(
        ProvidedKeys::new(master(2)).get(&space("work")).await,
        Ok(work)
    );
}

#[tokio::test]
async fn derivation_is_pinned() {
    let key = ProvidedKeys::new(master(7)).derived(&space("work"));
    let hex = almanac_core::hex_of(key.expose());
    assert_eq!(
        hex, "67e81bf55489d2adf416a05d637b3db1057735ce0dd6678b143f750392b5faf7",
        "update this golden only with a format change"
    );
}

#[tokio::test]
async fn destroy_bars_the_space_in_this_store() {
    let keys = ProvidedKeys::new(master(1));
    assert_eq!(keys.destroy(&space("work")).await, Ok(()));
    assert!(matches!(
        keys.get(&space("work")).await,
        Err(KeyError::Store(_))
    ));
    assert!(matches!(
        keys.create(&space("work")).await,
        Err(KeyError::Store(_))
    ));
    assert!(keys.get(&space("home")).await.is_ok());
}

#[tokio::test]
async fn a_provided_key_seals_and_unseals() {
    let keys = ProvidedKeys::new(master(3));
    let key = keys.get(&space("work")).await.expect("get");
    let sub = almanac_seal::derive(&key, &space("work"), almanac_seal::Purpose::Files);
    let aad = Aad::file(&space("work"), "facts/a.md");
    let sealed = seal(&sub, &aad, b"plain", Nonce([4; 24])).expect("seal");
    assert!(!sealed.ends_with(b"plain"));
    assert_eq!(unseal(&sub, &aad, &sealed).expect("unseal"), b"plain");
    let other = keys.get(&space("home")).await.expect("get");
    let wrong = almanac_seal::derive(&other, &space("work"), almanac_seal::Purpose::Files);
    assert!(unseal(&wrong, &aad, &sealed).is_err());
}
