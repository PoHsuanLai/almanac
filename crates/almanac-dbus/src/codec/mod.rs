//! The argument codec: a `MemoryRequest` as the bus member and arguments that carry it, and a
//! reply body back as a `MemoryReply`; and the same two in the other direction for the daemon
//! (`decode_request`, `encode_reply`). One table per direction, over the member list of
//! memory.md section 3.10; `tests/codec.rs` round-trips every request and reply of the wire
//! samples through both.
//!
//! Conventions: a Space is its plain id; every other typed argument is the serde JSON of its
//! `almanac-core` type in an `s`; outputs are the same, in order. Three outputs are not JSON:
//! `Primer` is the markdown, `RecordBatch`'s count is a decimal, and a `Record` or `RecordBatch`
//! that admission dropped answers an empty first output (nothing was kept).

mod reply;
mod request;

pub use reply::{decode_reply, encode_reply};
pub use request::{decode_request, encode_request};

/// Which interface a call goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Iface {
    /// `org.quire.Memory1.Record`.
    Record,
    /// `org.quire.Memory1.Recall`.
    Recall,
    /// `org.quire.Memory1.Control`.
    Control,
}

/// One argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallArg {
    /// An `s`: the serde JSON of a core type, or a space id.
    Text(String),
    /// A `t`: seconds since the epoch.
    Seconds(u64),
}

/// A bus call: interface, member and arguments in order. `Export`'s fd travels beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The interface.
    pub interface: Iface,
    /// The member (`RecordBatch`, `PlanForget`).
    pub member: &'static str,
    /// The arguments.
    pub args: Vec<CallArg>,
}

/// Why a body could not be encoded or decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// JSON did not match the type.
    #[error("json: {0}")]
    Json(String),
    /// The reply has more or fewer values than the member returns.
    #[error("unexpected reply shape")]
    Shape,
}

pub(crate) fn json<T: serde::Serialize>(value: &T) -> Result<String, CodecError> {
    serde_json::to_string(value).map_err(|e| CodecError::Json(e.to_string()))
}

pub(crate) fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, CodecError> {
    serde_json::from_str(text).map_err(|e| CodecError::Json(e.to_string()))
}

/// The arguments of a call, taken in order.
#[derive(Debug)]
pub(crate) struct Args<'a>(std::slice::Iter<'a, CallArg>);

impl<'a> Args<'a> {
    pub(crate) fn of(call: &'a Call) -> Self {
        Self(call.args.iter())
    }

    pub(crate) fn text(&mut self) -> Result<&'a str, CodecError> {
        match self.0.next() {
            Some(CallArg::Text(t)) => Ok(t),
            _ => Err(CodecError::Shape),
        }
    }

    /// The next argument, parsed as JSON.
    pub(crate) fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, CodecError> {
        parse(self.text()?)
    }

    pub(crate) fn seconds(&mut self) -> Result<u64, CodecError> {
        match self.0.next() {
            Some(CallArg::Seconds(s)) => Ok(*s),
            _ => Err(CodecError::Shape),
        }
    }

    /// No arguments are left.
    pub(crate) fn end(mut self) -> Result<(), CodecError> {
        match self.0.next() {
            None => Ok(()),
            Some(_) => Err(CodecError::Shape),
        }
    }
}
