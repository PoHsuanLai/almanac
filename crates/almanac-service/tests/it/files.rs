//! Retention, export, config files and timeline rows.

use crate::support::*;
use almanac_core::*;
use almanac_seal::{Purpose, SpaceKey, derive};
use almanac_service::*;
use eventlog::{LogRead, LogWrite, MemoryLog, NewHeader};
use memfiles::VaultPath;
use std::io::Read;

#[test]
fn retention_sweep_table() {
    let day = 86_400;
    let at = |days: i64| UnixSeconds(NOW.0 + days * day);
    let cases: Vec<(&str, Retention, UnixSeconds, SourceState, Sweep)> = vec![
        (
            "29 days of 30",
            Retention::Days(DayCount(30)),
            at(29),
            SourceState::Exists,
            Sweep::Keep,
        ),
        (
            "30 days of 30",
            Retention::Days(DayCount(30)),
            at(30),
            SourceState::Exists,
            Sweep::EraseBody,
        ),
        (
            "a source that exists",
            Retention::WhileSourceExists,
            at(900),
            SourceState::Exists,
            Sweep::Keep,
        ),
        (
            "a deleted source",
            Retention::WhileSourceExists,
            NOW,
            SourceState::Gone,
            Sweep::EraseBody,
        ),
        (
            "until forgotten",
            Retention::UntilForgotten,
            at(9000),
            SourceState::Gone,
            Sweep::Keep,
        ),
    ];
    for (name, retention, now, source, want) in cases {
        assert_eq!(sweep_body(retention, NOW, now, source), want, "{name}");
    }
    assert!(!header_expired(NOW, UnixSeconds(NOW.0 + 364 * day)));
    assert!(header_expired(NOW, UnixSeconds(NOW.0 + 365 * day)));
}

fn entries() -> Vec<eventlog::Entry> {
    let key = derive(
        &SpaceKey::from_bytes([7; 32]),
        &space("work"),
        Purpose::Digest,
    );
    let mut log = MemoryLog::new(&space("work"), ReplicaId([9; 16]), key.clone());
    let rec = record(thing_body("org.quire.Mail", "7f3a"), user("org.quire.Mail"));
    log.append(NewHeader::of(&rec, NOW, &key), Some(rec.body.clone()))
        .expect("append");
    log.append(NewHeader::of(&rec, NOW, &key), None)
        .expect("append");
    log.scan(Seq(0)).expect("scan")
}

#[test]
fn event_lines_are_the_header_the_link_and_the_body_or_erased() {
    let lines: Vec<EventLine> = entries().iter().map(EventLine::of).collect();
    let text: String = lines.iter().map(|l| l.to_json() + "\n").collect();
    assert_eq!(
        text,
        include_str!("../golden/events.jsonl"),
        "events.jsonl lines are pinned"
    );
    for (line, text) in lines.iter().zip(text.lines()) {
        assert_eq!(
            &serde_json::from_str::<EventLine>(text).expect("parse"),
            line
        );
    }
    assert!(
        text.lines()
            .nth(1)
            .expect("line")
            .ends_with(r#""body":"erased"}"#)
    );
}

#[test]
fn export_tar_layout_golden() {
    let w = space("work");
    let manifest = ExportManifest {
        format: EXPORT_FORMAT.to_owned(),
        created: NOW,
        spaces: vec![ExportedSpace {
            space: w.clone(),
            head: None,
            counts: ExportCounts {
                events: Count(2),
                facts: Count(1),
                pending: Count(0),
                procedures: Count(0),
            },
        }],
        counts: ExportCounts {
            events: Count(2),
            facts: Count(1),
            pending: Count(0),
            procedures: Count(0),
        },
    };
    let lines: Vec<EventLine> = entries().iter().map(EventLine::of).collect();
    let mut writer = ExportWriter::new(Vec::new(), NOW);
    writer.manifest(&manifest).expect("manifest");
    writer.events(&w, &lines).expect("events");
    writer
        .file(
            &w,
            &VaultPath::parse("facts/prefs/meetings.md").expect("p"),
            b"# topic\n",
        )
        .expect("file");
    writer
        .file(
            &w,
            &VaultPath::parse("pending/01j9zk3m0q8h2v6x4c1b7n5t2a.md").expect("p"),
            b"p",
        )
        .expect("file");
    writer.rules("# rules\n").expect("rules");
    let bytes = writer.finish().expect("finish");
    let mut archive = tar::Archive::new(bytes.as_slice());
    let mut seen = Vec::new();
    for entry in archive.entries().expect("entries") {
        let mut entry = entry.expect("entry");
        let path = entry.path().expect("path").to_string_lossy().into_owned();
        let mtime = entry.header().mtime().expect("mtime");
        let mut body = String::new();
        entry.read_to_string(&mut body).expect("read");
        assert_eq!(mtime, u64::try_from(NOW.0).expect("time"));
        seen.push((path, body));
    }
    let names: Vec<&str> = seen.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        [
            "quire-memory-export-1/manifest.json",
            "quire-memory-export-1/work/events.jsonl",
            "quire-memory-export-1/work/facts/prefs/meetings.md",
            "quire-memory-export-1/work/pending/01j9zk3m0q8h2v6x4c1b7n5t2a.md",
            "quire-memory-export-1/rules.toml",
        ]
    );
    assert!(
        seen.iter()
            .all(|(p, _)| !p.contains("index.db") && !p.contains("key")),
        "no index and no keys inside"
    );
    let back: ExportManifest = serde_json::from_str(&seen[0].1).expect("manifest json");
    assert_eq!(back, manifest);
    assert_eq!(seen[1].1.lines().count(), 2);
    // The same inputs make the same bytes.
    let again = {
        let mut w2 = ExportWriter::new(Vec::new(), NOW);
        w2.manifest(&manifest).expect("m");
        w2.events(&w, &lines).expect("e");
        w2.finish().expect("f")
    };
    assert!(bytes.starts_with(&again[..512]), "headers are reproducible");
}

