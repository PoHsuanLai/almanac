//! Spaces as memory sees them: their lifecycle state, their metadata file and their status.

use crate::chain::ChainReport;
use crate::ids::ReplicaId;
use crate::slug::slug_enum;
use porter_core::{Bytes, Count, SpaceId, UnixSeconds};
use prov::RunId;
use serde::{Deserialize, Serialize};

/// Where a Space is in its lifecycle (the machine in `almanac-service::space`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SpaceState {
    /// The key is not available; records are buffered.
    Locked,
    /// Recording.
    Open,
    /// Not remembering until the time passes or the person resumes.
    Paused {
        /// When it resumes by itself.
        until: UnixSeconds,
    },
    /// Being deleted.
    Deleting,
    /// Deleted: key destroyed, directories removed.
    Gone,
}

slug_enum!(
    /// How a Space's files are held at rest.
    VaultKind {
        /// Sealed per file with the Space's key (default).
        Sealed => "sealed",
        /// Plain markdown files, protected only by the disk.
        Plain => "plain",
    }
);

/// One entry of `spaces.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceMeta {
    /// The Space.
    pub id: SpaceId,
    /// When memory for it began.
    pub created: UnixSeconds,
    /// This machine's replica of it.
    pub replica: ReplicaId,
    /// How its files are held.
    pub vault: VaultKind,
    /// The on-disk format number (`format: quire-memory N`).
    pub format: u32,
}

/// What the Spaces list shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceSummary {
    /// The Space.
    pub id: SpaceId,
    /// Its state.
    pub state: SpaceState,
    /// How its files are held.
    pub vault: VaultKind,
    /// When memory for it began.
    pub created: UnixSeconds,
}

/// Why an index is out of date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleWhy {
    /// The configured embedder differs from the one the vectors came from.
    EmbedderChanged,
    /// The index format changed.
    FormatChanged,
    /// The files and log are newer than the index.
    TruthNewer,
}

/// Why search is lexical only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedWhy {
    /// No embedder is reachable.
    EmbedderUnavailable,
    /// The embedder refused (data class floor, cap).
    EmbedderRefused,
}

/// The recall index as the UI shows it (the wire form of `recall::IndexState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum IndexView {
    /// Not built yet.
    Absent,
    /// Being built.
    Building {
        /// Documents done.
        done: Count,
        /// Documents in all.
        total: Count,
    },
    /// Up to date.
    Ready,
    /// Out of date.
    Stale(StaleWhy),
    /// Lexical search only.
    LexicalOnly(DegradedWhy),
}

/// What the last verification found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ChainHealth {
    /// Not verified since the daemon started.
    Unchecked,
    /// The last result.
    Checked(ChainReport),
}

/// One Space's status page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceStatus {
    /// Its state.
    pub state: SpaceState,
    /// Its index.
    pub index: IndexView,
    /// Its chain.
    pub chain: ChainHealth,
    /// Disk used by its files and log.
    pub usage: Bytes,
    /// Events held.
    pub events: Count,
    /// Active facts.
    pub facts: Count,
    /// Pending facts.
    pub pending: Count,
    /// The last consolidation run.
    pub last_run: Option<RunId>,
}
