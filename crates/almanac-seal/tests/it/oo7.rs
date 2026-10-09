//! `Oo7Keys`' logic against oo7's temporary file keyring: nothing here reaches the session's
//! Secret Service or any keyring on disk.

use almanac_core::SpaceId;
use almanac_seal::{KeyError, SCHEMA, attributes, create_in, destroy_in, get_in};
use oo7::{Keyring, Secret, file};
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock};

fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space")
}

async fn unlocked() -> Keyring {
    let temporary = file::UnlockedKeyring::temporary(Secret::text("scratch-master"))
        .await
        .expect("temporary keyring");
    Keyring::File(Arc::new(RwLock::new(Some(file::Keyring::Unlocked(
        temporary,
    )))))
}

static SHARED: OnceCell<Keyring> = OnceCell::const_new();

/// One unlocked keyring for every test that can keep to Spaces of its own (each pays the
/// password KDF once, not once per test). A test that needs an empty keyring or its own lock
/// state builds one with `unlocked` or `locked` instead.
async fn shared() -> &'static Keyring {
    SHARED.get_or_init(unlocked).await
}

async fn locked() -> Keyring {
    let temporary = file::UnlockedKeyring::temporary(Secret::text("scratch-master"))
        .await
        .expect("temporary keyring");
    Keyring::File(Arc::new(RwLock::new(Some(file::Keyring::Locked(
        temporary.lock(),
    )))))
}

/// One Space walks the whole script: missing, create, get, create again, destroy, destroy again,
/// create again.
#[tokio::test]
async fn a_key_is_missing_made_found_kept_destroyed_and_made_again() {
    let ring = shared().await;
    let work = space("script");
    assert_eq!(
        get_in(ring, &work).await.unwrap_err(),
        KeyError::Missing,
        "get before create"
    );
    let made = create_in(ring, &work).await.expect("create");
    assert_eq!(
        get_in(ring, &work).await.expect("get"),
        made,
        "get returns the created key"
    );
    assert_eq!(
        create_in(ring, &work).await.unwrap_err(),
        KeyError::Exists,
        "create twice"
    );
    assert_eq!(
        get_in(ring, &work).await.expect("get"),
        made,
        "a second create keeps the first key"
    );
    destroy_in(ring, &work).await.expect("destroy");
    assert_eq!(
        get_in(ring, &work).await.unwrap_err(),
        KeyError::Missing,
        "get after destroy"
    );
    assert_eq!(
        destroy_in(ring, &work).await.unwrap_err(),
        KeyError::Missing,
        "a second destroy"
    );
    create_in(ring, &work)
        .await
        .expect("a destroyed Space can be created again");
}

#[tokio::test]
async fn spaces_have_their_own_keys() {
    let ring = shared().await;
    let work = create_in(ring, &space("own-work")).await.expect("create");
    let home = create_in(ring, &space("own-home")).await.expect("create");
    assert_ne!(work, home);
    destroy_in(ring, &space("own-home")).await.expect("destroy");
    assert_eq!(get_in(ring, &space("own-work")).await.expect("get"), work);
}

#[tokio::test]
async fn the_item_carries_the_schema_and_space_attributes() {
    let ring = shared().await;
    let attrs = space("attrs");
    create_in(ring, &attrs).await.expect("create");
    let items = ring.search_items(&attributes(&attrs)).await.expect("items");
    assert_eq!(items.len(), 1);
    let found = items[0].attributes().await.expect("attributes");
    assert_eq!(found.get("xdg:schema").map(String::as_str), Some(SCHEMA));
    assert_eq!(found.get("space").map(String::as_str), Some("attrs"));
}

#[tokio::test]
async fn a_locked_keyring_is_locked() {
    let ring = locked().await;
    assert_eq!(
        get_in(&ring, &space("work")).await.unwrap_err(),
        KeyError::Locked
    );
    assert_eq!(
        create_in(&ring, &space("work")).await.unwrap_err(),
        KeyError::Locked
    );
    assert_eq!(
        destroy_in(&ring, &space("work")).await.unwrap_err(),
        KeyError::Locked
    );
}

#[tokio::test]
async fn a_secret_that_is_not_32_bytes_is_a_store_error() {
    let ring = shared().await;
    ring.create_item("short", &attributes(&space("short")), "five!!", false)
        .await
        .expect("plant");
    assert!(matches!(
        get_in(ring, &space("short")).await,
        Err(KeyError::Store(_))
    ));
}
