//! Lexical search: FTS5 in `index.db`. One implementation, so a struct and not a trait.

use crate::doc::{Allow, Doc, DocId, Ranked, TopK};
use crate::index::IndexError;

/// The tables of `index.db` (SQLCipher, deletable and rebuilt from the files and the log).
/// `docs` is the FTS5 table; `vectors` holds `ExactScan`'s BLOBs; `meta` records the embedder
/// card the vectors came from and the format.
pub const SCHEMA_V1: &str = "
CREATE VIRTUAL TABLE docs USING fts5(
  text, id UNINDEXED, kind UNINDEXED, app UNINDEXED, trust UNINDEXED, at_s UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2');
CREATE TABLE vectors(id TEXT PRIMARY KEY, vec BLOB NOT NULL);
CREATE TABLE meta(format INTEGER NOT NULL, model TEXT, dims INTEGER, metric TEXT);
";

/// The FTS5 `MATCH` expression for free text: every alphanumeric word quoted, joined with `OR`
/// so ranking (bm25) decides, never a syntax error. Empty text gives `None`.
pub fn match_expression(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\""))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

/// The lexical half of the index.
#[derive(Debug)]
pub struct Fts5 {
    conn: rusqlite::Connection,
}

impl Fts5 {
    /// Wraps an open (and, for a file, keyed) connection.
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// The connection, shared with `ExactScan` in one `index.db`.
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.conn
    }

    /// Creates the tables of [`SCHEMA_V1`] in an empty database.
    pub fn create(&self) -> Result<(), IndexError> {
        todo!("execute SCHEMA_V1 and insert the meta row")
    }

    /// Inserts or replaces documents.
    pub fn upsert(&mut self, docs: &[Doc]) -> Result<(), IndexError> {
        let _ = docs;
        todo!("delete then insert each id in one transaction")
    }

    /// Removes documents; how many were there.
    pub fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError> {
        let _ = ids;
        todo!("delete by id, return the changes")
    }

    /// The `k` best lexical matches (bm25) among the allowed documents.
    pub fn search(&self, text: &str, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexError> {
        let _ = (text, k, allow, match_expression(text));
        todo!("SELECT id FROM docs WHERE docs MATCH ? ORDER BY bm25(docs), filter by allow")
    }

    /// Removes everything.
    pub fn clear(&mut self) -> Result<(), IndexError> {
        todo!("delete from docs")
    }
}
