//! The daemon's half of the bus: the three interfaces of `org.quire.Memory1` as zbus objects over
//! one [`Serve`] handler. Every method builds the [`Call`] of its member from its raw arguments
//! and hands it, with the sender's unique name, to the handler; the handler answers with the
//! member's outputs (`decode_request` and `encode_reply` are the two ends of that table). Their
//! introspection is the checked-in `dbus/org.quire.Memory1.xml`, the same as the frozen
//! skeletons' (`tests/it/served.rs`).

mod control;
mod recall;
mod record;
mod signal;

pub use control::ControlObject;
pub use recall::RecallObject;
pub use record::RecordObject;
pub use signal::{Signal, emit, emit_to};

use almanac_dbus::{Call, CallArg, Iface, MEMORY_BUS, MEMORY_PATH, MemoryError};
use std::future::Future;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use zbus::Connection;
use zbus::message::Header;
use zbus::object_server::Interface;

/// The `Confinement` property word of a handler that applies no sandbox (a test).
pub const CONFINEMENT_OFF: &str = "off";

/// What memoryd implements: one call in, the member's outputs out. `sender` is the caller's
/// unique bus name; who that is (`Caller`) is the handler's to establish.
pub trait Serve: Send + Sync + 'static {
    /// Serves `call` for `sender`; `fd` is the stream `Export` writes to.
    fn serve(
        &self,
        sender: &str,
        call: Call,
        fd: Option<OwnedFd>,
    ) -> impl Future<Output = Result<Vec<String>, MemoryError>> + Send;

    /// How far the handler's process confines itself, as the word the `Confinement` property
    /// carries (`confined`, `partial`, `not-available` or `off`). A host that applies no sandbox
    /// (a test) is [`CONFINEMENT_OFF`].
    fn confinement(&self) -> &'static str {
        CONFINEMENT_OFF
    }
}

fn sender_of(header: &Header<'_>) -> Result<String, MemoryError> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or_else(|| MemoryError::NotAllowed("no sender".into()))
}

fn text(value: String) -> CallArg {
    CallArg::Text(value)
}

async fn forward<S: Serve>(
    handler: &S,
    header: &Header<'_>,
    interface: Iface,
    member: &'static str,
    args: Vec<CallArg>,
    fd: Option<OwnedFd>,
) -> Result<Vec<String>, MemoryError> {
    let call = Call {
        interface,
        member,
        args,
    };
    // Boxed: the handler's future holds every member's state machine, and inline it sits on the
    // bus worker's stack under zbus's own dispatch frames. A worker has 2 MiB, and the deepest
    // member (a shell's call through the queues, the service and the store) overflowed it.
    Box::pin(handler.serve(&sender_of(header)?, call, fd)).await
}

fn shape() -> MemoryError {
    MemoryError::Invalid("the handler answered with the wrong number of values".into())
}

fn first(mut outputs: Vec<String>) -> Result<String, MemoryError> {
    match outputs.len() {
        1 => outputs.pop().ok_or_else(shape),
        _ => Err(shape()),
    }
}

fn two(mut outputs: Vec<String>) -> Result<(String, String), MemoryError> {
    match (outputs.pop(), outputs.pop(), outputs.is_empty()) {
        (Some(b), Some(a), true) => Ok((a, b)),
        _ => Err(shape()),
    }
}

fn none(outputs: Vec<String>) -> Result<(), MemoryError> {
    outputs.is_empty().then_some(()).ok_or_else(shape)
}

/// Puts the three interfaces at `MEMORY_PATH` over `handler`, then claims `MEMORY_BUS`: nothing
/// is reachable under the name before every interface is.
pub async fn serve_on<S: Serve>(connection: &Connection, handler: Arc<S>) -> zbus::Result<()> {
    let server = connection.object_server();
    server
        .at(MEMORY_PATH, RecordObject(handler.clone()))
        .await?;
    server
        .at(MEMORY_PATH, RecallObject(handler.clone()))
        .await?;
    server.at(MEMORY_PATH, ControlObject(handler)).await?;
    connection.request_name(MEMORY_BUS).await
}

/// The introspection of the served objects: the same document as the skeletons' (`introspection`).
pub fn served_introspection<S: Serve>(handler: &Arc<S>) -> String {
    let objects = (
        RecordObject(handler.clone()),
        RecallObject(handler.clone()),
        ControlObject(handler.clone()),
    );
    crate::introspect::document([&objects.0 as &dyn Interface, &objects.1, &objects.2])
}
