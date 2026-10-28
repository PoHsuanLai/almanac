//! The caller table as files, and porter's caller mapped onto almanac's.
//!
//! The identification is porter's (`porter_callers::ProcCallers`: the bus names the pid, the pid's
//! cgroup names the unit or the app scope), and so is the reading of the table files
//! (`porter_callers::load_callers`). What is memoryd's own is where they are —
//! `/etc/quire/memory-callers.toml` with the user's `<config>/quire/memory-callers.toml` laid
//! over it — and [`caller_for`] that turns a porter role into one of almanac's callers. The router, the shell, cuad and the Settings app are the four fixed
//! callers; every other identified process is an app.

use almanac_service::Caller;
use porter_callers::CallerRole;

/// The app that is intentd, the action router (its row has the `agent` role).
pub const ROUTER_APP: &str = "org.quire.Intents";
/// The app that is the shell, sill (its row has the `sheet_host` role).
pub const SHELL_APP: &str = "org.quire.Shell";
/// The app that is the System Settings app, detent (its row has the `settings` role).
pub const SETTINGS_APP: &str = "org.quire.Settings";

/// almanac's caller for the process porter identified: cuad is the `cua` role, the shell the
/// `sheet_host` role of [`SHELL_APP`], the router the `agent` role of [`ROUTER_APP`], the Settings
/// app the `settings` role of [`SETTINGS_APP`]; any other
/// row, or an app scope or Flatpak scope the table does not name, is that app. The fixed callers
/// need the role as well as the app, so a row cannot make an app the router by naming it.
pub fn caller_for(found: porter_callers::Caller) -> Caller {
    let name = found.app.name.as_str();
    match found.role {
        CallerRole::Cua => Caller::Cuad,
        CallerRole::SheetHost if name == SHELL_APP => Caller::ShellUi,
        CallerRole::Agent if name == ROUTER_APP => Caller::Router,
        CallerRole::Settings if name == SETTINGS_APP => Caller::Settings,
        // A terminal (temor) is no app and no fixed caller, but this file maps every identified
        // process that is not a fixed caller to an app, as porter's lane did for `Terminal`; it
        // gets no shell or router standing, only its own app's access.
        CallerRole::App
        | CallerRole::Terminal
        | CallerRole::Settings
        | CallerRole::SheetHost
        | CallerRole::PorterDaemon
        | CallerRole::AiBroker
        | CallerRole::AgentLauncher
        | CallerRole::Agent => Caller::App(found.app),
        // The compositor, or a role a later porter adds: its own app's access, no fixed standing.
        _ => Caller::App(found.app),
    }
}
