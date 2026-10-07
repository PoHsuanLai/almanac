//! `index.db`: FTS5 and the vectors in one SQLCipher file.

use almanac_seal::DbKey;
use recall::{Embedder, ExactScan, Fts5, Index, IndexError};
use rusqlite::Connection;
use std::path::Path;

/// A SQLCipher file opened with `key` (`PRAGMA key`, then `secure_delete` so removed rows do not
/// linger in free pages, and no temporary files). A wrong key shows at the first query.
pub(crate) fn open_keyed(path: &Path, key: &DbKey) -> Result<Connection, IndexError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", key.pragma())?;
    conn.pragma_update(None, "secure_delete", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(conn)
}

/// The index at `path`, created with `recall::SCHEMA_V1` when new. The two halves hold a
/// connection each (the file is theirs alone and they never run at the same time).
pub(crate) fn open_index_file<E: Embedder>(
    path: &Path,
    key: &DbKey,
    embedder: &E,
) -> Result<Index<ExactScan>, IndexError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| IndexError::Sqlite(e.to_string()))?;
    }
    let lexical = open_keyed(path, key)?;
    let version: u32 = lexical.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let fts = Fts5::new(lexical);
    if version == 0 {
        fts.create()?;
        fts.connection().pragma_update(None, "user_version", 1)?;
    }
    let vectors = ExactScan::new(open_keyed(path, key)?, embedder.card().clone());
    Ok(Index::new(fts, vectors))
}
