//! The app-facing memory API: `Memory` over a `Transport` (`Absent`; `DbusTransport` behind the
//! `dbus` feature; `InProcess` behind the default-off `in_process` feature, the only one that
//! links `almanac-service`, SQLCipher and OpenSSL). Apps record what happened; the shell UI reads
//! the timeline and controls.

mod memory;
mod transport;

pub use memory::{ClientError, Memory, Recorded};
#[cfg(feature = "dbus")]
pub use transport::DbusTransport;
#[cfg(feature = "in_process")]
pub use transport::InProcess;
pub use transport::{Absent, Transport, TransportError};
