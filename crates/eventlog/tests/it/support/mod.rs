#![allow(dead_code)]

use almanac_core::*;
use almanac_seal::{Purpose, SpaceKey, SubKey, derive};
use eventlog::*;
use std::collections::BTreeSet;

pub const NOW: UnixSeconds = UnixSeconds(1_790_000_000);

pub fn space() -> SpaceId {
    SpaceId::parse("work").expect("space")
}

pub fn digest_key() -> SubKey {
    derive(&SpaceKey::from_bytes([7; 32]), &space(), Purpose::Digest)
}

pub fn new_log() -> MemoryLog {
    MemoryLog::new(&space(), ReplicaId([9; 16]), digest_key())
}

pub fn app(name: &str) -> AppName {
    AppName::parse(name).expect("app")
}

pub fn label(integrity: Integrity) -> Label {
    Label {
        integrity,
        confidentiality: Confidentiality::Public,
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::User]),
    }
}

pub fn view(key: &str) -> ThingView {
    ThingView {
        thing: ThingRef {
            app: app("org.quire.Mail"),
            kind: ThingKind::parse("mail.thread").expect("kind"),
            key: ThingKey::parse(key).expect("key"),
        },
        title: "Q4 budget".into(),
        subtitle: "".into(),
    }
}

pub fn record(key: &str, verb: Verb, actor: Actor) -> Record {
    Record {
        space: space(),
        occurred: NOW,
        actor,
        effect: Effect::UndoableWrite,
        label: label(Integrity::Trusted),
        body: EventBody::Thing {
            verb,
            thing: view(key),
            sources: vec![],
        },
        cause: Cause::None,
    }
}

pub fn user() -> Actor {
    Actor::User {
        via: app("org.quire.Mail"),
    }
}

pub fn put(log: &mut MemoryLog, record: &Record) -> Entry {
    let header = NewHeader::of(record, UnixSeconds(NOW.0 + 1), &digest_key());
    log.append(header, Some(record.body.clone()))
        .expect("append")
}

pub fn put_header_only(log: &mut MemoryLog, record: &Record) -> Entry {
    let header = NewHeader::of(record, UnixSeconds(NOW.0 + 1), &digest_key());
    log.append(header, None).expect("append")
}

/// Three entries: archived 7f3a, viewed 7f3a, archived 8b21.
pub fn filled() -> MemoryLog {
    let mut log = new_log();
    put(&mut log, &record("7f3a", Verb::Archived, user()));
    put(&mut log, &record("7f3a", Verb::Viewed, Actor::Unknown));
    put(&mut log, &record("8b21", Verb::Archived, user()));
    log
}

pub fn verify(log: &MemoryLog) -> ChainReport {
    let from = log.checkpoint().expect("checkpoint");
    verify_chain(&from, &log.scan(Seq(0)).expect("scan"), &digest_key())
}
