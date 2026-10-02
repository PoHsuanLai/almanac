//! The hash chain's public shapes: heads, checkpoints and what verification reports.

use crate::ids::Seq;
use crate::text::Link32;
use porter_core::Count;
use serde::{Deserialize, Serialize};

/// The newest entry of a log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Head {
    /// Its sequence number.
    pub seq: Seq,
    /// Its link.
    pub link: Link32,
}

/// A point verification may start from: the log was intact up to `cut` and its link there was
/// `link`. Genesis is `cut = 0` with the genesis link; pruning a prefix leaves one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Checkpoint {
    /// The last sequence number covered.
    pub cut: Seq,
    /// The link at `cut`.
    pub link: Link32,
}

/// What `verify_chain` found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ChainReport {
    /// Every link and every present body checks out.
    Intact {
        /// The newest entry.
        head: Head,
        /// How many bodies are erased (forgotten, expired or never stored).
        erased: Count,
    },
    /// The first entry that does not.
    Broken {
        /// Where.
        at: Seq,
        /// Why.
        why: Break,
    },
}

/// How a chain can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Break {
    /// A header was edited, or `prev` does not point at the entry before.
    LinkMismatch,
    /// A sequence number is missing, repeated or out of order.
    Gap,
    /// A present body does not match its digest.
    BodyDigestMismatch,
    /// The first entry does not continue from the checkpoint.
    CheckpointMismatch,
}
