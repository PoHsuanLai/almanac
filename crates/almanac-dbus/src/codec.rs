//! The argument codec: a `MemoryRequest` as the bus member and arguments that carry it, and a
//! reply body back as a `MemoryReply`. Frozen signatures; the bodies are `todo!()` until
//! memoryd serves the bus (FINDINGS.md).

use almanac_core::{MemoryReply, MemoryRequest};

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

/// The call that carries `request`.
pub fn encode_request(request: &MemoryRequest) -> Result<Call, CodecError> {
    let _ = request;
    todo!(
        "one match: each variant to its member and JSON arguments, as the Memory1 table in memory.md section 3.10"
    )
}

/// The reply of `call` from the body's values (the JSON `s` outputs, in order).
pub fn decode_reply(call: &Call, outputs: &[String]) -> Result<MemoryReply, CodecError> {
    let _ = (call, outputs);
    todo!("parse each output as the reply type of the member; a bus error name maps to Refusal")
}
