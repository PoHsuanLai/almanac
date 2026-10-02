//! `SqliteLog`: the log in SQLCipher. Frozen: the schema and the signatures.
//!
//! One transaction per write, `secure_delete=ON`, WAL, and a truncating checkpoint after every
//! erase so forgotten bodies do not linger in the write-ahead log. The row codec is in
//! `rows.rs`.

use crate::chain::{BodyState, Entry, verify_chain};
use crate::filter::passes;
use crate::header::{NewHeader, genesis_link, link};
use crate::rows::{self, Stored, to_sql};
use crate::traits::{LogError, LogRead, LogWrite, PageQuery, RoleFilter};
use almanac_core::{
    ChainReport, Checkpoint, Count, EventBody, Head, ReplicaId, Seq, SpaceId, ThingRef,
};
use almanac_seal::{DbKey, SpaceKey, SubKey};
use rusqlite::{Connection, OptionalExtension, params};
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

/// The Space a log opened without one is bound to: its genesis link then differs from any real
/// Space's, so a chain written by [`SqliteLog::open`] is only verifiable from its own
/// checkpoint. [`SqliteLog::open_for`] is the form the daemon uses.
const UNBOUND_SPACE: &str = "unbound";

/// The event log of one Space in one SQLCipher file.
#[derive(Debug)]
pub struct SqliteLog {
    conn: Connection,
    replica: ReplicaId,
}

impl SqliteLog {
    /// Opens (or creates) the log at `path`, keyed with `key`; a wrong key is
    /// [`LogError::Locked`]. A new file gets a random replica and a genesis bound to no Space;
    /// use [`SqliteLog::open_for`] to name them.
    pub fn open(path: &Path, key: &DbKey) -> Result<Self, LogError> {
        let space = SpaceId::parse(UNBOUND_SPACE).map_err(|e| LogError::Sqlite(e.to_string()))?;
        let replica = SpaceKey::generate().map_err(|e| LogError::Sqlite(e.to_string()))?;
        let mut id = [0u8; 16];
        id.copy_from_slice(&replica.expose()[..16]);
        Self::open_for(path, key, &space, ReplicaId(id))
    }

    /// Opens (or creates) the log of `space` on `replica`. A file that already exists keeps its
    /// own replica and genesis; the two arguments only seed a new one.
    pub fn open_for(
        path: &Path,
        key: &DbKey,
        space: &SpaceId,
        replica: ReplicaId,
    ) -> Result<Self, LogError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| LogError::Sqlite(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(rows::err)?;
        conn.pragma_update(None, "key", key.pragma())
            .map_err(rows::err)?;
        // SQLCipher reads nothing until the first query: a wrong key shows here.
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(rows::err)?;
        conn.pragma_update(None, "secure_delete", "ON")
            .map_err(rows::err)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .map_err(rows::err)?;
        let replica = match version {
            0 => Self::create(&conn, space, replica)?,
            SCHEMA_VERSION => conn
                .query_row("SELECT replica FROM meta", [], |r| r.get::<_, Vec<u8>>(0))
                .map_err(rows::err)
                .and_then(|b| {
                    rows::array16(&b)
                        .map(ReplicaId)
                        .ok_or(LogError::Corrupt { at: Seq(0) })
                })?,
            found => return Err(LogError::Schema { found }),
        };
        Ok(Self { conn, replica })
    }

    fn create(
        conn: &Connection,
        space: &SpaceId,
        replica: ReplicaId,
    ) -> Result<ReplicaId, LogError> {
        let genesis = genesis_link(space, &replica);
        let tx = conn.unchecked_transaction().map_err(rows::err)?;
        tx.execute_batch(SCHEMA_V1).map_err(rows::err)?;
        tx.execute(
            "INSERT INTO meta(format, replica) VALUES (?1, ?2)",
            params![SCHEMA_VERSION, replica.0.as_slice()],
        )
        .map_err(rows::err)?;
        tx.execute(
            "INSERT INTO checkpoints(cut, link, at) VALUES (0, ?1, 0)",
            params![genesis.0.as_slice()],
        )
        .map_err(rows::err)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(rows::err)?;
        tx.commit().map_err(rows::err)?;
        Ok(replica)
    }

    /// The connection, for the fill's own queries.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The replica this log writes as.
    pub fn replica(&self) -> ReplicaId {
        self.replica
    }

    /// The audit: reads every retained entry and verifies the chain from the checkpoint, the
    /// present bodies against their keyed digests with `digest_key`.
    pub fn audit(&self, digest_key: &SubKey) -> Result<ChainReport, LogError> {
        let from = self.checkpoint()?;
        let entries = self.scan(Seq(from.cut.0 + 1))?;
        Ok(verify_chain(&from, &entries, digest_key))
    }

    fn truncate_wal(&self) -> Result<(), LogError> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(rows::err)
    }

    fn entries(&self, sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<Entry>, LogError> {
        let mut stmt = self.conn.prepare_cached(sql).map_err(rows::err)?;
        let found = stmt.query_map(args, Stored::read).map_err(rows::err)?;
        found
            .map(|row| {
                row.map_err(rows::err)
                    .and_then(|stored| stored.entry(self.replica))
            })
            .collect()
    }
}

