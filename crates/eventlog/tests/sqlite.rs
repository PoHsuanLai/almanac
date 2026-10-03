//! `SqliteLog` beyond the shared contract: persistence, the chain and audit over the file,
//! erasure and pruning, and the pragmas. Every file lives in a scratch directory.

mod common;

use almanac_core::*;
use almanac_seal::{DbKey, Purpose, SpaceKey, derive};
use common::*;
use eventlog::*;
use std::path::PathBuf;

fn key() -> DbKey {
    DbKey::of(&derive(
        &SpaceKey::from_bytes([7; 32]),
        &space(),
        Purpose::Eventlog,
    ))
}

fn open(dir: &tempfile::TempDir) -> SqliteLog {
    SqliteLog::open_for(
        &path(dir),
        &key(),
        &space(),
        ReplicaId([9; 16]),
        &digest_key(),
    )
    .expect("open")
}

fn path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("events.db")
}

fn fill(log: &mut SqliteLog) -> Vec<Entry> {
    [
        record("7f3a", Verb::Archived, user()),
        record("7f3a", Verb::Viewed, Actor::Unknown),
        record("8b21", Verb::Archived, user()),
    ]
    .iter()
    .map(|rec| {
        let header = NewHeader::of(rec, UnixSeconds(NOW.0 + 1), &digest_key());
        log.append(header, Some(rec.body.clone())).expect("append")
    })
    .collect()
}

fn audit(log: &SqliteLog) -> ChainReport {
    log.audit(&digest_key()).expect("audit")
}

#[test]
fn sqlite_entries_equal_memory_entries() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut sql = open(&dir);
    let mem = filled();
    let from_sql = fill(&mut sql);
    let from_mem = mem.scan(Seq(1)).expect("scan");
    assert_eq!(from_sql, from_mem);
    assert_eq!(sql.scan(Seq(1)).expect("scan"), from_mem);
    assert_eq!(sql.head().expect("head"), mem.head().expect("head"));
    assert_eq!(sql.checkpoint().expect("cp"), mem.checkpoint().expect("cp"));
}

#[test]
fn an_empty_log_has_the_genesis_head() {
    let dir = tempfile::tempdir().expect("scratch");
    let log = open(&dir);
    let genesis = genesis_link(&space(), &ReplicaId([9; 16]));
    assert_eq!(
        log.head().expect("head"),
        Head {
            seq: Seq(0),
            link: genesis
        }
    );
    assert_eq!(
        audit(&log),
        ChainReport::Intact {
            head: Head {
                seq: Seq(0),
                link: genesis
            },
            erased: Count(0)
        }
    );
}

#[test]
fn the_log_persists_across_reopen_and_keeps_its_replica() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    let written = fill(&mut log);
    drop(log);
    let again = SqliteLog::open_for(
        &path(&dir),
        &key(),
        &space(),
        ReplicaId([1; 16]),
        &digest_key(),
    )
    .expect("reopen");
    assert_eq!(again.replica(), ReplicaId([9; 16]));
    assert_eq!(again.scan(Seq(1)).expect("scan"), written);
    assert!(matches!(audit(&again), ChainReport::Intact { .. }));
}

#[test]
fn append_after_reopen_continues_the_chain() {
    let dir = tempfile::tempdir().expect("scratch");
    drop({
        let mut log = open(&dir);
        fill(&mut log);
        log
    });
    let mut log = open(&dir);
    let rec = record("9c00", Verb::Archived, user());
    let entry = log
        .append(
            NewHeader::of(&rec, NOW, &digest_key()),
            Some(rec.body.clone()),
        )
        .expect("append");
    assert_eq!(entry.header.seq, Seq(4));
    assert!(matches!(
        audit(&log),
        ChainReport::Intact { head, .. } if head.seq == Seq(4)
    ));
}

#[test]
fn header_only_entries_are_erased_and_have_no_things() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    let rec = record("7f3a", Verb::Archived, user());
    let entry = log
        .append(NewHeader::of(&rec, NOW, &digest_key()), None)
        .expect("append");
    assert_eq!(entry.body, BodyState::Erased);
    assert!(
        log.touching(&view("7f3a").thing, RoleFilter::Either)
            .expect("touching")
            .is_empty()
    );
    assert_eq!(
        audit(&log),
        ChainReport::Intact {
            head: log.head().expect("head"),
            erased: Count(1)
        }
    );
}

