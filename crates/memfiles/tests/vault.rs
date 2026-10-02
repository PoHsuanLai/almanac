//! The vault contract, and the path grammar.

use almanac_core::{FactId, TopicPath};
use memfiles::*;

fn p(text: &str) -> VaultPath {
    VaultPath::parse(text).expect("path")
}

#[test]
fn vault_paths_follow_their_grammar() {
    for ok in [
        "facts/a.md",
        "a",
        "pending/01j9zk3m0q8h2v6x4c1b7n5t2a.md",
        "facts/people/sam-lee.md",
    ] {
        assert!(VaultPath::parse(ok).is_some(), "{ok}");
    }
    for bad in [
        "", "/a", "a/", "a//b", "../a", "a/../b", "./a", "a\\b", "a\nb",
    ] {
        assert!(VaultPath::parse(bad).is_none(), "{bad:?}");
    }
    assert!(VaultPath::parse(&"a".repeat(513)).is_none());
}

#[test]
fn layout_paths_are_pinned() {
    let topic = TopicPath::parse("people/sam-lee").expect("topic");
    let id = FactId::parse("01j9zk3m0q8h2v6x4c1b7n5t2a").expect("id");
    assert_eq!(VaultPath::topic(&topic).as_str(), "facts/people/sam-lee.md");
    assert_eq!(
        VaultPath::pending(&id).as_str(),
        "pending/01j9zk3m0q8h2v6x4c1b7n5t2a.md"
    );
    assert_eq!(VaultPath::primer().as_str(), "facts/INDEX.md");
    assert!(p("facts/a/b.md").is_under(&VaultPath::facts_dir()));
    assert!(!p("factsx/a.md").is_under(&VaultPath::facts_dir()));
}

/// Every `Vault` must pass this: `MemoryVault` now; `PlainDir` and `SealedDir` at the fill.
fn contract(vault: &impl Vault) {
    assert_eq!(
        vault.read(&p("facts/a.md")),
        Err(VaultError::NotFound(p("facts/a.md")))
    );
    vault.write_atomic(&p("facts/a.md"), b"one").expect("write");
    vault
        .write_atomic(&p("facts/people/b.md"), b"two")
        .expect("write");
    vault
        .write_atomic(&p("pending/c.md"), b"three")
        .expect("write");
    assert_eq!(vault.read(&p("facts/a.md")).expect("read"), b"one");
    vault
        .write_atomic(&p("facts/a.md"), b"replaced")
        .expect("rewrite");
    assert_eq!(vault.read(&p("facts/a.md")).expect("read"), b"replaced");
    assert_eq!(
        vault.list(&p("facts")).expect("list"),
        vec![p("facts/a.md"), p("facts/people/b.md")]
    );
    assert_eq!(
        vault.list(&p("pending")).expect("list"),
        vec![p("pending/c.md")]
    );
    assert!(vault.list(&p("nothing")).expect("list").is_empty());
    vault.remove(&p("facts/a.md")).expect("remove");
    assert_eq!(
        vault.remove(&p("facts/a.md")),
        Err(VaultError::NotFound(p("facts/a.md")))
    );
    assert_eq!(
        vault.list(&p("facts")).expect("list"),
        vec![p("facts/people/b.md")]
    );
}

#[test]
fn vault_contract_memory() {
    contract(&MemoryVault::new());
}

#[test]
#[ignore = "PlainDir is a todo!() until the memfiles fill (FINDINGS.md)"]
fn vault_contract_plain_dir() {
    let dir = std::env::temp_dir().join("almanac-memfiles-plain");
    contract(&PlainDir::new(dir));
}

#[test]
#[ignore = "SealedDir is a todo!() until the memfiles fill (FINDINGS.md)"]
fn vault_contract_sealed_dir() {
    use almanac_seal::{Purpose, SpaceKey, derive};
    let space = almanac_core::SpaceId::parse("work").expect("space");
    let key = derive(&SpaceKey::from_bytes([1; 32]), &space, Purpose::Files);
    let dir = std::env::temp_dir().join("almanac-memfiles-sealed");
    contract(&SealedDir::new(PlainDir::new(dir), space, key));
}

#[test]
fn the_primer_is_capped_and_linked() {
    let entry = |n: usize| PrimerEntry {
        topic: TopicPath::parse(&format!("t{n}")).expect("topic"),
        title: format!("Topic {n}").into(),
        summary: "about it".into(),
    };
    let primer = Primer {
        entries: (0..300).map(entry).collect(),
    };
    let text = primer.render();
    assert_eq!(text.lines().count(), PRIMER_MAX_LINES);
    assert!(text.starts_with("# Memory\n\n- [Topic 0](t0.md): about it\n"));
}
