//! Sealing, derivation and the key store.

use almanac_core::SpaceId;
use almanac_seal::*;

fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space id")
}

fn key() -> SpaceKey {
    SpaceKey::from_bytes([7; 32])
}

#[test]
fn seal_round_trip() {
    let sub = derive(&key(), &space("work"), Purpose::Files);
    let aad = Aad::file(&space("work"), "facts/people/sam-lee.md");
    let plain = b"## 2026-10-01\n\n- Prefers meetings after 10:00.\n";
    let sealed = seal(&sub, &aad, plain, Nonce([1; 24])).expect("seal");
    assert!(sealed.starts_with(b"QMEM\x01"));
    assert_eq!(&sealed[5..29], &[1u8; 24]);
    assert_ne!(&sealed[29..], plain.as_slice());
    assert_eq!(unseal(&sub, &aad, &sealed).expect("unseal"), plain);
}

#[test]
fn unseal_rejects_moved_file() {
    let sub = derive(&key(), &space("work"), Purpose::Files);
    let sealed = seal(
        &sub,
        &Aad::file(&space("work"), "facts/a.md"),
        b"x",
        Nonce([2; 24]),
    )
    .expect("seal");
    let moved_path = Aad::file(&space("work"), "facts/b.md");
    let moved_space = Aad::file(&space("home"), "facts/a.md");
    assert_eq!(
        unseal(&sub, &moved_path, &sealed),
        Err(SealError::Unauthentic)
    );
    assert_eq!(
        unseal(&sub, &moved_space, &sealed),
        Err(SealError::Unauthentic)
    );
    let other = derive(&key(), &space("home"), Purpose::Files);
    assert_eq!(
        unseal(&other, &Aad::file(&space("work"), "facts/a.md"), &sealed),
        Err(SealError::Unauthentic)
    );
}

#[test]
fn unseal_reports_each_malformation() {
    let sub = derive(&key(), &space("work"), Purpose::Files);
    let aad = Aad::file(&space("work"), "a");
    let good = seal(&sub, &aad, b"hello", Nonce([3; 24])).expect("seal");
    let mut unknown = good.clone();
    unknown[4] = 9;
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    let cases: Vec<(&str, Vec<u8>, SealError)> = vec![
        ("empty", vec![], SealError::Truncated),
        ("half a magic", b"QM".to_vec(), SealError::Truncated),
        (
            "not sealed",
            b"---\nformat: quire-memory 1\n".to_vec(),
            SealError::BadMagic,
        ),
        ("magic only", b"QMEM".to_vec(), SealError::Truncated),
        ("short body", good[..30].to_vec(), SealError::Truncated),
        ("future version", unknown, SealError::UnknownVersion(9)),
        ("flipped bit", flipped, SealError::Unauthentic),
    ];
    for (name, bytes, want) in cases {
        assert_eq!(unseal(&sub, &aad, &bytes), Err(want), "{name}");
    }
}

#[test]
fn derive_is_stable_golden() {
    let spaces = [space("work"), space("home")];
    let mut lines = Vec::new();
    for s in &spaces {
        for purpose in [
            Purpose::Eventlog,
            Purpose::Index,
            Purpose::Files,
            Purpose::Digest,
        ] {
            let sub = derive(&key(), s, purpose);
            lines.push(format!(
                "{s} {} {}",
                purpose.context(),
                almanac_core::hex_of(sub.expose())
            ));
        }
    }
    let golden = include_str!("golden/derive.txt");
    assert_eq!(
        lines.join("\n") + "\n",
        golden,
        "derivation changed; the contexts are a format"
    );
}

#[test]
fn purposes_never_share_a_subkey() {
    let all = [
        Purpose::Eventlog,
        Purpose::Index,
        Purpose::Files,
        Purpose::Digest,
    ];
    let keys: Vec<_> = all
        .iter()
        .map(|p| *derive(&key(), &space("work"), *p).expose())
        .collect();
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn space_key_debug_is_redacted() {
    let k = SpaceKey::from_bytes([0xAB; 32]);
    let sub = derive(&k, &space("work"), Purpose::Eventlog);
    let db = DbKey::of(&sub);
    for shown in [format!("{k:?}"), format!("{sub:?}"), format!("{db:?}")] {
        assert!(shown.contains("redacted"), "{shown}");
        assert!(!shown.to_lowercase().contains("abab"), "{shown}");
    }
}

#[test]
fn db_key_is_sqlcipher_raw_form() {
    let sub = derive(&key(), &space("work"), Purpose::Eventlog);
    let pragma = DbKey::of(&sub).pragma();
    assert!(pragma.starts_with("x'") && pragma.ends_with('\''));
    assert_eq!(pragma.len(), 2 + 64 + 1);
}

#[tokio::test]
async fn memory_keys_create_get_destroy_and_lock() {
    let store = MemoryKeys::new();
    let work = space("work");
    assert_eq!(store.get(&work).await, Err(KeyError::Missing));
    let made = store.create(&work).await.expect("create");
    assert_eq!(store.get(&work).await.expect("get"), made);
    assert_eq!(store.create(&work).await, Err(KeyError::Exists));
    store.lock();
    assert_eq!(store.get(&work).await, Err(KeyError::Locked));
    store.unlock();
    store.destroy(&work).await.expect("destroy");
    assert_eq!(store.get(&work).await, Err(KeyError::Missing));
}

#[test]
fn oo7_items_are_found_by_schema_and_space() {
    let attrs = oo7_attributes();
    assert_eq!(
        attrs[0],
        ("xdg:schema", "org.quire.Memory.SpaceKey".to_owned())
    );
    assert_eq!(attrs[1], ("space", "work".to_owned()));
}

fn oo7_attributes() -> [(&'static str, String); 2] {
    almanac_seal::attributes(&space("work"))
}
