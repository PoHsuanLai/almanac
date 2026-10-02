//! Who is calling memoryd. Derived by the transport, never sent in a request.

use porter_core::AppId;
use serde::{Deserialize, Serialize};

/// A caller class. The D-Bus sender is mapped to its `AppId` (porter R12: unsandboxed callers
/// are advisory); `Router`, `Cuad` and `ShellUi` are fixed `AppName`s configured in memoryd.
///
/// The companion has no class of its own: it reads and proposes through the action router
/// (`org.quire.Memory` is a built-in provider of intentd), which calls as `Router` acting for
/// `Actor::Companion` with the Space taken from the invocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Caller {
    /// An app, for its own things.
    App(AppId),
    /// docket's intentd.
    Router,
    /// cuad, for its run records.
    Cuad,
    /// The shell's own UI (sill): the person.
    ShellUi,
}
