//! The fact store over a vault: add-only appends, the pending area, settling, the strict
//! cascade and journal rollups. Every test runs over memory and over a scratch directory.

use almanac_core::*;
use jiff::tz::TimeZone;
use memfiles::*;
use std::collections::BTreeSet;

fn id(n: u32) -> FactId {
    FactId::parse(&format!("01j9zk3m0q8h2v6x4c1b7n{n:04}")).expect("id")
}

fn topic(text: &str) -> TopicPath {
    TopicPath::parse(text).expect("topic")
}

fn thread(key: &str) -> Link {
    Link::Thing(ThingRef {
        app: AppName::parse("org.quire.Mail").expect("app"),
        kind: ThingKind::parse("mail.thread").expect("kind"),
        key: ThingKey::parse(key).expect("key"),
    })
}

fn untrusted() -> Label {
    Label {
        integrity: Integrity::Untrusted,
        confidentiality: Confidentiality::Private(BTreeSet::from([
            SpaceId::parse("work").expect("space")
        ])),
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::Mail]),
    }
}

fn fact(n: u32, links: Vec<Link>) -> Fact {
    Fact {
        id: id(n),
        text: FactText::parse(&format!("Fact number {n}.")).expect("text"),
        recorded: UnixSeconds(1_790_845_964 + i64::from(n)),
        by: Actor::Unknown,
        label: untrusted(),
        links,
        supersedes: vec![],
        valid: Validity::Unstated,
    }
}

fn receipt() -> ConfirmReceipt {
    ConfirmReceipt {
        id: ConfirmId::parse("01j9zk3m0q8h2v6x4c1b7ncnf0").expect("confirm id"),
        input: InputProof::ShellCaller,
        at: UnixSeconds(1_790_900_000),
        // A keep endorses; it opens nothing.
        covers: Confidentiality::Secret,
    }
}

fn store_over<V: Vault>(vault: V) -> Store<V> {
    Store::new(vault, SpaceId::parse("work").expect("space"), TimeZone::UTC)
}

/// Either vault behind one type.
struct Either(Box<dyn Vault>);

impl Vault for Either {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError> {
        self.0.list(dir)
    }
    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError> {
        self.0.read(p)
    }
    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError> {
        self.0.write_atomic(p, bytes)
    }
    fn remove(&self, p: &VaultPath) -> Result<(), VaultError> {
        self.0.remove(p)
    }
}

/// Runs `check` over a memory vault and a scratch directory.
fn on_every_vault(check: impl Fn(&Store<Either>)) {
    check(&store_over(Either(Box::new(MemoryVault::new()))));
    let dir = tempfile::tempdir().expect("scratch");
    let plain = PlainDir::new(dir.path().to_owned());
    check(&store_over(Either(Box::new(plain))));
}

#[test]
fn append_is_add_only() {
    on_every_vault(|store| {
        let t = topic("people/sam-lee");
        assert_eq!(store.read(&t), Err(MemfilesError::NoSuchTopic(t.clone())));
        store.append(&t, fact(1, vec![thread("a")])).expect("first");
        let path = VaultPath::topic(&t);
        let before = store.vault().read(&path).expect("bytes");
        store
            .append(&t, fact(2, vec![thread("a")]))
            .expect("second");
        let after = store.vault().read(&path).expect("bytes");
        assert!(after.starts_with(&before), "earlier bytes are untouched");
        let file = store.read(&t).expect("read");
        assert_eq!(file.title, "sam-lee");
        let ids: Vec<_> = file
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Fact(f) => Some(f.id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, [id(1), id(2)]);
        assert_eq!(store.topics().expect("topics"), [t]);
    });
}

#[test]
fn a_persons_edits_survive_an_append() {
    on_every_vault(|store| {
        let t = topic("prefs");
        let text = "---\nformat: quire-memory 1\ntopic: prefs\ntitle: Prefs\n---\n\n- A bullet I typed.\nA note.\n";
        store
            .vault()
            .write_atomic(&VaultPath::topic(&t), text.as_bytes())
            .expect("seed");
        store.append(&t, fact(1, vec![])).expect("append");
        let blocks = store.read(&t).expect("read").blocks;
        assert_eq!(blocks[0], Block::Unstamped("A bullet I typed.".into()));
        assert_eq!(blocks[1], Block::Verbatim("A note.".into()));
        assert!(matches!(blocks[2], Block::Fact(_)));
    });
}

#[test]
fn untrusted_fact_lands_in_pending() {
    on_every_vault(|store| {
        let t = topic("people/sam-lee");
        store
            .stage(fact(3, vec![thread("a")]), t.clone())
            .expect("stage");
        assert_eq!(store.topics().expect("topics"), []);
        assert_eq!(store.read(&t), Err(MemfilesError::NoSuchTopic(t.clone())));
        let waiting = store.pending().expect("pending");
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].0, t);
        assert_eq!(waiting[0].1, fact(3, vec![thread("a")]));
        assert!(store.vault().read(&VaultPath::pending(&id(3))).is_ok());
    });
}

