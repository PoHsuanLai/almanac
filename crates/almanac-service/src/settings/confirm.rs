//! The person's confirmations of loosened settings (ALM-16).
//!
//! `settings.toml` is writable by any process of the same user, so a value looser than the
//! shipped default (`loosen.rs`) takes effect only when the person confirmed exactly that value in
//! the Settings app, which says so to memoryd (`Memory1.ConfirmSettings`). memoryd keeps the
//! confirmations in `confirmed-settings.toml` in its own data directory, which the agent sandbox
//! and the apps cannot write. This module is the pure half: the confirmations as a value, their
//! text, the check of what a confirmation may say and the gate over a read. [`Locator`] reads and
//! writes the file.
//!
//! [`Locator`]: super::Locator

use super::MemorySettings;
use super::keys::{Rule, table};
use super::loosen::{loose, loosenable, shipped_text};
use super::read::{Fallback, Loaded, Why};
use almanac_core::WaitingSetting;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The file the confirmations live in, in memoryd's data directory.
pub const CONFIRMED_FILE: &str = "confirmed-settings.toml";

/// Why a confirmation was not taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfirmError {
    /// The key is not one that can loosen a shipped default.
    #[error("{0}: no setting by that name can be confirmed")]
    UnknownKey(String),
    /// The value is not one the key takes.
    #[error("{key}: {why}")]
    BadValue {
        /// The key.
        key: String,
        /// Why the value is refused.
        why: Why,
    },
    /// The store could not be read as a table or written.
    #[error("confirmed-settings.toml: {0}")]
    Store(String),
}

/// What the person confirmed: setting key to the value, as the app wrote it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmations {
    #[serde(default)]
    confirmed: BTreeMap<String, String>,
}

/// The canonical text of `value` for `key`: a number in decimal, a word as it is. Refused when
/// `key` cannot loosen a default or the key does not take `value`.
fn canonical(key: &str, value: &str) -> Result<String, ConfirmError> {
    let unknown = || ConfirmError::UnknownKey(key.to_owned());
    let bad = |why| ConfirmError::BadValue {
        key: key.to_owned(),
        why,
    };
    if !loosenable(key) {
        return Err(unknown());
    }
    let found = table()
        .into_iter()
        .find(|k| k.path == key)
        .ok_or_else(unknown)?;
    match found.rule {
        Rule::Number { range, .. } => match value.trim().parse::<i64>() {
            Err(_) => Err(bad(Why::NotANumber)),
            Ok(n) if range.contains(&n) => Ok(n.to_string()),
            Ok(_) => Err(bad(Why::OutOfRange {
                min: *range.start(),
                max: *range.end(),
            })),
        },
        Rule::Word { words, .. } => match words.contains(&value) {
            true => Ok(value.to_owned()),
            false => Err(bad(Why::NotAWord { words })),
        },
    }
}

impl Confirmations {
    /// The confirmations in the store's text.
    pub fn parse(text: &str) -> Result<Self, ConfirmError> {
        toml::from_str(text).map_err(|e| ConfirmError::Store(e.to_string()))
    }

    /// The store's text.
    pub fn to_toml(&self) -> Result<String, ConfirmError> {
        toml::to_string(self).map_err(|e| ConfirmError::Store(e.to_string()))
    }

    /// Whether the person confirmed exactly `value` for `key` (`value` in canonical text).
    pub fn holds(&self, key: &str, value: &str) -> bool {
        self.confirmed.get(key).is_some_and(|have| have == value)
    }

    /// These confirmations with `values` added over them. Every pair is checked first: one that is
    /// refused refuses them all and nothing is added.
    pub fn with(&self, values: &BTreeMap<String, String>) -> Result<Self, ConfirmError> {
        let checked = values
            .iter()
            .map(|(key, value)| canonical(key, value).map(|v| (key.clone(), v)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut next = self.clone();
        next.confirmed.extend(checked);
        Ok(next)
    }
}

/// `loaded` with each value looser than the shipped default that `confirmed` does not hold put
/// back to the shipped value, and reported as [`Why::NotConfirmed`]. A value that is not looser
/// needs nothing; a confirmation for a value the file no longer has is not used.
pub fn gate(loaded: Loaded, confirmed: &Confirmations) -> Loaded {
    let shipped = MemorySettings::default();
    let mut value = loaded.value;
    let mut fallbacks = loaded.fallbacks;
    for l in loose(&loaded.value) {
        if !confirmed.holds(l.key, &l.now) {
            (l.restore)(&mut value, &shipped);
            fallbacks.push(Fallback {
                key: l.key.to_owned(),
                why: Why::NotConfirmed { value: l.now },
            });
        }
    }
    Loaded {
        value,
        fallbacks,
        unknown: loaded.unknown,
    }
}

/// The values of `gated` (a read already put through [`gate`]) that wait for the person's
/// confirmation, each with the shipped value in force meanwhile, in key order.
pub fn waiting(gated: &Loaded) -> Vec<WaitingSetting> {
    let mut waiting: Vec<WaitingSetting> = gated
        .fallbacks
        .iter()
        .filter_map(|f| match &f.why {
            Why::NotConfirmed { value } => Some(WaitingSetting {
                key: f.key.clone(),
                waiting: value.clone(),
                shipped: shipped_text(&f.key)?,
            }),
            _ => None,
        })
        .collect();
    waiting.sort_by(|a, b| a.key.cmp(&b.key));
    waiting
}
