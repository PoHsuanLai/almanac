//! File-change capture: the `FileWatch` seam, `InotifyWatch` (notify 8.2), and the pure `join` of observed changes with the reasons apps supply.
//!
//! Linux only. Everything but `InotifyWatch` is pure.

mod inotify;
mod join;
mod observed;
mod translate;

pub use inotify::{FileWatch, InotifyWatch, WatchError};
pub use join::{JoinStep, Pending, change_for, join};
pub use observed::{Alias, FileEvent, JoinWindow, Joined, Observed, Stamp, WhyAt};
