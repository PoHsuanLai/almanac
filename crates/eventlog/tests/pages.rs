//! Paging, filtering and thing lookups, and the contract every log implementation meets.

mod common;

use almanac_core::*;
use common::*;
use eventlog::*;

fn query(before: Option<u64>, limit: u32, filter: TimelineFilter) -> PageQuery {
    TimelineQuery {
        before: before.map(|s| Cursor(Seq(s))),
        limit: Count(limit),
        filter,
    }
}

fn everyone() -> TimelineFilter {
    TimelineFilter {
        actors: ActorFilter::Everyone,
        apps: vec![],
        kinds: vec![],
        trust: TrustFilter::Any,
        range: None,
    }
}

fn seqs(entries: &[Entry]) -> Vec<u64> {
    entries.iter().map(|e| e.header.seq.0).collect()
}

/// The behaviour every `LogWrite` must have: `MemoryLog` now, `SqliteLog` at the fill.
fn contract<L: LogWrite>(mut log: L, digest: &almanac_seal::SubKey) {
    let put = |log: &mut L, key: &str, verb: Verb, actor: Actor| {
        let rec = record(key, verb, actor);
        log.append(NewHeader::of(&rec, NOW, digest), Some(rec.body.clone()))
            .expect("append")
    };
    put(&mut log, "7f3a", Verb::Archived, user());
    put(&mut log, "7f3a", Verb::Viewed, Actor::Unknown);
    put(&mut log, "8b21", Verb::Archived, user());

    assert_eq!(
        seqs(&log.page(&query(None, 10, everyone())).expect("page")),
        vec![3, 2, 1]
    );
    assert_eq!(
        seqs(&log.page(&query(Some(3), 1, everyone())).expect("page")),
        vec![2]
    );
    let you = TimelineFilter {
        actors: ActorFilter::You,
        ..everyone()
    };
    assert_eq!(
        seqs(&log.page(&query(None, 10, you)).expect("page")),
        vec![3, 1]
    );
    let unknown = TimelineFilter {
        actors: ActorFilter::Unknown,
        ..everyone()
    };
    assert_eq!(
        seqs(&log.page(&query(None, 10, unknown)).expect("page")),
        vec![2]
    );
    let viewed = TimelineFilter {
        kinds: vec![KindPattern::parse("thing.viewed").expect("k")],
        ..everyone()
    };
    assert_eq!(
        seqs(&log.page(&query(None, 10, viewed)).expect("page")),
        vec![2]
    );
    let untrusted = TimelineFilter {
        trust: TrustFilter::UntrustedOnly,
        ..everyone()
    };
    assert!(
        log.page(&query(None, 10, untrusted))
            .expect("page")
            .is_empty()
    );
    let other_app = TimelineFilter {
        apps: vec![app("org.quire.Files")],
        ..everyone()
    };
    assert!(
        log.page(&query(None, 10, other_app))
            .expect("page")
            .is_empty()
    );
    let out_of_range = TimelineFilter {
        range: Some((UnixSeconds(1), UnixSeconds(2))),
        ..everyone()
    };
    assert!(
        log.page(&query(None, 10, out_of_range))
            .expect("page")
            .is_empty()
    );

    let thing = view("7f3a").thing;
    let touching = log.touching(&thing, RoleFilter::Subject).expect("touching");
    assert_eq!(touching, vec![Seq(1), Seq(2)]);
    assert!(
        log.touching(&thing, RoleFilter::Source)
            .expect("touching")
            .is_empty()
    );
    assert_eq!(log.scan(Seq(2)).expect("scan").len(), 2);
    assert_eq!(log.head().expect("head").seq, Seq(3));
}

#[test]
fn memory_log_meets_the_contract() {
    let log = new_log();
    contract(log, &digest_key());
}

#[test]
#[ignore = "SqliteLog is a todo!() until the eventlog fill (FINDINGS.md)"]
fn sqlite_and_memory_logs_agree() {
    let dir = std::env::temp_dir().join("almanac-eventlog-contract");
    let key = almanac_seal::DbKey::of(&digest_key());
    let log = SqliteLog::open(&dir.join("events.db"), &key).expect("open");
    contract(log, &digest_key());
}

#[test]
#[ignore = "SqliteLog is a todo!() until the eventlog fill (FINDINGS.md)"]
fn wrong_key_is_locked() {
    let dir = std::env::temp_dir().join("almanac-eventlog-locked");
    let right = almanac_seal::DbKey::of(&digest_key());
    drop(SqliteLog::open(&dir.join("events.db"), &right).expect("create"));
    let other = almanac_seal::derive(
        &almanac_seal::SpaceKey::from_bytes([1; 32]),
        &space(),
        almanac_seal::Purpose::Eventlog,
    );
    assert!(matches!(
        SqliteLog::open(&dir.join("events.db"), &almanac_seal::DbKey::of(&other)),
        Err(LogError::Locked)
    ));
}

#[test]
fn the_schema_is_valid_sql_and_has_the_documented_tables() {
    let conn = rusqlite::Connection::open_in_memory().expect("memory db");
    conn.execute_batch(SCHEMA_V1).expect("schema");
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("prepare");
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    assert_eq!(
        names,
        [
            "aliases",
            "bodies",
            "checkpoints",
            "events",
            "meta",
            "things"
        ]
    );
}
