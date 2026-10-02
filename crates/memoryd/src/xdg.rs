//! The XDG roots from the environment: read once, in `main`, and injected everywhere else.

use almanac_core::Dirs;
use std::path::PathBuf;

/// Why the directories could not be found.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum XdgError {
    /// Neither `XDG_*` nor `HOME` is set.
    #[error("neither {0} nor HOME is set")]
    NoHome(&'static str),
    /// `XDG_RUNTIME_DIR` is not set (memoryd needs a runtime directory for edit copies).
    #[error("XDG_RUNTIME_DIR is not set")]
    NoRuntimeDir,
}

fn root(
    lookup: &impl Fn(&str) -> Option<String>,
    var: &'static str,
    under_home: &str,
) -> Result<PathBuf, XdgError> {
    match lookup(var).filter(|v| !v.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => lookup("HOME")
            .filter(|h| !h.is_empty())
            .map(|home| PathBuf::from(home).join(under_home))
            .ok_or(XdgError::NoHome(var)),
    }
}

/// The directories as `lookup` (the environment, or a test's map) says: `XDG_DATA_HOME`
/// (default `~/.local/share`), `XDG_CACHE_HOME` (`~/.cache`), `XDG_CONFIG_HOME` (`~/.config`)
/// and the required `XDG_RUNTIME_DIR`.
pub fn dirs_from(lookup: impl Fn(&str) -> Option<String>) -> Result<Dirs, XdgError> {
    Ok(Dirs::new(
        root(&lookup, "XDG_DATA_HOME", ".local/share")?,
        root(&lookup, "XDG_CACHE_HOME", ".cache")?,
        root(&lookup, "XDG_CONFIG_HOME", ".config")?,
        lookup("XDG_RUNTIME_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or(XdgError::NoRuntimeDir)?,
    ))
}

/// The directories from the process environment.
pub fn dirs_from_env() -> Result<Dirs, XdgError> {
    dirs_from(|name| std::env::var(name).ok())
}
