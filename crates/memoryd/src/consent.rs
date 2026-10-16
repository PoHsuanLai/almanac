//! The person's confirmation of loosened settings (ALM-16). The Settings app writes a value that
//! is looser than the shipped default into `settings.toml`, which any process of the same user can
//! also write; the value takes effect only once the Settings app tells memoryd the person
//! confirmed it (`Memory1.ConfirmSettings`). memoryd keeps the confirmations in `confirmed-settings.toml` in
//! its own data directory, which the agent sandbox and the apps cannot write, then reads the
//! settings again so the confirmed value applies at once.

use almanac_core::Refusal;
use almanac_service::{ConfirmError, Loaded, Locator, MemorySettings};
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

/// Where confirmations go: the settings [`Locator`] that holds the store, and the one writer.
#[derive(Debug)]
pub struct Consent {
    locator: Locator,
    writing: Mutex<()>,
}

/// The refusal a bus caller gets for a confirmation that failed.
fn refusal_of(error: ConfirmError) -> Refusal {
    match error {
        ConfirmError::Store(why) => {
            // The daemon's one log path is standard error, prefixed with its name.
            eprintln!("memoryd: could not keep the confirmation: {why}");
            Refusal::Unavailable
        }
        refused => Refusal::Invalid(refused.to_string()),
    }
}

impl Consent {
    /// Confirmations kept where `locator` says (it must have a store: `Locator::confirming`).
    pub fn new(locator: Locator) -> Self {
        Self {
            locator,
            writing: Mutex::new(()),
        }
    }

    /// Records `values` (setting key to value) and reads the settings again. Nothing is recorded
    /// when any pair is refused.
    pub fn confirm(&self, values: &BTreeMap<String, String>) -> Result<Loaded, Refusal> {
        // One writer at a time: the store is read, merged and replaced.
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        self.locator.confirm(values).map_err(refusal_of)?;
        Ok(self.locator.read(MemorySettings::default()))
    }
}
