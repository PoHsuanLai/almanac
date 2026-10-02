//! The app-facing memory API: `Memory` over a `Transport` (`InProcess`, `Absent`, and
//! `DbusTransport` behind the `dbus` feature). Apps record what happened; the shell UI reads
//! the timeline and controls. Without the `dbus` feature it builds wherever the service does.

mod memory;
mod transport;

pub use memory::{ClientError, Memory, Recorded};
#[cfg(feature = "dbus")]
pub use transport::DbusTransport;
pub use transport::{Absent, InProcess, Transport, TransportError};
