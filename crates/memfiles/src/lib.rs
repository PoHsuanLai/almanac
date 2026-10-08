//! almanac's agent memory as plain markdown files: the topic file format (frozen, with parse
//! and render built), the vault seam (`PlainDir`, `SealedDir`, `MemoryVault`), and the fact
//! `Store` over a vault.

mod dirs;
#[cfg(feature = "testing")]
mod memory;
mod store;
mod topic;
mod trailer;
mod vault;

pub use dirs::{PlainDir, SealedDir};
#[cfg(feature = "testing")]
pub use memory::MemoryVault;
pub use store::{MemfilesError, PRIMER_MAX_LINES, Primer, PrimerEntry, Store};
pub use topic::{Block, FORMAT_LINE, ParseError, TopicFile, parse_topic, render_topic};
pub use trailer::{TrailerFault, parse_trailer, render_trailer};
pub use vault::{FACTS_DIR, PENDING_DIR, PROCEDURES_DIR, Vault, VaultError, VaultPath};
