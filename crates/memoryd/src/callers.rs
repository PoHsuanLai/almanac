//! The caller table as files, and porter's caller mapped onto almanac's.
//!
//! The identification is porter's (`porter_dbus::ProcCallers`: the bus names the pid, the pid's
//! cgroup names the unit or the app scope). What is memoryd's own is the file and the mapping:
//! `/etc/quire/memory-callers.toml` with the user's `<config>/quire/memory-callers.toml` laid
//! over it, in porter's `[[caller]]` shape, and [`caller_for`] that turns a porter role into one
//! of almanac's callers. The router, the shell and cuad are the three fixed callers; every other
//! identified process is an app.

use almanac_core::Caller;
use porter_dbus::{CallerRole, CallerTable};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The app that is intentd, the action router (its row has the `agent` role).
pub const ROUTER_APP: &str = "org.quire.Intents";
/// The app that is the shell, sill (its row has the `sheet_host` role).
pub const SHELL_APP: &str = "org.quire.Shell";

/// A table file that exists and cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}: {message}", path.display())]
pub struct CallerFileError {
    /// The file.
    pub path: PathBuf,
    /// What is wrong with it.
    pub message: String,
}

/// The table in TOML text: `[[caller]]` rows of `app`, optional `unit` and `role`.
pub fn table_from_toml(text: &str) -> Result<CallerTable, toml::de::Error> {
    toml::from_str(text)
}

/// The table in the file `path`; a file that is not there is an empty table.
pub fn table_from_file(path: &Path) -> Result<CallerTable, CallerFileError> {
    let failed = |message: String| CallerFileError {
        path: path.to_owned(),
        message,
    };
    match std::fs::read_to_string(path) {
        Ok(text) => table_from_toml(&text).map_err(|e| failed(e.to_string())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(CallerTable::default()),
        Err(e) => Err(failed(e.to_string())),
    }
}

/// The system's file with the user's laid over it (the user's rows win). A file that does not
/// parse is an error, not an empty table: a daemon that dropped a bad user file would grant
/// less or more than it was told.
pub fn load_callers(system: &Path, user: &Path) -> Result<CallerTable, CallerFileError> {
    Ok(CallerTable::layered(
        table_from_file(system)?,
        table_from_file(user)?,
    ))
}

/// almanac's caller for the process porter identified: cuad is the `cua` role, the shell the
/// `sheet_host` role of [`SHELL_APP`], the router the `agent` role of [`ROUTER_APP`]; any other
/// row, or an app scope or Flatpak scope the table does not name, is that app. The fixed callers
/// need the role as well as the app, so a row cannot make an app the router by naming it.
pub fn caller_for(found: porter_dbus::Caller) -> Caller {
    let name = found.app.name.as_str();
    match found.role {
        CallerRole::Cua => Caller::Cuad,
        CallerRole::SheetHost if name == SHELL_APP => Caller::ShellUi,
        CallerRole::Agent if name == ROUTER_APP => Caller::Router,
        // A terminal (temor) is no app and no fixed caller, but this file maps every identified
        // process that is not a fixed caller to an app, as porter's lane did for `Terminal`; it
        // gets no shell or router standing, only its own app's access.
        CallerRole::App
        | CallerRole::Terminal
        | CallerRole::Settings
        | CallerRole::SheetHost
        | CallerRole::PorterDaemon
        | CallerRole::AgentLauncher
        | CallerRole::Agent => Caller::App(found.app),
    }
}