#[test]
fn confirm_requires_witness() {
    on_every_vault(|store| {
        let t = topic("people/sam-lee");
        store
            .stage(fact(3, vec![thread("a")]), t.clone())
            .expect("stage");
        store
            .stage(fact(4, vec![thread("a")]), t.clone())
            .expect("stage");
        store
            .settle(&id(3), Settlement::Keep(receipt()))
            .expect("keep");
        // Kept: in its topic, trusted with the person among its sources, no longer pending.
        let Block::Fact(kept) = &store.read(&t).expect("topic").blocks[0] else {
            panic!("a fact")
        };
        assert_eq!(kept.id, id(3));
        assert_eq!(kept.label.integrity, Integrity::Trusted);
        assert!(kept.label.sources.contains(&Source::User));
        assert!(kept.label.sources.contains(&Source::Mail));
        assert_eq!(kept.label.confidentiality, untrusted().confidentiality);
        assert_eq!(store.pending().expect("pending").len(), 1);
        // Discarded: gone from both.
        store.settle(&id(4), Settlement::Discard).expect("discard");
        assert_eq!(store.pending().expect("pending"), []);
        assert_eq!(store.read(&t).expect("topic").blocks.len(), 1);
        // Not pending any more.
        assert_eq!(
            store.settle(&id(3), Settlement::Discard),
            Err(MemfilesError::NotPending(id(3)))
        );
        assert_eq!(
            store.settle(&id(9), Settlement::Keep(receipt())),
            Err(MemfilesError::NotPending(id(9)))
        );
    });
}

#[test]
fn remove_cascades_through_topics_and_pending() {
    on_every_vault(|store| {
        let t = topic("people/sam-lee");
        store.append(&t, fact(1, vec![thread("a")])).expect("1");
        store.append(&t, fact(2, vec![thread("c")])).expect("2");
        store
            .stage(fact(3, vec![Link::Fact(id(1))]), topic("prefs"))
            .expect("3");
        let doomed = [id(1), id(3)];
        assert_eq!(store.remove(&doomed), Ok(Count(2)));
        let left = store.read(&t).expect("topic");
        assert_eq!(left.blocks.len(), 1);
        assert!(matches!(&left.blocks[0], Block::Fact(f) if f.id == id(2)));
        assert_eq!(store.pending().expect("pending"), []);
        assert!(store.vault().read(&VaultPath::pending(&id(3))).is_err());
        // Removing what is not there removes nothing.
        assert_eq!(store.remove(&doomed), Ok(Count(0)));
    });
}

#[test]
fn journal_rollups_are_plain_topics() {
    on_every_vault(|store| {
        let day = topic("journal/2026-10-03");
        let week = topic("journal/2026-w40");
        store.append(&day, fact(1, vec![thread("a")])).expect("day");
        store
            .append(&week, fact(2, vec![Link::Fact(id(1))]))
            .expect("week");
        let mut topics = store.topics().expect("topics");
        topics.sort();
        assert_eq!(topics, [day.clone(), week]);
        assert!(store.vault().read(&VaultPath::topic(&day)).is_ok());
        // A rollup is removed like any other fact.
        assert_eq!(store.remove(&[id(1), id(2)]), Ok(Count(2)));
    });
}

#[test]
fn the_primer_is_not_a_topic() {
    on_every_vault(|store| {
        store
            .append(&topic("prefs"), fact(1, vec![]))
            .expect("append");
        let primer = Primer {
            entries: vec![PrimerEntry {
                topic: topic("prefs"),
                title: "Prefs".into(),
                summary: "what I like".into(),
            }],
        };
        store
            .vault()
            .write_atomic(&VaultPath::primer(), primer.render().as_bytes())
            .expect("primer");
        assert_eq!(
            store.vault().read(&VaultPath::primer()).expect("bytes"),
            primer.render().into_bytes()
        );
        assert_eq!(store.topics().expect("topics"), [topic("prefs")]);
        // Scans skip it rather than failing to parse it.
        assert_eq!(store.remove(&[id(1)]), Ok(Count(1)));
    });
}

#[test]
fn a_corrupt_file_is_named_not_swallowed() {
    on_every_vault(|store| {
        let t = topic("prefs");
        store
            .vault()
            .write_atomic(&VaultPath::topic(&t), b"not a topic file")
            .expect("seed");
        assert!(matches!(
            store.read(&t),
            Err(MemfilesError::Parse {
                err: ParseError::MissingFrontMatter,
                ..
            })
        ));
    });
}

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn appended_facts_read_back_in_order(count in 1u32..12, split in 0usize..3) {
            let dir = tempfile::tempdir().expect("scratch");
            let plain = PlainDir::new(dir.path().to_owned());
            let store = store_over(plain);
            let topics = [topic("a"), topic("b/c"), topic("journal/2026-w40")];
            for n in 0..count {
                let t = &topics[(n as usize + split) % topics.len()];
                store.append(t, fact(n, vec![thread("k")])).expect("append");
            }
            let mut ids = Vec::new();
            for t in &topics {
                if let Ok(file) = store.read(t) {
                    for block in file.blocks {
                        if let Block::Fact(f) = block { ids.push(f.id); }
                    }
                }
            }
            ids.sort();
            prop_assert_eq!(&ids, &(0..count).map(id).collect::<Vec<_>>());
            prop_assert_eq!(store.remove(&ids), Ok(Count(count)));
        }
    }
}
