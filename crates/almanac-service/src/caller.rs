//! Who is calling memoryd. Derived by the transport, never sent in a request.

use almanac_core::AppId;

/// A caller class: the input of the authorisation matrix ([`crate::allowed`]). It is not wire:
/// no request, reply or stored file carries one. The daemon maps the D-Bus sender to its `AppId`
/// (porter R12: unsandboxed callers are advisory); `Router`, `Cuad`, `ShellUi` and `Settings` are
/// fixed `AppName`s configured in memoryd. A library user picks the class when it builds its
/// transport (`almanac-client`'s `InProcess`).
///
/// The companion has no class of its own: it reads and proposes through the action router
/// (`org.quire.Memory` is a built-in provider of intentd), which calls as `Router` acting for
/// `Actor::Companion` with the Space taken from the invocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Caller {
    /// An app, for its own things.
    App(AppId),
    /// docket's intentd.
    Router,
    /// cuad, for its run records.
    Cuad,
    /// The shell's own UI (sill): the person.
    ShellUi,
    /// The System Settings app (detent): the person, saving a setting. Not an app with memory: it
    /// may only confirm a loosened memory setting and read the ones that wait for it.
    Settings,
}