#[test]
fn erasing_bodies_keeps_the_chain_intact_and_drops_things() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    fill(&mut log);
    assert_eq!(erased_of(&log), Count(0));
    assert_eq!(
        log.erase_bodies(&[Seq(1), Seq(2), Seq(2)]).expect("erase"),
        Count(2)
    );
    assert_eq!(erased_of(&log), Count(2));
    let scanned = log.scan(Seq(1)).expect("scan");
    assert_eq!(scanned[0].body, BodyState::Erased);
    assert!(matches!(scanned[2].body, BodyState::Present(_)));
    assert!(
        log.touching(&view("7f3a").thing, RoleFilter::Either)
            .expect("touching")
            .is_empty()
    );
    assert_eq!(
        log.touching(&view("8b21").thing, RoleFilter::Subject)
            .expect("touching"),
        vec![Seq(3)]
    );
}

fn erased_of(log: &SqliteLog) -> Count {
    match audit(log) {
        ChainReport::Intact { erased, .. } => erased,
        broken @ ChainReport::Broken { .. } => panic!("{broken:?}"),
    }
}

#[test]
fn erasing_leaves_no_body_text_and_truncates_the_wal() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    fill(&mut log);
    log.erase_bodies(&[Seq(1), Seq(2), Seq(3)]).expect("erase");
    let wal = std::fs::metadata(dir.path().join("events.db-wal")).map_or(0, |m| m.len());
    assert_eq!(wal, 0);
    let left: i64 = log
        .connection()
        .query_row("SELECT count(*) FROM bodies", [], |r| r.get(0))
        .expect("count");
    assert_eq!(left, 0);
}

#[test]
fn pragmas_are_secure_delete_and_wal() {
    let dir = tempfile::tempdir().expect("scratch");
    let log = open(&dir);
    let conn = log.connection();
    let secure: i64 = conn
        .query_row("PRAGMA secure_delete", [], |r| r.get(0))
        .expect("pragma");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("pragma");
    assert_eq!((secure, mode.as_str()), (1, "wal"));
}

#[test]
fn the_file_is_not_plain_sqlite() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    fill(&mut log);
    drop(log);
    let bytes = std::fs::read(path(&dir)).expect("read");
    assert!(!bytes.starts_with(b"SQLite format 3"));
    assert!(!bytes.windows(b"Q4 budget".len()).any(|w| w == b"Q4 budget"));
}

#[test]
fn pruning_leaves_a_checkpoint_that_verifies() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    let written = fill(&mut log);
    let cp = log.prune_before(Seq(2)).expect("prune");
    assert_eq!(
        cp,
        Checkpoint {
            cut: Seq(2),
            link: written[1].link
        }
    );
    assert_eq!(log.checkpoint().expect("cp"), cp);
    assert_eq!(log.scan(Seq(0)).expect("scan"), vec![written[2].clone()]);
    assert_eq!(
        log.touching(&view("7f3a").thing, RoleFilter::Either)
            .expect("touching"),
        Vec::<Seq>::new()
    );
    assert!(matches!(
        audit(&log),
        ChainReport::Intact { head, .. } if head.seq == Seq(3)
    ));
    assert_eq!(log.prune_before(Seq(1)), Err(LogError::NoSuchEntry(Seq(1))));
}

#[test]
fn pruning_everything_leaves_the_checkpoint_as_head() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    let written = fill(&mut log);
    let cp = log.prune_before(Seq(3)).expect("prune");
    assert_eq!(
        log.head().expect("head"),
        Head {
            seq: Seq(3),
            link: written[2].link
        }
    );
    let rec = record("9c00", Verb::Archived, user());
    let next = log
        .append(NewHeader::of(&rec, NOW, &digest_key()), Some(rec.body))
        .expect("append");
    assert_eq!((next.header.seq, next.header.prev), (Seq(4), cp.link));
    assert!(matches!(audit(&log), ChainReport::Intact { .. }));
}

