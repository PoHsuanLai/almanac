//! The row codec of `SqliteLog`: entries to and from `events`, `bodies` and `things` rows.

use crate::chain::{BodyState, Entry};
use crate::header::Header;
use crate::traits::LogError;
use almanac_core::{
    Digest32, Effect, EventBody, KindTag, Link32, ReplicaId, Seq, ThingRole, UnixSeconds,
};
use rusqlite::{Row, Transaction, params};

/// The columns of an entry with its body, in the order [`Stored::read`] takes them.
pub(crate) const SELECT_ENTRY: &str = "SELECT e.seq, e.occurred, e.recorded, e.actor, e.kind, \
     e.effect, e.label, e.cause, e.body_digest, e.prev, e.link, b.json \
     FROM events e LEFT JOIN bodies b ON b.seq = e.seq";

/// A rusqlite error as the log's: a wrong key (`NotADatabase`) is `Locked`, a full disk `Full`.
pub(crate) fn err(e: rusqlite::Error) -> LogError {
    use rusqlite::ErrorCode::{DiskFull, NotADatabase};
    match e.sqlite_error_code() {
        Some(NotADatabase) => LogError::Locked,
        Some(DiskFull) => LogError::Full,
        _ => LogError::Sqlite(e.to_string()),
    }
}

/// A sequence number as SQLite's integer (saturating; real numbers are far below).
pub(crate) fn to_sql(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A stored integer as a sequence number.
pub(crate) fn seq(n: i64) -> Result<Seq, LogError> {
    u64::try_from(n)
        .map(Seq)
        .map_err(|_| LogError::Corrupt { at: Seq(0) })
}

pub(crate) fn array16(bytes: &[u8]) -> Option<[u8; 16]> {
    bytes.try_into().ok()
}

fn array32(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.try_into().ok()
}

/// A stored link, corrupt at `at` when it is not 32 bytes.
pub(crate) fn link32(bytes: &[u8], at: Seq) -> Result<Link32, LogError> {
    array32(bytes).map(Link32).ok_or(LogError::Corrupt { at })
}

const fn effect_int(effect: Effect) -> i64 {
    match effect {
        Effect::Read => 0,
        Effect::UndoableWrite => 1,
        Effect::Outbound => 2,
        Effect::Destructive => 3,
    }
}

const fn effect_of(n: i64) -> Option<Effect> {
    match n {
        0 => Some(Effect::Read),
        1 => Some(Effect::UndoableWrite),
        2 => Some(Effect::Outbound),
        3 => Some(Effect::Destructive),
        _ => None,
    }
}

const fn role_text(role: ThingRole) -> &'static str {
    match role {
        ThingRole::Subject => "subject",
        ThingRole::Source => "source",
    }
}

/// One row as SQLite returned it, before it is decoded.
#[derive(Debug)]
pub(crate) struct Stored {
    seq: i64,
    occurred: i64,
    recorded: i64,
    actor: String,
    kind: String,
    effect: i64,
    label: String,
    cause: String,
    body_digest: Vec<u8>,
    prev: Vec<u8>,
    link: Vec<u8>,
    body: Option<String>,
}

impl Stored {
    /// Reads the columns of [`SELECT_ENTRY`].
    pub(crate) fn read(r: &Row<'_>) -> rusqlite::Result<Stored> {
        Ok(Stored {
            seq: r.get(0)?,
            occurred: r.get(1)?,
            recorded: r.get(2)?,
            actor: r.get(3)?,
            kind: r.get(4)?,
            effect: r.get(5)?,
            label: r.get(6)?,
            cause: r.get(7)?,
            body_digest: r.get(8)?,
            prev: r.get(9)?,
            link: r.get(10)?,
            body: r.get(11)?,
        })
    }

    /// Decodes the row; any column that does not parse is corruption at its sequence number.
    pub(crate) fn entry(self, replica: ReplicaId) -> Result<Entry, LogError> {
        let at = seq(self.seq)?;
        let bad = |_| LogError::Corrupt { at };
        let digest = array32(&self.body_digest).ok_or(LogError::Corrupt { at })?;
        let body = match self.body {
            Some(json) => {
                BodyState::Present(serde_json::from_str::<EventBody>(&json).map_err(bad)?)
            }
            None => BodyState::Erased,
        };
        Ok(Entry {
            header: Header {
                seq: at,
                replica,
                occurred: UnixSeconds(self.occurred),
                recorded: UnixSeconds(self.recorded),
                actor: serde_json::from_str(&self.actor).map_err(bad)?,
                kind: KindTag::parse(&self.kind).map_err(|_| LogError::Corrupt { at })?,
                effect: effect_of(self.effect).ok_or(LogError::Corrupt { at })?,
                label: serde_json::from_str(&self.label).map_err(bad)?,
                cause: serde_json::from_str(&self.cause).map_err(bad)?,
                body_digest: Digest32(digest),
                prev: link32(&self.prev, at)?,
            },
            link: link32(&self.link, at)?,
            body,
        })
    }
}

/// Inserts an entry's `events` row, and its `bodies` and `things` rows when it has a body.
pub(crate) fn insert(tx: &Transaction<'_>, entry: &Entry) -> Result<(), LogError> {
    let h = &entry.header;
    tx.execute(
        "INSERT INTO events(seq, occurred, recorded, actor, kind, effect, label, cause, \
         body_digest, prev, link) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            to_sql(h.seq.0),
            h.occurred.0,
            h.recorded.0,
            json(&h.actor),
            h.kind.as_str(),
            effect_int(h.effect),
            json(&h.label),
            json(&h.cause),
            h.body_digest.0.as_slice(),
            h.prev.0.as_slice(),
            entry.link.0.as_slice(),
        ],
    )
    .map_err(err)?;
    let BodyState::Present(body) = &entry.body else {
        return Ok(());
    };
    tx.execute(
        "INSERT INTO bodies(seq, json) VALUES (?1, ?2)",
        params![to_sql(h.seq.0), json(body)],
    )
    .map_err(err)?;
    for (thing, role) in body.thing_refs() {
        tx.execute(
            "INSERT INTO things(app, kind, key, seq, role) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                thing.app.as_str(),
                thing.kind.as_str(),
                thing.key.as_str(),
                to_sql(h.seq.0),
                role_text(role),
            ],
        )
        .map_err(err)?;
    }
    Ok(())
}

/// Compact JSON of a typed value, as the header bytes use (plain derives: cannot fail).
fn json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
