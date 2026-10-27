//! The desktop-wide Spaces whose removal is not yet finished settling, and what the person chose
//! for each (its memories and its history).
//!
//! The note is written before the first memory moves and struck out after the last, so a daemon
//! that dies half way finds it on the next start and finishes the way the person chose (the move
//! is safe to repeat). A note without a choice (an older file, or a removal no one asked the
//! person about) is the non-destructive one: [`Removal::KEEP_ALL`].
//!
//! This module holds what is recorded and when a note is honoured; where the notes are kept is
//! the [`RemovalLedger`] seam, which the daemon implements over a file.

use almanac_core::{Removal, SpaceId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Why the text of the removal notes could not be made or read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotesFileError {
    /// The notes could not be written as TOML.
    #[error("encode: {0}")]
    Encode(String),
    /// The text is not a valid notes file.
    #[error("decode: {0}")]
    Decode(String),
}

/// Why the ledger could not keep a note.
#[derive(Debug, thiserror::Error)]
#[error("could not keep the removal notes: {0}")]
pub struct LedgerError(#[source] pub Box<dyn std::error::Error + Send + Sync>);

/// Where the notes are kept. Shared by reference, so an implementation does its own locking; it
/// writes a note down before `set` and `strike` return.
pub trait RemovalLedger {
    /// Every Space still to settle, with the choice noted for it.
    fn open(&self) -> Vec<(SpaceId, Removal)>;
    /// The choice noted for `space`, if any.
    fn noted(&self, space: &SpaceId) -> Option<Removal>;
    /// Notes `removal` for `space`, replacing an earlier choice.
    fn set(&self, space: &SpaceId, removal: Removal) -> Result<(), LedgerError>;
    /// Strikes `space` out.
    fn strike(&self, space: &SpaceId) -> Result<(), LedgerError>;
}

/// The removal notes over a ledger.
#[derive(Debug)]
pub struct Removals<L> {
    ledger: L,
}

impl<L: RemovalLedger> Removals<L> {
    /// The notes kept in `ledger`.
    pub fn new(ledger: L) -> Self {
        Self { ledger }
    }

    /// The Spaces still to settle, with the choice for each.
    pub fn open(&self) -> Vec<(SpaceId, Removal)> {
        self.ledger.open()
    }

    /// Notes that `space` is being settled as the person chose; a choice noted earlier for it is
    /// replaced.
    pub fn begin(&self, space: &SpaceId, removal: Removal) -> Result<(), LedgerError> {
        self.ledger.set(space, removal)
    }

    /// Notes that `space` is being settled with no one asked: a choice already noted stands,
    /// else [`Removal::KEEP_ALL`]. Answers the choice in force, and the failure to note it if
    /// there was one (the choice is in force for this run all the same).
    pub fn begin_unasked(&self, space: &SpaceId) -> (Removal, Result<(), LedgerError>) {
        match self.ledger.noted(space) {
            Some(noted) => (noted, Ok(())),
            None => (Removal::KEEP_ALL, self.ledger.set(space, Removal::KEEP_ALL)),
        }
    }

    /// Strikes `space` out: its memories are settled.
    pub fn finish(&self, space: &SpaceId) -> Result<(), LedgerError> {
        self.ledger.strike(space)
    }
}

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

/// The notes in `removals.toml` text.
pub fn removals_from_toml(text: &str) -> Result<BTreeMap<SpaceId, Removal>, NotesFileError> {
    let file: RemovalsFile =
        toml::from_str(text).map_err(|e| NotesFileError::Decode(e.to_string()))?;
    Ok(file.removed.into_iter().map(Note::into_pair).collect())
}

/// `notes` as `removals.toml` text.
pub fn removals_to_toml(notes: &BTreeMap<SpaceId, Removal>) -> Result<String, NotesFileError> {
    let file = RemovalsFile {
        removed: notes
            .iter()
            .map(|(space, removal)| Note::Chosen {
                space: space.clone(),
                removal: *removal,
            })
            .collect(),
    };
    toml::to_string(&file).map_err(|e| NotesFileError::Encode(e.to_string()))
}
