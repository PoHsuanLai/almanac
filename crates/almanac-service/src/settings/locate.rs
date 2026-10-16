//! Where the settings file is: `$XDG_CONFIG_HOME/almanac/settings.toml`, then each of
//! `$XDG_CONFIG_DIRS` (design/22 section 2). The first file that reads wins.

use super::confirm::{ConfirmError, Confirmations, gate};
use super::read::{Fallback, Loaded, Why, read};
use super::{MemorySettings, SETTINGS_FILE};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The configuration directories, in order of precedence, and the store of the person's
/// confirmations (ALM-16) that gates what a read lets through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locator {
    dirs: Vec<PathBuf>,
    confirmations: Option<PathBuf>,
}

impl Locator {
    /// The directories named by `XDG_CONFIG_HOME` (else `$HOME/.config`) and `XDG_CONFIG_DIRS`
    /// (else `/etc/xdg`).
    pub fn from_env(env: &impl Fn(&str) -> Option<String>) -> Self {
        let home = env("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env("HOME").unwrap_or_default()).join(".config"));
        let rest = env("XDG_CONFIG_DIRS")
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/etc/xdg".to_owned());
        Self {
            dirs: std::iter::once(home)
                .chain(rest.split(':').map(PathBuf::from))
                .collect(),
            confirmations: None,
        }
    }

    /// This locator with the confirmations kept in `file`. Without one, nothing is confirmed and a
    /// value looser than the shipped default never takes effect.
    pub fn confirming(mut self, file: PathBuf) -> Self {
        self.confirmations = Some(file);
        self
    }

    /// The configuration directories, the person's first.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// The directory a live watch looks at: the person's own `almanac` directory, where the
    /// Settings app writes.
    pub fn watch_dir(&self) -> Option<PathBuf> {
        let person = self.dirs.first()?;
        Path::new(SETTINGS_FILE)
            .parent()
            .map(|name| person.join(name))
    }

    /// The first settings file that can be read, as text.
    fn text(&self) -> Option<String> {
        self.dirs
            .iter()
            .map(|d| d.join(SETTINGS_FILE))
            .find_map(|path| std::fs::read_to_string(path).ok())
    }

    /// The confirmations as stored now; a store that is not there is nothing confirmed, a store
    /// that does not read is nothing confirmed and a report.
    fn stored(&self) -> (Confirmations, Option<Fallback>) {
        let Some(file) = &self.confirmations else {
            return (Confirmations::default(), None);
        };
        match std::fs::read_to_string(file) {
            Err(_) => (Confirmations::default(), None),
            Ok(text) => match Confirmations::parse(&text) {
                Ok(found) => (found, None),
                Err(e) => (
                    Confirmations::default(),
                    Some(Fallback {
                        key: file.display().to_string(),
                        why: Why::NotToml(e.to_string()),
                    }),
                ),
            },
        }
    }

    /// The file as it is now over `base`, with every value looser than the shipped default that
    /// the person has not confirmed put back; no file is `base`, nothing to report.
    pub fn read(&self, base: MemorySettings) -> Loaded {
        let loaded = match self.text() {
            Some(text) => read(&text, base),
            None => Loaded::of(base),
        };
        let (confirmed, unreadable) = self.stored();
        let mut gated = gate(loaded, &confirmed);
        gated.fallbacks.extend(unreadable);
        gated
    }

    /// Records that the person confirmed `values` (key to value): all of them are checked first,
    /// then the store is written atomically (a temp file, synced, renamed). The caller serialises
    /// calls. Refused when there is no store or a pair is not a confirmable setting.
    pub fn confirm(&self, values: &BTreeMap<String, String>) -> Result<(), ConfirmError> {
        let file = self
            .confirmations
            .as_ref()
            .ok_or_else(|| ConfirmError::Store("no store to keep confirmations in".to_owned()))?;
        // A store that does not read holds nothing, so it is written afresh: less is confirmed,
        // never more.
        let (have, _) = self.stored();
        let text = have.with(values)?.to_toml()?;
        crate::config::write_private_atomic(file, &text)
            .map_err(|e| ConfirmError::Store(e.to_string()))
    }
}
