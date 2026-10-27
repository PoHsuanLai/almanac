//! `removals.toml`: the removal notes (`almanac_service::RemovalLedger`) kept in a file. The
//! file is replaced whole through `atomic-write-file` (temp file, fsync, rename), so a crash
//! leaves the old notes or the new ones.

use almanac_core::{Removal, SpaceId};
use almanac_service::{LedgerError, RemovalLedger, removals_from_toml, removals_to_toml};
use atomic_write_file::AtomicWriteFile;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The notes, in memory and in their file.
#[derive(Debug)]
pub struct FileLedger {
    path: PathBuf,
    open: Mutex<BTreeMap<SpaceId, Removal>>,
}

impl FileLedger {
    /// The notes in `path`; a file that is missing or unreadable holds none.
    pub fn load(path: PathBuf) -> Self {
        let open = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| removals_from_toml(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            open: Mutex::new(open),
        }
    }

    fn notes(&self) -> MutexGuard<'_, BTreeMap<SpaceId, Removal>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, notes: &BTreeMap<SpaceId, Removal>) -> Result<(), LedgerError> {
        let text = removals_to_toml(notes).map_err(|e| LedgerError(Box::new(e)))?;
        let mut file = AtomicWriteFile::open(&self.path).map_err(|e| LedgerError(Box::new(e)))?;
        file.write_all(text.as_bytes())
            .map_err(|e| LedgerError(Box::new(e)))?;
        file.commit().map_err(|e| LedgerError(Box::new(e)))
    }

    /// Applies `change` and rewrites the file when it changed anything.
    fn edit(
        &self,
        change: impl FnOnce(&mut BTreeMap<SpaceId, Removal>) -> bool,
    ) -> Result<(), LedgerError> {
        let mut notes = self.notes();
        if change(&mut notes) {
            self.write(&notes)
        } else {
            Ok(())
        }
    }
}

impl RemovalLedger for FileLedger {
    fn open(&self) -> Vec<(SpaceId, Removal)> {
        self.notes()
            .iter()
            .map(|(space, removal)| (space.clone(), *removal))
            .collect()
    }

    fn noted(&self, space: &SpaceId) -> Option<Removal> {
        self.notes().get(space).copied()
    }

    fn set(&self, space: &SpaceId, removal: Removal) -> Result<(), LedgerError> {
        self.edit(|notes| notes.insert(space.clone(), removal) != Some(removal))
    }

    fn strike(&self, space: &SpaceId) -> Result<(), LedgerError> {
        self.edit(|notes| notes.remove(space).is_some())
    }
}
