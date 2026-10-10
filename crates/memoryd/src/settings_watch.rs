//! The settings, live (design/22 section 2): a `notify` watch on the directory that holds
//! `almanac/settings.toml` (not on the file: the Settings app's atomic writer and every editor replace
//! the inode by rename), a 30 ms debounce, then the whole file is read again through the lenient
//! reader.

use almanac_service::{Backend, Loaded, Locator, MemoryService, MemorySettings, SETTINGS_FILE};
use porter_daemon::Watch;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// How long a burst of events settles before the file is read (design/22 section 2).
pub const DEBOUNCE: Duration = Duration::from_millis(30);

/// Whether a [`SettingsWatch`] is looking at the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchState {
    /// Every settled change arrives through [`SettingsWatch::changed`].
    Live,
    /// The directory could not be watched: the settings stay as first read until a restart.
    Blind {
        /// Why the watch could not start.
        reason: String,
    },
}

/// A running watch on the settings file. Dropping it stops the watch.
#[derive(Debug)]
pub struct SettingsWatch {
    changes: watch::Receiver<Loaded>,
    state: WatchState,
    _watcher: Option<Watch>,
}

/// Reads again after every settled burst of events and publishes, until the watch is dropped.
async fn settle(
    locator: Locator,
    base: MemorySettings,
    mut events: mpsc::UnboundedReceiver<()>,
    out: watch::Sender<Loaded>,
) {
    while events.recv().await.is_some() {
        // Every further event restarts the window: a rename can fire more than one.
        loop {
            tokio::select! {
                () = tokio::time::sleep(DEBOUNCE) => break,
                more = events.recv() => if more.is_none() { return },
            }
        }
        if out.send(locator.read(base)).is_err() {
            return;
        }
    }
}

impl SettingsWatch {
    /// Reads the file now over `base` and starts watching it. Must run inside a tokio runtime.
    pub fn start(locator: Locator, base: MemorySettings) -> Self {
        let initial = locator.read(base);
        let (out, changes) = watch::channel(initial);
        let (signal, events) = mpsc::unbounded_channel();
        let started = locator
            .watch_dir()
            .ok_or_else(|| "no configuration directory".to_owned())
            .and_then(|dir| {
                let file = std::path::Path::new(SETTINGS_FILE)
                    .file_name()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| "no settings file name".to_owned())?;
                Watch::start(dir, file, move || {
                    // The receiver is gone once the watch ended; nobody is left to tell.
                    let _ = signal.send(());
                })
                .map_err(|error| error.to_string())
            });
        match started {
            Ok(watch) => {
                tokio::spawn(settle(locator, base, events, out));
                Self {
                    changes,
                    state: WatchState::Live,
                    _watcher: Some(watch),
                }
            }
            Err(reason) => Self {
                changes,
                state: WatchState::Blind { reason },
                _watcher: None,
            },
        }
    }

    /// The file as last read.
    pub fn current(&self) -> Loaded {
        self.changes.borrow().clone()
    }

    /// Whether the directory is being watched.
    pub fn state(&self) -> &WatchState {
        &self.state
    }

    /// Waits for the next settled change: the whole file read again. `None` once the watch has
    /// stopped (or was never live).
    pub async fn changed(&mut self) -> Option<Loaded> {
        self.changes.changed().await.ok()?;
        Some(self.changes.borrow_and_update().clone())
    }
}

/// Logs what a read refused, then puts its values in force on `service`.
pub fn apply<B: Backend>(service: &MemoryService<B>, loaded: &Loaded) {
    for line in loaded.lines("memoryd") {
        eprintln!("{line}");
    }
    service.apply_settings(loaded.value);
}

/// Waits for the next change of the file and applies it to `service`; `None` when the watch ended.
pub async fn apply_next<B: Backend>(
    service: &MemoryService<B>,
    settings: &mut SettingsWatch,
) -> Option<Loaded> {
    let loaded = settings.changed().await?;
    apply(service, &loaded);
    Some(loaded)
}
