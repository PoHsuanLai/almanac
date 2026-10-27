//! almanac's agent memory as plain markdown files on disk: the vaults `PlainDir` and `SealedDir`
//! (the `Vault` seam itself is in `almanac-store`), and `MemoryVault` for tests. The topic file
//! format and the fact `Store` are in `memfiles-format`.

mod dirs;
#[cfg(feature = "testing")]
mod memory;

pub use dirs::{PlainDir, SealedDir};
#[cfg(feature = "testing")]
pub use memory::MemoryVault;
