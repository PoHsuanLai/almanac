//! almanac's D-Bus API (memory.md section 3.10): `org.quire.Memory1` at `/org/quire/Memory1`
//! as three interfaces (`Record`, `Recall`, `Control`), each declared twice from one table: a
//! proxy trait for callers and a skeleton for memoryd, whose introspection is the checked-in
//! `dbus/org.quire.Memory1.xml` (see `tests/it/introspection.rs`). Bodies are the serde JSON of
//! `almanac-core` types in `s` arguments. Signatures only: every skeleton method answers
//! `NotSupported`; the served objects (`serve`) are the live ones, over a `Serve` handler.

mod codec;
mod control;
mod error;
mod introspect;
mod invoke;
mod names;
mod recall;
mod record;
mod serve;

pub use codec::{
    Call, CallArg, CodecError, Iface, decode_reply, decode_request, encode_reply, encode_request,
};
pub use control::{ControlProxy, ControlSkeleton};
pub use error::{Failure, MemoryError};
pub use introspect::{INTROSPECTION_FILE, introspection};
pub use invoke::invoke;
pub use names::{
    CONTROL_INTERFACE, ERROR_PREFIX, MEMORY_BUS, MEMORY_PATH, RECALL_INTERFACE, RECORD_INTERFACE,
};
pub use recall::{RecallProxy, RecallSkeleton};
pub use record::{RecordProxy, RecordSkeleton};
pub use serve::{
    ControlObject, RecallObject, RecordObject, Serve, Signal, emit, serve_on, served_introspection,
};
/// The session-bus connection transports and daemons hold.
pub use zbus::Connection as BusConnection;