impl LogRead for SqliteLog {
    fn head(&self) -> Result<Head, LogError> {
        let newest = self
            .conn
            .query_row(
                "SELECT seq, link FROM events ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(rows::err)?;
        match newest {
            Some((seq, link)) => Ok(Head {
                seq: rows::seq(seq)?,
                link: rows::link32(&link, rows::seq(seq)?)?,
            }),
            None => self.checkpoint().map(|c| Head {
                seq: c.cut,
                link: c.link,
            }),
        }
    }

    fn checkpoint(&self) -> Result<Checkpoint, LogError> {
        let (cut, link) = self
            .conn
            .query_row(
                "SELECT cut, link FROM checkpoints ORDER BY cut DESC LIMIT 1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .map_err(rows::err)?;
        let cut = rows::seq(cut)?;
        Ok(Checkpoint {
            cut,
            link: rows::link32(&link, cut)?,
        })
    }

    fn page(&self, q: &PageQuery) -> Result<Vec<Entry>, LogError> {
        let limit = usize::try_from(q.limit.0).unwrap_or(usize::MAX);
        let before = q.before.map_or(i64::MAX, |c| to_sql(c.0.0));
        let mut stmt = self
            .conn
            .prepare_cached(&format!(
                "{} WHERE e.seq < ?1 ORDER BY e.seq DESC",
                rows::SELECT_ENTRY
            ))
            .map_err(rows::err)?;
        let mut found = stmt
            .query_map(params![before], Stored::read)
            .map_err(rows::err)?;
        let mut page = Vec::new();
        while page.len() < limit {
            let Some(row) = found.next() else { break };
            let entry = row.map_err(rows::err).and_then(|s| s.entry(self.replica))?;
            if passes(&q.filter, &entry) {
                page.push(entry);
            }
        }
        Ok(page)
    }

    fn touching(&self, thing: &ThingRef, role: RoleFilter) -> Result<Vec<Seq>, LogError> {
        let wanted = match role {
            RoleFilter::Either => "role IN ('subject', 'source')",
            RoleFilter::Subject => "role = 'subject'",
            RoleFilter::Source => "role = 'source'",
        };
        let sql = format!(
            "SELECT DISTINCT seq FROM things WHERE app = ?1 AND kind = ?2 AND key = ?3 \
             AND {wanted} ORDER BY seq"
        );
        let mut stmt = self.conn.prepare_cached(&sql).map_err(rows::err)?;
        let found = stmt
            .query_map(
                params![thing.app.as_str(), thing.kind.as_str(), thing.key.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .map_err(rows::err)?;
        found
            .map(|n| n.map_err(rows::err).and_then(rows::seq))
            .collect()
    }

    fn scan(&self, from: Seq) -> Result<Vec<Entry>, LogError> {
        self.entries(
            &format!("{} WHERE e.seq >= ?1 ORDER BY e.seq", rows::SELECT_ENTRY),
            &[&to_sql(from.0)],
        )
    }
}

impl LogWrite for SqliteLog {
    fn append(&mut self, header: NewHeader, body: Option<EventBody>) -> Result<Entry, LogError> {
        let tip = self.head()?;
        let chained = header.chained(Seq(tip.seq.0 + 1), self.replica, tip.link);
        let entry = Entry {
            link: link(&chained),
            header: chained,
            body: body.map_or(BodyState::Erased, BodyState::Present),
        };
        let tx = self.conn.transaction().map_err(rows::err)?;
        rows::insert(&tx, &entry)?;
        tx.commit().map_err(rows::err)?;
        Ok(entry)
    }

    fn erase_bodies(&mut self, seqs: &[Seq]) -> Result<Count, LogError> {
        let tx = self.conn.transaction().map_err(rows::err)?;
        let mut erased: u32 = 0;
        for seq in seqs {
            let gone = tx
                .execute("DELETE FROM bodies WHERE seq = ?1", params![to_sql(seq.0)])
                .map_err(rows::err)?;
            tx.execute("DELETE FROM things WHERE seq = ?1", params![to_sql(seq.0)])
                .map_err(rows::err)?;
            erased += u32::try_from(gone).unwrap_or(u32::MAX);
        }
        tx.commit().map_err(rows::err)?;
        self.truncate_wal()?;
        Ok(Count(erased))
    }

    fn prune_before(&mut self, cut: Seq) -> Result<Checkpoint, LogError> {
        let tx = self.conn.transaction().map_err(rows::err)?;
        let at = tx
            .query_row(
                "SELECT link, recorded FROM events WHERE seq = ?1",
                params![to_sql(cut.0)],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(rows::err)?
            .ok_or(LogError::NoSuchEntry(cut))?;
        let checkpoint = Checkpoint {
            cut,
            link: rows::link32(&at.0, cut)?,
        };
        let cut_sql = to_sql(cut.0);
        tx.execute(
            "INSERT INTO checkpoints(cut, link, at) VALUES (?1, ?2, ?3)",
            params![cut_sql, checkpoint.link.0.as_slice(), at.1],
        )
        .map_err(rows::err)?;
        for table in ["things", "bodies", "events"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE seq <= ?1"),
                params![cut_sql],
            )
            .map_err(rows::err)?;
        }
        tx.commit().map_err(rows::err)?;
        self.truncate_wal()?;
        Ok(checkpoint)
    }
}
