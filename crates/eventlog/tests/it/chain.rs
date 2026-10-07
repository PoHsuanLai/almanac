//! The hash chain: format, verification, erasure and pruning.

use crate::support::*;
use almanac_core::*;
use eventlog::*;

fn fixed_header() -> Header {
    Header {
        seq: Seq(2),
        replica: ReplicaId([0x11; 16]),
        occurred: UnixSeconds(1_790_000_000),
        recorded: UnixSeconds(1_790_000_005),
        actor: Actor::User {
            via: app("org.quire.Mail"),
        },
        kind: KindTag::parse("thing.archived").expect("kind"),
        effect: Effect::UndoableWrite,
        label: label(Integrity::Trusted),
        cause: Cause::None,
        body_digest: Digest32([0xd1; 32]),
        prev: Link32([0xa0; 32]),
    }
}

#[test]
fn header_bytes_golden() {
    let bytes = header_bytes(&fixed_header());
    assert!(bytes.starts_with(b"QEL1"));
    let golden = include_str!("../golden/header_v1.hex").trim();
    assert_eq!(hex_of(&bytes), golden, "the v1 header encoding is a format");
    assert_eq!(
        hex_of(&link(&fixed_header()).0),
        include_str!("../golden/header_v1.link").trim()
    );
}

#[test]
fn header_bytes_have_the_documented_layout() {
    let h = fixed_header();
    let bytes = header_bytes(&h);
    assert_eq!(&bytes[4..12], &2u64.to_be_bytes());
    assert_eq!(&bytes[12..28], &[0x11; 16]);
    assert_eq!(&bytes[28..36], &1_790_000_000i64.to_be_bytes());
    assert_eq!(&bytes[36..44], &1_790_000_005i64.to_be_bytes());
    assert_eq!(&bytes[bytes.len() - 32..], &[0xa0; 32]);
    assert_eq!(&bytes[bytes.len() - 64..bytes.len() - 32], &[0xd1; 32]);
}

#[test]
fn genesis_depends_on_space_and_replica() {
    let w = space();
    let a = genesis_link(&w, &ReplicaId([1; 16]));
    assert_ne!(a, genesis_link(&w, &ReplicaId([2; 16])));
    assert_ne!(
        a,
        genesis_link(&SpaceId::parse("home").expect("s"), &ReplicaId([1; 16]))
    );
}

#[test]
fn chain_verifies_after_append() {
    let log = filled();
    match verify(&log) {
        ChainReport::Intact { head, erased } => {
            assert_eq!(head.seq, Seq(3));
            assert_eq!(erased, Count(0));
            assert_eq!(log.head().expect("head"), head);
        }
        other => panic!("{other:?}"),
    }
    let empty = new_log();
    assert!(matches!(verify(&empty), ChainReport::Intact { head, .. } if head.seq == Seq(0)));
}

#[test]
fn chain_detects_edit_gap_reorder() {
    let log = filled();
    let from = log.checkpoint().expect("checkpoint");
    let entries = log.scan(Seq(0)).expect("scan");
    let key = digest_key();
    let broken = |entries: &[Entry]| verify_chain(&from, entries, &key);

    let mut edited = entries.clone();
    edited[1].header.occurred = UnixSeconds(1);
    let mut tampered = entries.clone();
    tampered[1].body = BodyState::Present(record("zzzz", Verb::Deleted, user()).body);
    let mut relinked = entries.clone();
    relinked[2].header.prev = Link32([0; 32]);
    let mut wrong_start = entries.clone();
    wrong_start[0].header.prev = Link32([0; 32]);
    let gap = vec![entries[0].clone(), entries[2].clone()];
    let reordered = vec![entries[0].clone(), entries[2].clone(), entries[1].clone()];

    let cases: Vec<(&str, Vec<Entry>, Seq, Break)> = vec![
        ("edited header", edited, Seq(2), Break::LinkMismatch),
        ("tampered body", tampered, Seq(2), Break::BodyDigestMismatch),
        ("wrong prev", relinked, Seq(3), Break::LinkMismatch),
        (
            "first entry off the checkpoint",
            wrong_start,
            Seq(1),
            Break::CheckpointMismatch,
        ),
        ("a missing entry", gap, Seq(3), Break::Gap),
        ("reordered", reordered, Seq(3), Break::Gap),
    ];
    for (name, entries, at, why) in cases {
        assert_eq!(broken(&entries), ChainReport::Broken { at, why }, "{name}");
    }
}

#[test]
fn erasing_bodies_keeps_chain_intact() {
    let mut log = filled();
    put_header_only(&mut log, &record("9c00", Verb::Sent, user()));
    assert!(matches!(
        verify(&log),
        ChainReport::Intact {
            erased: Count(1),
            ..
        }
    ));
    let n = log.erase_bodies(&[Seq(1), Seq(2), Seq(2)]).expect("erase");
    assert_eq!(n, Count(2));
    assert!(matches!(
        verify(&log),
        ChainReport::Intact {
            erased: Count(3),
            ..
        }
    ));
    let all = log.scan(Seq(0)).expect("scan");
    assert_eq!(all[0].body, BodyState::Erased);
    assert!(matches!(all[2].body, BodyState::Present(_)));
    let thing = view("7f3a").thing;
    assert!(
        log.touching(&thing, RoleFilter::Either)
            .expect("touching")
            .is_empty()
    );
}

#[test]
fn prune_prefix_leaves_checkpoint() {
    let mut log = filled();
    let before = match verify(&log) {
        ChainReport::Intact { head, .. } => head,
        other => panic!("{other:?}"),
    };
    let checkpoint = log.prune_before(Seq(2)).expect("prune");
    assert_eq!(checkpoint.cut, Seq(2));
    assert_eq!(log.checkpoint().expect("cp"), checkpoint);
    assert_eq!(log.head().expect("head"), before);
    assert!(matches!(verify(&log), ChainReport::Intact { head, .. } if head == before));
    // Verifying the retained suffix from genesis is a gap: a pruned middle is impossible.
    let genesis = Checkpoint {
        cut: Seq(0),
        link: genesis_link(&space(), &ReplicaId([9; 16])),
    };
    let suffix = log.scan(Seq(0)).expect("scan");
    assert!(matches!(
        verify_chain(&genesis, &suffix, &digest_key()),
        ChainReport::Broken {
            why: Break::Gap,
            ..
        }
    ));
    assert_eq!(log.prune_before(Seq(9)), Err(LogError::NoSuchEntry(Seq(9))));
    // The chain continues past the checkpoint.
    put(&mut log, &record("aaaa", Verb::Created, user()));
    assert!(matches!(verify(&log), ChainReport::Intact { head, .. } if head.seq == Seq(4)));
}

#[test]
fn append_rejects_a_body_that_does_not_match_its_digest() {
    let mut log = new_log();
    let rec = record("7f3a", Verb::Archived, user());
    let header = NewHeader::of(&rec, NOW, &digest_key());
    let other = record("other", Verb::Deleted, user()).body;
    assert_eq!(log.append(header, Some(other)), Err(LogError::BadDigest));
}

#[test]
fn the_digest_is_keyed() {
    let rec = record("7f3a", Verb::Archived, user());
    let other_key = almanac_seal::derive(
        &almanac_seal::SpaceKey::from_bytes([8; 32]),
        &space(),
        almanac_seal::Purpose::Digest,
    );
    assert_ne!(
        body_digest(&digest_key(), &rec.body),
        body_digest(&other_key, &rec.body)
    );
}