#[test]
fn config_files_round_trip() {
    let mut rules = RuleSet::standard();
    let r = |id: &str, scope| RememberRule {
        id: RuleId::parse(id).expect("id"),
        scope,
        mode: RememberMode::HeaderOnly,
        retention: Retention::Days(DayCount(7)),
    };
    rules.rules = vec![
        r("r-space", RuleScope::Space(space("work"))),
        r("r-app", RuleScope::App(app("org.quire.Mail"))),
        r(
            "r-kind",
            RuleScope::Kind(KindPattern::parse("mail.*").expect("k")),
        ),
        r(
            "r-path",
            RuleScope::Path(PathGlob::parse("/home/u/Downloads/**").expect("g")),
        ),
        r("r-thing", RuleScope::Thing(thing("org.quire.Mail", "7f3a"))),
        r("r-actor", RuleScope::Actor(ActorKind::Unknown)),
    ];
    let text = rules_to_toml(&rules).expect("encode");
    assert_eq!(rules_from_toml(&text).expect("decode"), rules);
    assert!(text.contains("[[rules]]") && text.contains("[[defaults]]"));
    assert!(matches!(
        rules_from_toml("rules = 3"),
        Err(ConfigError::Decode(_))
    ));

    let spaces = SpacesFile {
        spaces: vec![
            SpaceMeta {
                id: space("work"),
                created: NOW,
                replica: ReplicaId([1; 16]),
                vault: VaultKind::Sealed,
                format: 1,
            },
            SpaceMeta {
                id: space("desktop"),
                created: NOW,
                replica: ReplicaId([2; 16]),
                vault: VaultKind::Plain,
                format: 1,
            },
        ],
    };
    let text = spaces_to_toml(&spaces).expect("encode");
    assert_eq!(spaces_from_toml(&text).expect("decode"), spaces);
}

#[test]
fn timeline_rows_hide_erased_things() {
    let rows: Vec<TimelineEntry> = entries()
        .iter()
        .map(|e| timeline_entry(&space("work"), e, EraseCause::HeaderOnly, Count(0)))
        .collect();
    assert_eq!(rows[0].body, EntryBody::Present);
    assert_eq!(rows[0].things.len(), 1);
    assert_eq!(
        rows[0].event,
        EventRef {
            space: space("work"),
            replica: ReplicaId([9; 16]),
            seq: Seq(1)
        }
    );
    assert_eq!(
        rows[1].body,
        EntryBody::Erased {
            by: EraseCause::HeaderOnly
        }
    );
    assert!(rows[1].things.is_empty());
    assert_eq!(rows[1].kind.as_str(), "thing.archived");
}
