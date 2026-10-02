//! The export: a tar stream whose layout, entry lines and writer are pinned here.
//!
//! ```text
//! quire-memory-export-1/manifest.json     ExportManifest
//! quire-memory-export-1/<space>/events.jsonl   one EventLine per entry
//! quire-memory-export-1/<space>/facts/**.md, procedures/**.md, pending/**.md   plain, never sealed
//! quire-memory-export-1/rules.toml
//! ```
//!
//! The index and the keys are never exported. `events.jsonl` lets a third party verify the
//! chain; bodies can be checked against their digests only by someone holding the digest
//! subkey, which is exported only when the person asks.

use almanac_core::{
    Actor, Cause, Digest32, EXPORT_ROOT, Effect, EventBody, ExportManifest, KindTag, Label, Link32,
    ReplicaId, Seq, SpaceId, UnixSeconds,
};
use eventlog::{BodyState, Entry};
use memfiles::VaultPath;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// The marker that stands where an erased body would be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErasedTag {
    /// The body was forgotten, expired or never stored.
    #[serde(rename = "erased")]
    Erased,
}

/// An event's body in `events.jsonl`: the typed body, or the string `"erased"`.
// The typed body is the stored one as it is; boxing it would complicate every match.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExportedBody {
    /// Erased.
    Erased(ErasedTag),
    /// Present.
    Present(EventBody),
}

/// One line of `events.jsonl`: the header fields, the link and the body-or-`"erased"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLine {
    /// Sequence number.
    pub seq: Seq,
    /// Replica.
    pub replica: ReplicaId,
    /// When it happened.
    pub occurred: UnixSeconds,
    /// When it was recorded.
    pub recorded: UnixSeconds,
    /// Who.
    pub actor: Actor,
    /// Kind.
    pub kind: KindTag,
    /// Effect.
    pub effect: Effect,
    /// Label.
    pub label: Label,
    /// Cause.
    pub cause: Cause,
    /// The keyed body digest.
    pub body_digest: Digest32,
    /// The previous link.
    pub prev: Link32,
    /// This entry's link.
    pub link: Link32,
    /// The body, or `"erased"`.
    pub body: ExportedBody,
}

impl EventLine {
    /// The line of `entry`.
    pub fn of(entry: &Entry) -> EventLine {
        let h = &entry.header;
        EventLine {
            seq: h.seq,
            replica: h.replica,
            occurred: h.occurred,
            recorded: h.recorded,
            actor: h.actor.clone(),
            kind: h.kind.clone(),
            effect: h.effect,
            label: h.label.clone(),
            cause: h.cause.clone(),
            body_digest: h.body_digest,
            prev: h.prev,
            link: entry.link,
            body: match &entry.body {
                BodyState::Present(body) => ExportedBody::Present(body.clone()),
                BodyState::Erased => ExportedBody::Erased(ErasedTag::Erased),
            },
        }
    }

    /// The JSON text of the line (no newline).
    pub fn to_json(&self) -> String {
        // Plain derives with string keys: serialisation cannot fail.
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// `quire-memory-export-1/manifest.json`.
pub fn manifest_path() -> String {
    format!("{EXPORT_ROOT}/manifest.json")
}

/// `quire-memory-export-1/rules.toml`.
pub fn rules_path() -> String {
    format!("{EXPORT_ROOT}/rules.toml")
}

/// `quire-memory-export-1/<space>/events.jsonl`.
pub fn events_path(space: &SpaceId) -> String {
    format!("{EXPORT_ROOT}/{space}/events.jsonl")
}

/// `quire-memory-export-1/<space>/<vault path>` (facts, procedures, pending).
pub fn file_path(space: &SpaceId, rel: &VaultPath) -> String {
    format!("{EXPORT_ROOT}/{space}/{rel}")
}

/// Writes the export into any `io::Write`, entry by entry, with fixed modes and a given
/// modification time so the stream is reproducible.
pub struct ExportWriter<W: Write> {
    builder: tar::Builder<W>,
    mtime: UnixSeconds,
}

impl<W: Write> std::fmt::Debug for ExportWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportWriter")
            .field("mtime", &self.mtime)
            .finish_non_exhaustive()
    }
}

impl<W: Write> ExportWriter<W> {
    /// A writer into `out`; every entry carries `mtime`.
    pub fn new(out: W, mtime: UnixSeconds) -> Self {
        Self {
            builder: tar::Builder::new(out),
            mtime,
        }
    }

    fn entry(&mut self, path: &str, bytes: &[u8]) -> std::io::Result<()> {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        header.set_mode(0o644);
        header.set_mtime(u64::try_from(self.mtime.0).unwrap_or(0));
        self.builder.append_data(&mut header, path, bytes)
    }

    /// `manifest.json`.
    pub fn manifest(&mut self, manifest: &ExportManifest) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(manifest).map_err(std::io::Error::other)?;
        self.entry(&manifest_path(), &json)
    }

    /// `<space>/events.jsonl`, one line per entry.
    pub fn events(&mut self, space: &SpaceId, lines: &[EventLine]) -> std::io::Result<()> {
        let mut text = String::new();
        for line in lines {
            text.push_str(&line.to_json());
            text.push('\n');
        }
        self.entry(&events_path(space), text.as_bytes())
    }

    /// One plain file of a Space (a topic, a pending fact, a procedure).
    pub fn file(&mut self, space: &SpaceId, rel: &VaultPath, bytes: &[u8]) -> std::io::Result<()> {
        self.entry(&file_path(space, rel), bytes)
    }

    /// `rules.toml`.
    pub fn rules(&mut self, toml: &str) -> std::io::Result<()> {
        self.entry(&rules_path(), toml.as_bytes())
    }

    /// Ends the archive and returns the output.
    pub fn finish(self) -> std::io::Result<W> {
        self.builder.into_inner()
    }
}
