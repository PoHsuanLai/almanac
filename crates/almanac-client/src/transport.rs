//! How a request reaches memoryd: in process, nowhere (other desktops), or over D-Bus.

use almanac_core::{Caller, MemoryReply, MemoryRequest};
use almanac_service::{Backend, MemoryService};
use std::future::Future;
use std::sync::Arc;

/// Why a request did not get an answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// There is no memory on this desktop: writers treat it as a no-op.
    #[error("no memory on this desktop")]
    Absent,
    /// The connection closed.
    #[error("connection closed")]
    Closed,
    /// The bus failed.
    #[error("bus: {0}")]
    Bus(String),
}

/// A way to send one request and get one reply. The caller identity is the transport's: the
/// bus derives it from the connection, in-process it is configured.
pub trait Transport: Send + Sync {
    /// Sends `request`.
    fn call(
        &self,
        request: MemoryRequest,
    ) -> impl Future<Output = Result<MemoryReply, TransportError>> + Send;
}

/// The app hosts the service itself (tests, single-process embedders).
pub struct InProcess<B: Backend> {
    service: Arc<MemoryService<B>>,
    caller: Caller,
}

impl<B: Backend> std::fmt::Debug for InProcess<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcess")
            .field("caller", &self.caller)
            .finish_non_exhaustive()
    }
}

impl<B: Backend> InProcess<B> {
    /// Calls `service` as `caller`.
    pub fn new(service: Arc<MemoryService<B>>, caller: Caller) -> Self {
        Self { service, caller }
    }
}

impl<B: Backend> Transport for InProcess<B> {
    async fn call(&self, request: MemoryRequest) -> Result<MemoryReply, TransportError> {
        Ok(self.service.handle(&self.caller, request).await)
    }
}

/// No memory here (another desktop, or memoryd is not installed): every call is `Absent`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Absent;

impl Transport for Absent {
    async fn call(&self, _request: MemoryRequest) -> Result<MemoryReply, TransportError> {
        Err(TransportError::Absent)
    }
}

/// `org.quire.Memory1` on the session bus.
#[cfg(feature = "dbus")]
#[derive(Debug)]
pub struct DbusTransport {
    connection: almanac_dbus::BusConnection,
}

#[cfg(feature = "dbus")]
impl DbusTransport {
    /// Over an existing session-bus connection.
    pub fn new(connection: almanac_dbus::BusConnection) -> Self {
        Self { connection }
    }

    /// The connection.
    pub fn connection(&self) -> &almanac_dbus::BusConnection {
        &self.connection
    }
}

#[cfg(feature = "dbus")]
impl Transport for DbusTransport {
    async fn call(&self, request: MemoryRequest) -> Result<MemoryReply, TransportError> {
        let _ = (&self.connection, almanac_dbus::encode_request(&request));
        todo!("encode_request, the proxy call, decode_reply; a bus error name maps back to Refusal")
    }
}
