//! The header and its canonical bytes (v1): the input of the hash chain.
//!
//! ```text
//! "QEL1" | seq u64be | replica [16] | occurred i64be | recorded i64be
//! | lp(actor json) | lp(kind tag) | effect u8 | lp(label json) | lp(cause json)
//! | body_digest [32] | prev_link [32]
//! lp(x) = u32be length ‖ bytes; json = serde_json of the typed value (declaration order)
//! ```
//!
//! The header holds no thing ids and no text: subjects and sources live in the body and in the
//! `things` table, both of which can be erased without breaking the chain.

use almanac_core::{
    Actor, Cause, Digest32, Effect, EventBody, KindTag, Label, Link32, Record, ReplicaId, Seq,
    SpaceId, UnixSeconds,
};
use almanac_seal::SubKey;
use serde::Serialize;

/// The four bytes that open every header's canonical form.
pub const HEADER_MAGIC: &[u8; 4] = b"QEL1";
/// What the genesis link hashes after the Space and replica.
pub const GENESIS_CONTEXT: &[u8] = b"QEL1 genesis";

/// One event's header, as chained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Its sequence number, from 1.
    pub seq: Seq,
    /// The replica that wrote it.
    pub replica: ReplicaId,
    /// When it happened.
    pub occurred: UnixSeconds,
    /// When it was recorded.
    pub recorded: UnixSeconds,
    /// Who did it.
    pub actor: Actor,
    /// What kind.
    pub kind: KindTag,
    /// How consequential.
    pub effect: Effect,
    /// Its provenance.
    pub label: Label,
    /// What caused it.
    pub cause: Cause,
    /// The keyed digest of its body.
    pub body_digest: Digest32,
    /// The previous entry's link.
    pub prev: Link32,
}

/// A header before the log numbers and chains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHeader {
    /// When it happened.
    pub occurred: UnixSeconds,
    /// When it was recorded.
    pub recorded: UnixSeconds,
    /// Who did it.
    pub actor: Actor,
    /// What kind (the body's `kind()`).
    pub kind: KindTag,
    /// How consequential.
    pub effect: Effect,
    /// Its provenance.
    pub label: Label,
    /// What caused it.
    pub cause: Cause,
    /// The keyed digest of the body, from [`body_digest`] (kept even when the body is not).
    pub body_digest: Digest32,
}

impl NewHeader {
    /// The header of `record`, stamped with the recorded time and the body's keyed digest.
    pub fn of(record: &Record, recorded: UnixSeconds, digest_key: &SubKey) -> NewHeader {
        NewHeader {
            occurred: record.occurred,
            recorded,
            actor: record.actor.clone(),
            kind: record.body.kind(),
            effect: record.effect,
            label: record.label.clone(),
            cause: record.cause.clone(),
            body_digest: body_digest(digest_key, &record.body),
        }
    }

    /// The header with its place in the chain.
    pub fn chained(self, seq: Seq, replica: ReplicaId, prev: Link32) -> Header {
        Header {
            seq,
            replica,
            occurred: self.occurred,
            recorded: self.recorded,
            actor: self.actor,
            kind: self.kind,
            effect: self.effect,
            label: self.label,
            cause: self.cause,
            body_digest: self.body_digest,
            prev,
        }
    }
}

fn json<T: Serialize>(value: &T) -> Vec<u8> {
    // The typed values here are plain derives with string keys: serialisation cannot fail.
    serde_json::to_vec(value).unwrap_or_default()
}

fn push_lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(bytes);
}

const fn effect_byte(effect: Effect) -> u8 {
    match effect {
        Effect::Read => 0,
        Effect::UndoableWrite => 1,
        Effect::Outbound => 2,
        Effect::Destructive => 3,
    }
}

/// The canonical bytes of a header. Pure, golden-tested.
pub fn header_bytes(h: &Header) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(HEADER_MAGIC);
    out.extend_from_slice(&h.seq.0.to_be_bytes());
    out.extend_from_slice(&h.replica.0);
    out.extend_from_slice(&h.occurred.0.to_be_bytes());
    out.extend_from_slice(&h.recorded.0.to_be_bytes());
    push_lp(&mut out, &json(&h.actor));
    push_lp(&mut out, h.kind.as_str().as_bytes());
    out.push(effect_byte(h.effect));
    push_lp(&mut out, &json(&h.label));
    push_lp(&mut out, &json(&h.cause));
    out.extend_from_slice(&h.body_digest.0);
    out.extend_from_slice(&h.prev.0);
    out
}

/// The entry's link: `blake3` of its canonical header bytes.
pub fn link(h: &Header) -> Link32 {
    Link32(*blake3::hash(&header_bytes(h)).as_bytes())
}

/// The `prev` of the first entry of a (Space, replica) chain.
pub fn genesis_link(space: &SpaceId, replica: &ReplicaId) -> Link32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(GENESIS_CONTEXT);
    hasher.update(space.as_str().as_bytes());
    hasher.update(&replica.0);
    Link32(*hasher.finalize().as_bytes())
}

/// The keyed digest of a body (`blake3` keyed with the Space's digest subkey), so an erased
/// body cannot be guessed from the chain.
pub fn body_digest(key: &SubKey, body: &EventBody) -> Digest32 {
    Digest32(*blake3::keyed_hash(key.expose(), &json(body)).as_bytes())
}
