//! almanac's per-Space event log: typed, append-only and hash-chained, and also the audit log.
//!
//! The chain commits to a keyed digest of each body, not the body: forgetting erases bodies
//! and their `things` rows while headers and the chain stay verifiable. Old prefixes can be
//! pruned behind a checkpoint.

mod chain;
mod filter;
mod header;
#[cfg(feature = "testing")]
mod memory;
mod rows;
mod sqlite;
mod traits;

pub use chain::{BodyState, Entry, verify_chain};
pub use filter::passes;
pub use header::{
    GENESIS_CONTEXT, HEADER_MAGIC, Header, NewHeader, body_digest, genesis_link, header_bytes, link,
};
#[cfg(feature = "testing")]
pub use memory::MemoryLog;
pub use sqlite::{SCHEMA_V1, SCHEMA_VERSION, SqliteLog};
pub use traits::{LogError, LogRead, LogWrite, PageQuery, RoleFilter};
