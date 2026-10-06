//! Where the settings file is: `$XDG_CONFIG_HOME/almanac/settings.toml`, then each of
//! `$XDG_CONFIG_DIRS` (design/22 section 2). The first file that reads wins.

use super::read::{Loaded, read};
use super::{MemorySettings, SETTINGS_FILE};
use std::path::{Path, PathBuf};

/// The configuration directories, in order of precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locator {
    dirs: Vec<PathBuf>,
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
        }
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

    /// The file as it is now over `base`; no file is `base`, nothing to report.
    pub fn read(&self, base: MemorySettings) -> Loaded {
        match self.text() {
            Some(text) => read(&text, base),
            None => Loaded::of(base),
        }
    }
}
