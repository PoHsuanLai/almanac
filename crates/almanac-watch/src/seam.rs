//! The watch seam: `FileWatch` and `WatchError`. Portable; a backend (`InotifyWatch` on Linux,
//! behind the `linux` feature) implements it.

use crate::observed::Observed;
use almanac_core::SpacePath;
use std::future::Future;

/// Why watching failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WatchError {
    /// The root does not exist or cannot be watched.
    #[error("cannot watch {0}")]
    Unwatchable(String),
    /// The kernel's watch or instance limit is reached.
    #[error("watch limit reached")]
    LimitReached,
    /// The backend failed.
    #[error("watcher: {0}")]
    Backend(String),
}

/// A source of file observations.
pub trait FileWatch: Send {
    /// Starts watching a root, recursively.
    fn watch(&mut self, root: &SpacePath) -> Result<(), WatchError>;
    /// Stops watching a root.
    fn unwatch(&mut self, root: &SpacePath) -> Result<(), WatchError>;
    /// The next change, with rename halves already paired; `None` when the watcher is closed.
    fn next(&mut self) -> impl Future<Output = Option<Observed>> + Send;
}
