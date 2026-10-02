//! The watch seam and `InotifyWatch`: inotify on Space roots (v1). A filesystem-wide fanotify
//! watcher is later, behind the same trait, if a privileged helper is ever accepted.

use crate::observed::Observed;
use almanac_core::SpacePath;
use std::future::Future;

/// Why watching failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WatchError {
    /// The root does not exist or cannot be watched.
    #[error("cannot watch {0}")]
    Unwatchable(String),
    /// The kernel's watch limit is reached.
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

/// inotify through notify 8.2.
#[derive(Debug)]
pub struct InotifyWatch {
    watcher: notify::RecommendedWatcher,
}

impl InotifyWatch {
    /// A watcher with no roots.
    pub fn new() -> Result<Self, WatchError> {
        todo!(
            "notify::recommended_watcher with a channel sink; map events to Observed, pairing MOVED_FROM and MOVED_TO by cookie"
        )
    }

    /// The underlying watcher.
    pub fn inner(&self) -> &notify::RecommendedWatcher {
        &self.watcher
    }
}

impl FileWatch for InotifyWatch {
    fn watch(&mut self, root: &SpacePath) -> Result<(), WatchError> {
        let _ = root;
        todo!("Watcher::watch(root, RecursiveMode::Recursive); LimitReached on ENOSPC")
    }

    fn unwatch(&mut self, root: &SpacePath) -> Result<(), WatchError> {
        let _ = root;
        todo!("Watcher::unwatch(root)")
    }

    async fn next(&mut self) -> Option<Observed> {
        todo!(
            "await the channel; stat for the inode; digest small files; resolve the pid to an app when possible"
        )
    }
}
