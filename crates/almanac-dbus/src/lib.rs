//! almanac's D-Bus API, the caller's half (memory.md section 3.10): `org.quire.Memory1` at
//! `/org/quire/Memory1` as three interfaces (`Record`, `Recall`, `Control`) with a zbus proxy
//! each, the argument codec between `almanac-core` requests and the member arguments, `invoke`
//! (a call over a connection) and the typed errors. Bodies are the serde JSON of `almanac-core`
//! types in `s` arguments. The daemon's half (skeletons, served objects, signals, the
//! introspection checked against `dbus/org.quire.Memory1.xml`) is `almanac-dbus-serve`.

mod codec;
mod control;
mod error;
mod invoke;
mod names;
mod recall;
mod record;

pub use codec::{
    Call, CallArg, CodecError, Iface, decode_reply, decode_request, encode_reply, encode_request,
};
pub use control::ControlProxy;
pub use error::{Failure, MemoryError};
pub use invoke::invoke;
pub use names::{
    CONTROL_INTERFACE, ERROR_PREFIX, MEMORY_BUS, MEMORY_PATH, RECALL_INTERFACE, RECORD_INTERFACE,
};
pub use recall::RecallProxy;
pub use record::RecordProxy;
/// The session-bus connection transports and daemons hold.
pub use zbus::Connection as BusConnection;
