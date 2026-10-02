//! The watch seam and `InotifyWatch`: inotify on Space roots (v1). A filesystem-wide fanotify
//! watcher is later, behind the same trait, if a privileged helper is ever accepted.

use crate::observed::Observed;
use crate::translate::{Inbox, now};
use almanac_core::SpacePath;
use notify::{RecursiveMode, Watcher};
use std::future::{Future, poll_fn};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::Poll;
use std::time::Duration;

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

/// inotify through notify 8.2.
///
/// Observations are queued by notify's own thread and handed out by [`FileWatch::next`], which
/// needs no runtime: it parks the task's waker in the queue. The stream never closes while the
/// watcher lives; dropping the watcher ends it.
#[derive(Debug)]
pub struct InotifyWatch {
    watcher: notify::RecommendedWatcher,
    inbox: Arc<Mutex<Inbox>>,
}

/// How long a `MOVED_FROM` waits for its `MOVED_TO` before it counts as a deletion.
const PAIRING_GRACE: Duration = Duration::from_millis(100);

fn locked(inbox: &Mutex<Inbox>) -> MutexGuard<'_, Inbox> {
    inbox.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wake(inbox: &mut Inbox) {
    if let Some(waker) = inbox.waker.take() {
        waker.wake();
    }
}

fn backend_error(err: notify::Error) -> WatchError {
    match err.kind {
        notify::ErrorKind::MaxFilesWatch => WatchError::LimitReached,
        notify::ErrorKind::Io(ref io) if matches!(io.raw_os_error(), Some(24 | 28)) => {
            WatchError::LimitReached
        }
        notify::ErrorKind::PathNotFound | notify::ErrorKind::WatchNotFound => {
            WatchError::Unwatchable(err.to_string())
        }
        _ => WatchError::Backend(err.to_string()),
    }
}

impl InotifyWatch {
    /// A watcher with no roots.
    pub fn new() -> Result<Self, WatchError> {
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let sink = Arc::clone(&inbox);
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            let mut guard = locked(&sink);
            guard.accept(event, now());
            let unpaired = guard.has_unpaired();
            wake(&mut guard);
            drop(guard);
            if unpaired {
                let later = Arc::clone(&sink);
                std::thread::spawn(move || {
                    std::thread::sleep(PAIRING_GRACE);
                    let mut guard = locked(&later);
                    guard.flush_unpaired(now());
                    wake(&mut guard);
                });
            }
        })
        .map_err(backend_error)?;
        Ok(Self { watcher, inbox })
    }

    /// The underlying watcher.
    pub fn inner(&self) -> &notify::RecommendedWatcher {
        &self.watcher
    }
}

impl FileWatch for InotifyWatch {
    fn watch(&mut self, root: &SpacePath) -> Result<(), WatchError> {
        self.watcher
            .watch(Path::new(root.as_str()), RecursiveMode::Recursive)
            .map_err(backend_error)
    }

    fn unwatch(&mut self, root: &SpacePath) -> Result<(), WatchError> {
        self.watcher
            .unwatch(Path::new(root.as_str()))
            .map_err(backend_error)
    }

    async fn next(&mut self) -> Option<Observed> {
        poll_fn(|cx| {
            let mut inbox = locked(&self.inbox);
            match inbox.queue.pop_front() {
                Some(observed) => Poll::Ready(Some(observed)),
                None => {
                    inbox.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await
    }
}
