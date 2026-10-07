//! `almanac-local`: the portable on-disk `Backend` for an app that hosts its own memory
//! (design/36-PORTABLE-CORE.md). mailo on macOS or Windows opens a [`LocalBackend`] over a
//! root directory it owns, with the master key it provides, and serves
//! `Memory::over(InProcess::new(service, caller))` from it: no daemon, no bus, no Secret
//! Service, no inotify, no Landlock, no XDG lookups.
//!
//! What it wires together: `SqliteLog` (eventlog), `SealedDir` or `PlainDir` (memfiles),
//! `ExactScan` over FTS5 (recall) and `ProvidedKeys` (almanac-seal). The embedder and the
//! consolidator are the app's (generic parameters, defaulting to [`NoEmbedder`], which leaves
//! recall lexical-only, and [`NoConsolidator`]); the clock is a parameter too, and [`WallClock`] is the
//! ready-made one an app passes when it has no need of its own. Nothing else here reads the
//! environment, the wall clock or the system's directories.

mod backend;
mod clock;
mod host;
mod index;
mod none;
mod root;
mod vault;

pub use backend::LocalBackend;
pub use clock::WallClock;
pub use host::{LocalError, create_space, open, save_spaces};
pub use none::{NoConsolidator, NoEmbedder};
pub use root::Root;
pub use vault::LocalVault;