#[test]
fn audit_finds_an_edited_header_a_gap_and_a_forged_body() {
    let edit = |sql: &str| {
        let dir = tempfile::tempdir().expect("scratch");
        let mut log = open(&dir);
        fill(&mut log);
        log.connection().execute_batch(sql).expect("tamper");
        audit(&log)
    };
    assert_eq!(
        edit("UPDATE events SET recorded = recorded + 1 WHERE seq = 2"),
        ChainReport::Broken {
            at: Seq(2),
            why: Break::LinkMismatch
        }
    );
    assert_eq!(
        edit("PRAGMA foreign_keys = OFF; DELETE FROM events WHERE seq = 2"),
        ChainReport::Broken {
            at: Seq(3),
            why: Break::Gap
        }
    );
    let body = serde_json::to_string(&record("0000", Verb::Archived, user()).body).expect("json");
    assert_eq!(
        edit(&format!("UPDATE bodies SET json = '{body}' WHERE seq = 3")),
        ChainReport::Broken {
            at: Seq(3),
            why: Break::BodyDigestMismatch
        }
    );
}

#[test]
fn an_unknown_schema_is_refused() {
    let dir = tempfile::tempdir().expect("scratch");
    drop(open(&dir));
    {
        let conn = rusqlite::Connection::open(path(&dir)).expect("open");
        conn.pragma_update(None, "key", key().pragma())
            .expect("key");
        conn.pragma_update(None, "user_version", 9)
            .expect("version");
    }
    assert!(matches!(
        SqliteLog::open(&path(&dir), &key()),
        Err(LogError::Schema { found: 9 })
    ));
}

#[test]
fn a_plain_sqlite_file_is_locked_to_a_keyed_open() {
    let dir = tempfile::tempdir().expect("scratch");
    {
        let conn = rusqlite::Connection::open(path(&dir)).expect("open");
        conn.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (1);")
            .expect("plain");
    }
    assert!(matches!(
        SqliteLog::open(&path(&dir), &key()),
        Err(LogError::Locked)
    ));
}

#[test]
fn an_unbound_open_creates_the_directory_and_works() {
    let dir = tempfile::tempdir().expect("scratch");
    let nested = dir.path().join("a").join("events.db");
    let mut log = SqliteLog::open(&nested, &key()).expect("open");
    let rec = record("7f3a", Verb::Archived, user());
    log.append(NewHeader::of(&rec, NOW, &digest_key()), Some(rec.body))
        .expect("append");
    assert_eq!(log.head().expect("head").seq, Seq(1));
}

#[test]
fn append_checks_the_body_against_its_digest_when_the_log_has_the_key() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    let rec = record("7f3a", Verb::Archived, user());
    let header = NewHeader::of(&rec, NOW, &digest_key());
    let other = record("other", Verb::Deleted, user()).body;
    assert_eq!(log.append(header, Some(other)), Err(LogError::BadDigest));
    assert_eq!(log.head().expect("head").seq, Seq(0), "nothing was written");
}

#[test]
fn a_message_names_its_entities_in_the_things_rows() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut log = open(&dir);
    let named = view("7f3a").thing;
    let message = Message {
        id: MessageId::parse("m-2").expect("id"),
        thread: ThreadId::parse("m-1").expect("id"),
        in_reply_to: None,
        from: Address::new(AgentRef::Companion, space()),
        to: Address::new(AgentRef::User, space()),
        kind: MessageKind::Note,
        parts: vec![
            Part::Text(MessageText::new("done")),
            Part::Entity(named.clone()),
        ],
        label: label(Integrity::Trusted),
        sent: NOW,
    };
    let mut rec = record("7f3a", Verb::Archived, user());
    rec.body = EventBody::Message(Box::new(message));
    let entry = log
        .append(
            NewHeader::of(&rec, NOW, &digest_key()),
            Some(rec.body.clone()),
        )
        .expect("append");
    assert_eq!(
        log.touching(&named, RoleFilter::Source).expect("touching"),
        vec![entry.header.seq],
        "the message is found by the entity it names"
    );
}
