//! `ExactScan`: vectors as BLOBs in `index.db`, compared exhaustively. The default backend
//! (no extension, no `unsafe`); a swappable `VectorIndex` can replace it at scale.

use crate::doc::{Allow, DocId, Ranked, TopK};
use crate::fts::SCHEMA_V1;
use crate::index::IndexError;
use crate::vector::{EmbedderCard, Vector, nearest_exact};
use std::collections::BTreeSet;

/// What a vector index does.
pub trait VectorIndex: Send {
    /// Which vector space the stored vectors live in.
    fn card(&self) -> &EmbedderCard;
    /// Inserts or replaces vectors.
    fn upsert(&mut self, items: &[(DocId, Vector)]) -> Result<(), IndexError>;
    /// Removes vectors; how many were there.
    fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError>;
    /// The `k` nearest to `q` among the allowed documents, best first.
    fn nearest(&self, q: &Vector, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexError>;
    /// Removes everything.
    fn clear(&mut self) -> Result<(), IndexError>;
    /// The ids that have a vector.
    fn ids(&self) -> Result<BTreeSet<DocId>, IndexError>;
}

/// Exhaustive scan over the `vectors` table (see `nearest_exact` for the scoring).
#[derive(Debug)]
pub struct ExactScan {
    conn: rusqlite::Connection,
    card: EmbedderCard,
}

impl ExactScan {
    /// Over an open connection (the same `index.db` as `Fts5`) for vectors of `card`.
    pub fn new(conn: rusqlite::Connection, card: EmbedderCard) -> Self {
        Self { conn, card }
    }

    /// A private in-memory database with the schema, for tests.
    pub fn in_memory(card: EmbedderCard) -> Result<Self, IndexError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA_V1)?;
        Ok(Self::new(conn, card))
    }

    /// The connection.
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

impl ExactScan {
    fn dims(&self) -> usize {
        usize::try_from(self.card.dims).unwrap_or(usize::MAX)
    }
}

impl VectorIndex for ExactScan {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    fn upsert(&mut self, items: &[(DocId, Vector)]) -> Result<(), IndexError> {
        if items.iter().any(|(_, v)| v.0.len() != self.dims()) {
            return Err(IndexError::CardMismatch);
        }
        let tx = self.conn.transaction()?;
        {
            let mut put =
                tx.prepare_cached("INSERT OR REPLACE INTO vectors(id, vec) VALUES (?1, ?2)")?;
            for (id, vector) in items {
                put.execute(rusqlite::params![id.0, vector.to_blob()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError> {
        let tx = self.conn.transaction()?;
        let mut removed = 0usize;
        {
            let mut delete = tx.prepare_cached("DELETE FROM vectors WHERE id = ?1")?;
            for id in ids {
                removed += delete.execute([&id.0])?;
            }
        }
        tx.commit()?;
        Ok(u32::try_from(removed).unwrap_or(u32::MAX))
    }

    fn nearest(&self, q: &Vector, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexError> {
        if q.0.len() != self.dims() {
            return Err(IndexError::CardMismatch);
        }
        let mut statement = self.conn.prepare_cached("SELECT id, vec FROM vectors")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (id, blob) = row?;
            let id = DocId(id);
            if !allow.permits(&id) {
                continue;
            }
            match Vector::from_blob(&blob) {
                Some(v) if v.0.len() == self.dims() => items.push((id, v)),
                Some(_) | None => return Err(IndexError::CardMismatch),
            }
        }
        Ok(nearest_exact(q, &items, self.card.metric, k))
    }

    fn clear(&mut self) -> Result<(), IndexError> {
        self.conn.execute("DELETE FROM vectors", [])?;
        Ok(())
    }

    fn ids(&self) -> Result<BTreeSet<DocId>, IndexError> {
        let mut statement = self.conn.prepare_cached("SELECT id FROM vectors")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0).map(DocId))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
