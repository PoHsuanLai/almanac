//! `SqliteLog`: the log in SQLCipher. Frozen: the schema and the signatures; the bodies are
//! `todo!()` until the eventlog fill.

use crate::chain::Entry;
use crate::header::NewHeader;
use crate::traits::{LogError, LogRead, LogWrite, PageQuery, RoleFilter};
use almanac_core::{Checkpoint, Count, EventBody, Head, Seq, ThingRef};
use almanac_seal::DbKey;
use std::path::Path;

/// The schema number stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: u32 = 1;

/// The tables of `events.db` (SQLCipher; `PRAGMA secure_delete=ON`, `journal_mode=WAL`, and
/// `wal_checkpoint(TRUNCATE)` after an erase). Bodies and `things` rows are the erasable part;
/// `events` is the chain.
pub const SCHEMA_V1: &str = "
CREATE TABLE meta(format INTEGER NOT NULL, replica BLOB NOT NULL);
CREATE TABLE events(
  seq INTEGER PRIMARY KEY, occurred INTEGER NOT NULL, recorded INTEGER NOT NULL,
  actor TEXT NOT NULL, kind TEXT NOT NULL, effect INTEGER NOT NULL, label TEXT NOT NULL,
  cause TEXT NOT NULL, body_digest BLOB NOT NULL, prev BLOB NOT NULL, link BLOB NOT NULL);
CREATE TABLE bodies(seq INTEGER PRIMARY KEY REFERENCES events(seq), json TEXT NOT NULL);
CREATE TABLE things(
  app TEXT NOT NULL, kind TEXT NOT NULL, key TEXT NOT NULL, seq INTEGER NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('subject', 'source')));
CREATE INDEX things_by_thing ON things(app, kind, key);
CREATE INDEX things_by_seq ON things(seq);
CREATE TABLE aliases(app TEXT NOT NULL, kind TEXT NOT NULL, old_key TEXT NOT NULL, new_key TEXT NOT NULL);
CREATE TABLE checkpoints(cut INTEGER PRIMARY KEY, link BLOB NOT NULL, at INTEGER NOT NULL);
";

/// The event log of one Space in one SQLCipher file.
#[derive(Debug)]
pub struct SqliteLog {
    conn: rusqlite::Connection,
}

impl SqliteLog {
    /// Opens (or creates) the log at `path`, keyed with `key`; a wrong key is
    /// [`LogError::Locked`].
    pub fn open(path: &Path, key: &DbKey) -> Result<Self, LogError> {
        let _ = (path, key);
        todo!(
            "open with PRAGMA key, check the schema number, create SCHEMA_V1 on a new file, set secure_delete and WAL"
        )
    }

    /// The connection, for the fill's own queries.
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

impl LogRead for SqliteLog {
    fn head(&self) -> Result<Head, LogError> {
        todo!("the newest `events` row, or the newest checkpoint")
    }

    fn checkpoint(&self) -> Result<Checkpoint, LogError> {
        todo!("the newest `checkpoints` row, or genesis")
    }

    fn page(&self, q: &PageQuery) -> Result<Vec<Entry>, LogError> {
        let _ = q;
        todo!("newest first from the cursor; `filter::passes` after the read")
    }

    fn touching(&self, thing: &ThingRef, role: RoleFilter) -> Result<Vec<Seq>, LogError> {
        let _ = (thing, role);
        todo!("select seq from `things` by (app, kind, key) and role")
    }

    fn scan(&self, from: Seq) -> Result<Vec<Entry>, LogError> {
        let _ = from;
        todo!("ascending from `from`, joining `bodies`")
    }
}

impl LogWrite for SqliteLog {
    fn append(&mut self, header: NewHeader, body: Option<EventBody>) -> Result<Entry, LogError> {
        let _ = (header, body);
        todo!("one transaction: events row, bodies row, things rows")
    }

    fn erase_bodies(&mut self, seqs: &[Seq]) -> Result<Count, LogError> {
        let _ = seqs;
        todo!("delete from bodies and things, then wal_checkpoint(TRUNCATE)")
    }

    fn prune_before(&mut self, cut: Seq) -> Result<Checkpoint, LogError> {
        let _ = cut;
        todo!("record the checkpoint, delete the prefix, then wal_checkpoint(TRUNCATE)")
    }
}
