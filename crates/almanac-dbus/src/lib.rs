//! almanac's D-Bus API (memory.md section 3.10): `org.quire.Memory1` at `/org/quire/Memory1`
//! as three interfaces (`Record`, `Recall`, `Control`), each declared twice from one table: a
//! proxy trait for callers and a skeleton for memoryd, whose introspection is the checked-in
//! `dbus/org.quire.Memory1.xml` (see `tests/introspection.rs`). Bodies are the serde JSON of
//! `almanac-core` types in `s` arguments. Signatures only: every skeleton method answers
//! `NotSupported`.

mod codec;
mod control;
mod error;
mod introspect;
mod names;
mod recall;
mod record;

pub use codec::{Call, CallArg, CodecError, Iface, decode_reply, encode_request};
pub use control::{ControlProxy, ControlSkeleton};
pub use error::MemoryError;
pub use introspect::{INTROSPECTION_FILE, introspection};
pub use names::{
    CONTROL_INTERFACE, ERROR_PREFIX, MEMORY_BUS, MEMORY_PATH, RECALL_INTERFACE, RECORD_INTERFACE,
};
pub use recall::{RecallProxy, RecallSkeleton};
pub use record::{RecordProxy, RecordSkeleton};
/// The session-bus connection transports and daemons hold.
pub use zbus::Connection as BusConnection;
