//! The desktop-wide Spaces whose removal memoryd has heard of and not yet finished settling, and
//! what the person chose for each (its memories and its history).
//!
//! The note is written before the first memory moves and struck out after the last, so a daemon
//! that dies half way finds it on the next start and finishes the way the person chose (the move
//! is safe to repeat). A note without a choice (an older file, or a removal no one asked the
//! person about) is the non-destructive one: [`Removal::KEEP_ALL`].

use almanac_core::{Removal, SpaceId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

/// One line of `removals.toml`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Note {
    /// A Space and the choice made for it.
    Chosen { space: SpaceId, removal: Removal },
    /// A Space alone (the file as it was before the history choice).
    Bare(SpaceId),
}

impl Note {
    fn into_pair(self) -> (SpaceId, Removal) {
        match self {
            Note::Chosen { space, removal } => (space, removal),
            Note::Bare(space) => (space, Removal::KEEP_ALL),
        }
    }
}

/// `removals.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RemovalsFile {
    removed: Vec<Note>,
}

/// The notes, in memory and in their file.
#[derive(Debug)]
pub struct Removals {
    path: PathBuf,
    open: Mutex<BTreeMap<SpaceId, Removal>>,
}

impl Removals {
    /// The notes in `path`; a file that is missing or unreadable holds none.
    pub fn load(path: PathBuf) -> Self {
        let open = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| toml::from_str::<RemovalsFile>(&text).ok())
            .map(|file| file.removed.into_iter().map(Note::into_pair).collect())
            .unwrap_or_default();
        Self {
            path,
            open: Mutex::new(open),
        }
    }

    /// The Spaces still to settle, with the choice for each.
    pub fn open(&self) -> Vec<(SpaceId, Removal)> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(space, removal)| (space.clone(), *removal))
            .collect()
    }

    /// Notes that `space` is being settled as the person chose (written to the file before this
    /// returns); a choice noted earlier for it is replaced.
    pub fn begin(&self, space: &SpaceId, removal: Removal) {
        self.edit(|open| open.insert(space.clone(), removal) != Some(removal));
    }

    /// Notes that `space` is being settled with no one asked: a choice already noted stands,
    /// else [`Removal::KEEP_ALL`]. Answers the choice in force.
    pub fn begin_unasked(&self, space: &SpaceId) -> Removal {
        let noted = self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(space)
            .copied();
        noted.unwrap_or_else(|| {
            self.begin(space, Removal::KEEP_ALL);
            Removal::KEEP_ALL
        })
    }

    /// Strikes `space` out: its memories are settled.
    pub fn finish(&self, space: &SpaceId) {
        self.edit(|open| open.remove(space).is_some());
    }

    fn edit(&self, change: impl FnOnce(&mut BTreeMap<SpaceId, Removal>) -> bool) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if !change(&mut open) {
            return;
        }
        let file = RemovalsFile {
            removed: open
                .iter()
                .map(|(space, removal)| Note::Chosen {
                    space: space.clone(),
                    removal: *removal,
                })
                .collect(),
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
