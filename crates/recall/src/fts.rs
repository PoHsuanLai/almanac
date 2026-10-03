//! Lexical search: FTS5 in `index.db`. One implementation, so a struct and not a trait.

use crate::doc::{Allow, Doc, DocId, Facets, Ranked, StoredDoc, TopK, TrustTier};
use crate::index::IndexError;
use crate::vector::EmbedderCard;

/// The tables of `index.db` (SQLCipher, deletable and rebuilt from the files and the log).
/// `docs` is the FTS5 table; `vectors` holds `ExactScan`'s BLOBs; `meta` records the embedder
/// card the vectors came from (`space` is `EmbedderCard::space_key`, empty until vectors exist;
/// a file made before the column is given it when the card is first recorded) and the format.
pub const SCHEMA_V1: &str = "
CREATE VIRTUAL TABLE docs USING fts5(
  text, id UNINDEXED, kind UNINDEXED, app UNINDEXED, trust UNINDEXED, at_s UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2');
CREATE TABLE vectors(id TEXT PRIMARY KEY, vec BLOB NOT NULL);
CREATE TABLE meta(format INTEGER NOT NULL, model TEXT, dims INTEGER, metric TEXT, space TEXT);
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
        self.conn.execute_batch(SCHEMA_V1)?;
        self.conn.execute(
            "INSERT INTO meta(format, model, dims, metric, space) VALUES (?1, NULL, NULL, NULL, NULL)",
            [FORMAT],
        )?;
        Ok(())
    }

    /// Inserts or replaces documents.
    pub fn upsert(&mut self, docs: &[Doc]) -> Result<(), IndexError> {
        let tx = self.conn.transaction()?;
        {
            let mut delete = tx.prepare_cached("DELETE FROM docs WHERE id = ?1")?;
            for doc in docs {
                delete.execute([&doc.id.0])?;
            }
        }
        insert_all(&tx, docs)?;
        tx.commit()?;
        Ok(())
    }

    /// Inserts documents known not to be there (a rebuild after `clear`), skipping the delete
    /// scan an upsert pays per id.
    pub(crate) fn insert_new(&mut self, docs: &[Doc]) -> Result<(), IndexError> {
        let tx = self.conn.transaction()?;
        insert_all(&tx, docs)?;
        tx.commit()?;
        Ok(())
    }

    /// Removes documents; how many were there.
    pub fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError> {
        let tx = self.conn.transaction()?;
        let mut removed = 0usize;
        {
            let mut delete = tx.prepare_cached("DELETE FROM docs WHERE id = ?1")?;
            for id in ids {
                removed += delete.execute([&id.0])?;
            }
        }
        tx.commit()?;
        Ok(u32::try_from(removed).unwrap_or(u32::MAX))
    }

    /// The `k` best lexical matches (bm25, ties by id) among the allowed documents.
    pub fn search(&self, text: &str, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexError> {
        let Some(expression) = match_expression(text) else {
            return Ok(Vec::new());
        };
        let mut statement = self
            .conn
            .prepare_cached("SELECT id FROM docs WHERE docs MATCH ?1 ORDER BY bm25(docs), id")?;
        let ids = statement.query_map([expression], |row| row.get::<_, String>(0))?;
        let mut hits = Vec::new();
        for id in ids {
            let id = DocId(id?);
            if allow.permits(&id) {
                hits.push(id);
            }
            if hits.len() >= usize::try_from(k.0).unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(hits
            .into_iter()
            .zip(1u32..)
            .map(|(id, rank)| Ranked { id, rank })
            .collect())
    }

    fn has_space_column(&self) -> Result<bool, IndexError> {
        Ok(self
            .conn
            .prepare("SELECT 1 FROM pragma_table_info('meta') WHERE name = 'space'")?
            .exists([])?)
    }

    /// The vector space the index says its vectors are in: `None` for a file that never recorded
    /// one (so its vectors cannot be trusted).
    pub fn recorded_space(&self) -> Result<Option<String>, IndexError> {
        if !self.has_space_column()? {
            return Ok(None);
        }
        Ok(self
            .conn
            .query_row("SELECT space FROM meta", [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?)
    }

    /// Records that the vectors being stored are `card`'s.
    pub fn record_space(&self, card: &EmbedderCard) -> Result<(), IndexError> {
        if !self.has_space_column()? {
            self.conn
                .execute("ALTER TABLE meta ADD COLUMN space TEXT", [])?;
        }
        let metric = format!("{:?}", card.metric).to_lowercase();
        let changed = self.conn.execute(
            "UPDATE meta SET model = ?1, dims = ?2, metric = ?3, space = ?4",
            rusqlite::params![card.model, card.dims, metric, card.space_key()],
        )?;
        if changed == 0 {
            self.conn.execute(
                "INSERT INTO meta(format, model, dims, metric, space) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![FORMAT, card.model, card.dims, metric, card.space_key()],
            )?;
        }
        Ok(())
    }

    /// Every document the lexical half holds, without its class (which is not stored).
    pub fn stored(&self) -> Result<Vec<StoredDoc>, IndexError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, text, kind, app, trust, at_s FROM docs")?;
        let rows = statement.query_map([], |row| {
            let trust: String = row.get(4)?;
            Ok(StoredDoc {
                id: DocId(row.get(0)?),
                text: row.get(1)?,
                at: row.get(5)?,
                facets: Facets {
                    kind: row.get(2)?,
                    app: row.get(3)?,
                    trust: if trust == "trusted" {
                        TrustTier::Trusted
                    } else {
                        TrustTier::Untrusted
                    },
                },
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Removes everything.
    pub fn clear(&mut self) -> Result<(), IndexError> {
        self.conn.execute("DELETE FROM docs", [])?;
        Ok(())
    }
}

/// The `meta.format` of [`SCHEMA_V1`].
const FORMAT: i64 = 1;

fn trust_slug(trust: TrustTier) -> &'static str {
    match trust {
        TrustTier::Trusted => "trusted",
        TrustTier::Untrusted => "untrusted",
    }
}

fn insert_all(tx: &rusqlite::Transaction<'_>, docs: &[Doc]) -> Result<(), IndexError> {
    let mut insert = tx.prepare_cached(
        "INSERT INTO docs(text, id, kind, app, trust, at_s) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for doc in docs {
        insert.execute(rusqlite::params![
            doc.text,
            doc.id.0,
            doc.facets.kind,
            doc.facets.app,
            trust_slug(doc.facets.trust),
            doc.at
        ])?;
    }
    Ok(())
}
