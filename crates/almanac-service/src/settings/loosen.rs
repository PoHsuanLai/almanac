//! What the settings file loosens below the shipped defaults (ALM-16).
//!
//! The file is writable by any process of the same user, and nothing authenticates it, so a value
//! that keeps less, keeps it for a shorter time or holds files less safely than the shipped
//! default takes effect only once the person confirmed exactly that value in the Settings app
//! (`confirm.rs`: memoryd keeps the confirmations where the agent sandbox cannot write). This
//! module finds the loose values; each is also reported by name on every read (at start and on
//! each reload), confirmed or not.

use super::{ConsolidateApply, MemorySettings};
use almanac_core::{DayCount, VaultKind};

/// `memory.files.at_rest`.
const AT_REST: &str = "memory.files.at_rest";
/// `memory.consolidation.apply`.
const APPLY: &str = "memory.consolidation.apply";

/// One day-count key, how to read its value from a [`MemorySettings`] and how to put the shipped
/// value back (the second argument holds the shipped settings).
struct Days {
    key: &'static str,
    of: fn(&MemorySettings) -> DayCount,
    restore: fn(&mut MemorySettings, &MemorySettings),
}

const DAY_KEYS: [Days; 2] = [
    Days {
        key: "memory.retention.search_days",
        of: |s| s.retention.search,
        restore: |s, o| s.retention.search = o.retention.search,
    },
    Days {
        key: "memory.retention.session_days",
        of: |s| s.retention.session,
        restore: |s, o| s.retention.session = o.retention.session,
    },
];

/// One value looser than the shipped default.
pub(crate) struct Loose {
    /// The dotted key.
    pub key: &'static str,
    /// What it is, for the log.
    pub said: String,
    /// The value as a confirmation holds it: the number in decimal, or the word.
    pub now: String,
    /// Puts the shipped value of this key back.
    pub restore: fn(&mut MemorySettings, &MemorySettings),
}

/// The values of `value` that are looser than the shipped defaults.
pub(crate) fn loose(value: &MemorySettings) -> Vec<Loose> {
    let shipped = MemorySettings::default();
    let mut out: Vec<Loose> = DAY_KEYS
        .iter()
        .filter(|d| (d.of)(value) < (d.of)(&shipped))
        .map(|d| Loose {
            key: d.key,
            said: format!(
                "{}: {} days, shorter than the shipped {}",
                d.key,
                (d.of)(value).0,
                (d.of)(&shipped).0
            ),
            now: (d.of)(value).0.to_string(),
            restore: d.restore,
        })
        .collect();
    if value.at_rest != shipped.at_rest && value.at_rest == VaultKind::Plain {
        out.push(Loose {
            key: AT_REST,
            said: "memory.files.at_rest: plain, new Spaces keep readable files".to_owned(),
            now: "plain".to_owned(),
            restore: |s, o| s.at_rest = o.at_rest,
        });
    }
    if value.apply != shipped.apply && value.apply == ConsolidateApply::Auto {
        out.push(Loose {
            key: APPLY,
            said: "memory.consolidation.apply: auto, runs apply without review".to_owned(),
            now: "auto".to_owned(),
            restore: |s, o| s.apply = o.apply,
        });
    }
    out
}

/// The shipped value of `key` as a confirmation names it (a number in decimal, or the word), for
/// a key that can loosen a default.
pub(crate) fn shipped_text(key: &str) -> Option<String> {
    let shipped = MemorySettings::default();
    let text = match key {
        AT_REST => match shipped.at_rest {
            VaultKind::Plain => "plain".to_owned(),
            _ => "sealed".to_owned(),
        },
        APPLY => match shipped.apply {
            ConsolidateApply::Auto => "auto".to_owned(),
            ConsolidateApply::Review => "review".to_owned(),
        },
        day => (DAY_KEYS.iter().find(|d| d.key == day)?.of)(&shipped)
            .0
            .to_string(),
    };
    Some(text)
}

/// Whether `key` is one that can loosen a shipped default, so one a confirmation may name.
pub(crate) fn loosenable(key: &str) -> bool {
    key == AT_REST || key == APPLY || DAY_KEYS.iter().any(|d| d.key == key)
}

/// The keys of `value` that are looser than the shipped defaults, each with a short description.
pub fn loosenings(value: &MemorySettings) -> Vec<String> {
    loose(value).into_iter().map(|l| l.said).collect()
}
