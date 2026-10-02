//! `ExactScan`: vectors as BLOBs in `index.db`, compared exhaustively. The default backend
//! (no extension, no `unsafe`); a swappable `VectorIndex` can replace it at scale.

use crate::doc::{Allow, DocId, Ranked, TopK};
use crate::index::IndexError;
use crate::vector::{EmbedderCard, Vector};

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
        let _ = card;
        todo!("open an in-memory connection and execute the vectors part of SCHEMA_V1")
    }

    /// The connection.
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

impl VectorIndex for ExactScan {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    fn upsert(&mut self, items: &[(DocId, Vector)]) -> Result<(), IndexError> {
        let _ = items;
        todo!("INSERT OR REPLACE INTO vectors with Vector::to_blob; CardMismatch on a wrong length")
    }

    fn remove(&mut self, ids: &[DocId]) -> Result<u32, IndexError> {
        let _ = ids;
        todo!("DELETE FROM vectors")
    }

    fn nearest(&self, q: &Vector, k: TopK, allow: &Allow) -> Result<Vec<Ranked>, IndexError> {
        let _ = (q, k, allow);
        todo!("read the rows, filter by allow, nearest_exact")
    }

    fn clear(&mut self) -> Result<(), IndexError> {
        todo!("DELETE FROM vectors")
    }
}
