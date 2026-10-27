//! The daemon's half of almanac's D-Bus API (`org.quire.Memory1`, memory.md section 3.10): the
//! served objects over a [`Serve`] handler, the signals ([`Signal`], [`emit_to`], [`emit`]), and
//! the frozen skeletons whose introspection is the checked-in `dbus/org.quire.Memory1.xml`
//! ([`introspection`]; the served objects declare the same document, [`served_introspection`]).
//! The caller's half (proxies, codec, `invoke`) is `almanac-dbus`; callers never need this crate.

mod introspect;
mod serve;
mod skeleton;

pub use introspect::{INTROSPECTION_FILE, introspection};
pub use serve::{
    CONFINEMENT_OFF, ControlObject, RecallObject, RecordObject, Serve, Signal, emit, emit_to,
    serve_on, served_introspection,
};
