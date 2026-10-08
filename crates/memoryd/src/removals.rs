//! The desktop-wide Spaces whose removal memoryd has heard of and not yet finished settling.
//!
//! The note is written before the first memory moves and struck out after the last, so a daemon
//! that dies half way finds it on the next start and finishes (the move is safe to repeat).

use almanac_core::SpaceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

/// `removals.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RemovalsFile {
    removed: Vec<SpaceId>,
}

/// The notes, in memory and in their file.
#[derive(Debug)]
pub struct Removals {
    path: PathBuf,
    open: Mutex<BTreeSet<SpaceId>>,
}

impl Removals {
    /// The notes in `path`; a file that is missing or unreadable holds none.
    pub fn load(path: PathBuf) -> Self {
        let open = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| toml::from_str::<RemovalsFile>(&text).ok())
            .map(|file| file.removed.into_iter().collect())
            .unwrap_or_default();
        Self {
            path,
            open: Mutex::new(open),
        }
    }

    /// The Spaces still to settle.
    pub fn open(&self) -> Vec<SpaceId> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// Notes that `space` is being settled (written to the file before this returns).
    pub fn begin(&self, space: &SpaceId) {
        self.edit(|open| open.insert(space.clone()));
    }

    /// Strikes `space` out: its memories are settled.
    pub fn finish(&self, space: &SpaceId) {
        self.edit(|open| open.remove(space));
    }

    fn edit(&self, change: impl FnOnce(&mut BTreeSet<SpaceId>) -> bool) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if !change(&mut open) {
            return;
        }
        let file = RemovalsFile {
            removed: open.iter().cloned().collect(),
        };
        let written = toml::to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                let staging = self.path.with_extension("toml.new");
                std::fs::write(&staging, text)
                    .and_then(|()| std::fs::rename(&staging, &self.path))
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = written {
            // The daemon's one log path is standard error, prefixed with its name.
            eprintln!("memoryd: could not write {}: {e}", self.path.display());
        }
    }
}
