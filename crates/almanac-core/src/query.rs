//! What readers and explainers ask: queries, file-why claims, marks and export options.

use crate::ids::{SpacePath, TopicPath};
use crate::slug::slug_enum;
use crate::text::{ContentDigest, UserText};
use crate::thing::{ThingRef, Verb};
use porter_core::{Count, SpaceId};
use prov::Actor;
use serde::{Deserialize, Serialize};

slug_enum!(
    /// Which documents a search covers.
    RecallOver {
        /// Facts only.
        Facts => "facts",
        /// Every event document with text: thing and search text, messages, episodes.
        Events => "events",
        /// Messages only.
        Messages => "messages",
        /// Episodes (skeletons and narratives) only.
        Episodes => "episodes",
        /// Facts and every event document.
        Both => "both"
    }
);

/// A search over one Space.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecallQuery {
    /// The Space.
    pub space: SpaceId,
    /// What to look for.
    pub text: UserText,
    /// How many hits at most.
    pub limit: Count,
    /// Which documents.
    pub over: RecallOver,
}

slug_enum!(
    /// Which facts a query lists.
    FactFilter {
        /// In topic files.
        Active => "active",
        /// Waiting in `pending/`.
        Pending => "pending",
        /// Both.
        All => "all"
    }
);

/// A query over facts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactQuery {
    /// The Space.
    pub space: SpaceId,
    /// Only this topic.
    pub topic: Option<TopicPath>,
    /// Only facts linked to this thing.
    pub about: Option<ThingRef>,
    /// Which facts.
    pub state: FactFilter,
    /// How many at most.
    pub limit: Count,
}

/// An app saying why a file changed: joined with the watcher's observation by path and
/// content within a short window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileWhyClaim {
    /// The Space.
    pub space: SpaceId,
    /// The file.
    pub path: SpacePath,
    /// Its content digest as the app wrote it.
    pub content: ContentDigest,
    /// The thing it came from.
    pub cause: ThingRef,
    /// What the app did.
    pub verb: Verb,
    /// Who.
    pub by: Actor,
}

slug_enum!(
    /// A mark on a thing.
    MarkKind {
        /// Never record events about it.
        DoNotRemember => "do_not_remember",
        /// Remove the mark.
        Clear => "clear"
    }
);

/// Mark or unmark a thing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarkRequest {
    /// The Space.
    pub space: SpaceId,
    /// The thing.
    pub thing: ThingRef,
    /// What to do.
    pub mark: MarkKind,
}

slug_enum!(
    /// Whether the export carries the digest subkey (so a third party can check bodies).
    VerificationKey {
        /// Leave it out (default).
        Omit => "omit",
        /// Include it, because the person ticked the box.
        Include => "include"
    }
);

/// What to export. The tar stream goes to the fd that travels beside the request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExportOptions {
    /// Which Spaces (empty: all).
    pub spaces: Vec<SpaceId>,
    /// Whether to include the digest subkey.
    pub verification_key: VerificationKey,
}
