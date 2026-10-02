//! The export: a tar stream with a manifest, plain files and the event lines.

use crate::chain::Head;
use crate::op::ExportCounts;
use porter_core::{SpaceId, UnixSeconds};
use serde::{Deserialize, Serialize};

/// The top directory of the tar stream; its trailing number is the export format.
pub const EXPORT_ROOT: &str = "quire-memory-export-1";
/// The manifest's `format` field.
pub const EXPORT_FORMAT: &str = "quire-memory-export 1";

/// One Space in an export.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExportedSpace {
    /// The Space.
    pub space: SpaceId,
    /// The chain head at export time: third parties verify `events.jsonl` up to it.
    pub head: Option<Head>,
    /// What it contributed.
    pub counts: ExportCounts,
}

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Always [`EXPORT_FORMAT`].
    pub format: String,
    /// The Spaces, in order.
    pub spaces: Vec<ExportedSpace>,
    /// When it was made.
    pub created: UnixSeconds,
    /// Totals over all Spaces.
    pub counts: ExportCounts,
}
