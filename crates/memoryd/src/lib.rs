//! memoryd's library half: the real seams (`SystemBackend`, `SystemClock`, the inferd-backed
//! `InferdEmbedder` and `InferdConsolidator`) and the XDG roots, so every module is testable
//! without a bus. The binary is a skeleton: it exits saying so.

mod backend;
mod clock;
mod infer;
mod xdg;

pub use almanac_dbus::{MEMORY_BUS, MEMORY_PATH};
pub use almanac_watch::InotifyWatch as Watcher;
pub use backend::{SpaceVault, SystemBackend};
pub use clock::SystemClock;
pub use infer::{InferdConsolidator, InferdEmbedder};
pub use xdg::{XdgError, dirs_from, dirs_from_env};
