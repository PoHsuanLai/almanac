//! The removal notes over an in-memory ledger: what is noted, which choice stands when no one
//! asked, and the file format with its older shape.

use crate::support::space;
use almanac_core::{HistoryFate, MemoryFate, Removal, SpaceId};
use almanac_tidy::{LedgerError, RemovalLedger, Removals, removals_from_toml, removals_to_toml};
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

#[derive(Default)]
struct MemLedger(Mutex<BTreeMap<SpaceId, Removal>>);

impl MemLedger {
    fn notes(&self) -> std::sync::MutexGuard<'_, BTreeMap<SpaceId, Removal>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl RemovalLedger for MemLedger {
    fn open(&self) -> Vec<(SpaceId, Removal)> {
        self.notes().iter().map(|(s, r)| (s.clone(), *r)).collect()
    }
    fn noted(&self, space: &SpaceId) -> Option<Removal> {
        self.notes().get(space).copied()
    }
    fn set(&self, space: &SpaceId, removal: Removal) -> Result<(), LedgerError> {
        self.notes().insert(space.clone(), removal);
        Ok(())
    }
    fn strike(&self, space: &SpaceId) -> Result<(), LedgerError> {
        self.notes().remove(space);
        Ok(())
    }
}

const DELETE_HISTORY: Removal = Removal {
    memories: MemoryFate::MoveToApps,
    history: HistoryFate::Delete,
};

#[test]
fn a_note_stands_until_it_is_struck_out() {
    let notes = Removals::new(MemLedger::default());
    assert!(notes.open().is_empty());
    notes.begin(&space("old"), Removal::KEEP_ALL).expect("note");
    notes.begin(&space("old"), DELETE_HISTORY).expect("replace");
    assert_eq!(notes.open(), vec![(space("old"), DELETE_HISTORY)]);
    notes.finish(&space("old")).expect("strike");
    assert!(notes.open().is_empty());
}

#[test]
fn a_removal_no_one_asked_about_keeps_everything_unless_a_choice_was_noted() {
    let notes = Removals::new(MemLedger::default());
    let (removal, noted) = notes.begin_unasked(&space("new"));
    assert_eq!(removal, Removal::KEEP_ALL);
    assert!(noted.is_ok());
    assert_eq!(notes.open(), vec![(space("new"), Removal::KEEP_ALL)]);

    notes.begin(&space("asked"), DELETE_HISTORY).expect("note");
    let (removal, _) = notes.begin_unasked(&space("asked"));
    assert_eq!(removal, DELETE_HISTORY, "the person's choice stands");
}

#[test]
fn the_file_round_trips_and_an_older_one_reads_as_keep_everything() {
    let notes = BTreeMap::from([
        (space("a"), Removal::KEEP_ALL),
        (space("b"), DELETE_HISTORY),
    ]);
    let text = removals_to_toml(&notes).expect("encode");
    assert_eq!(removals_from_toml(&text).expect("decode"), notes);
    assert_eq!(
        removals_from_toml("removed = [\"old\"]\n").expect("older file"),
        BTreeMap::from([(space("old"), Removal::KEEP_ALL)])
    );
    assert!(removals_from_toml("removed = 3").is_err());
}
