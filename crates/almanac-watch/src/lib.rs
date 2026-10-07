//! File-change capture: the `FileWatch` seam, `InotifyWatch` (notify 8.2, feature `linux`), and
//! the pure `join` of observed changes with the reasons apps supply.
//!
//! Everything but `InotifyWatch` is portable and builds with `--no-default-features`. The
//! `linux` feature (default on) adds the notify-backed watcher, whose rename pairing and
//! ownership stamps assume inotify and Unix metadata.

#[cfg(feature = "linux")]
mod inotify;
mod join;
mod observed;
mod seam;
#[cfg(feature = "linux")]
mod translate;

#[cfg(feature = "linux")]
pub use inotify::InotifyWatch;
pub use join::{JoinStep, Pending, change_for, join};
pub use observed::{Alias, FileEvent, JoinWindow, Joined, Observed, Stamp, WhyAt};
pub use seam::{FileWatch, WatchError};
