//! almanac's agent memory as plain markdown files on disk: the vaults `PlainDir` and `SealedDir`,
//! `SpaceVault` (one or the other, by the Space's choice; the `Vault` seam itself is in
//! `almanac-store`), and `MemoryVault` for tests. The topic file format and the fact `Store` are
//! in `memfiles-format`.

mod dirs;
#[cfg(feature = "testing")]
mod memory;
mod space_vault;

pub use dirs::{PlainDir, SealedDir};
#[cfg(feature = "testing")]
pub use memory::MemoryVault;
pub use space_vault::SpaceVault;
