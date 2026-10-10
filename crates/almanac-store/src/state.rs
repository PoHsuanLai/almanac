//! `IndexState` and its machine (the pure half of memory's section 4.5).

use crate::doc::Count;

/// Why an index is out of date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StaleWhy {
    /// The configured embedder differs from the one the vectors came from.
    EmbedderChanged,
    /// The index format changed.
    FormatChanged,
    /// The truth (files, log) is newer than the index.
    TruthNewer,
}

/// Why search is lexical only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DegradedWhy {
    /// No embedder is reachable.
    EmbedderUnavailable,
    /// The embedder refused.
    EmbedderRefused,
}

/// Where an index is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IndexState {
    /// Not built.
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
    /// Out of date; rebuild.
    Stale(StaleWhy),
    /// Vectors unavailable; lexical search only.
    LexicalOnly(DegradedWhy),
}

/// What happened to an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexEvent {
    /// A (re)build starts over `total` documents.
    Begin {
        /// How many.
        total: Count,
    },
    /// The build advanced.
    Progress {
        /// Documents done.
        done: Count,
    },
    /// The build finished.
    Finished,
    /// The configured embedder is not the one the vectors came from.
    CardChanged,
    /// The index format changed.
    FormatChanged,
    /// The files or log changed behind the index.
    TruthChanged,
    /// The embedder became unavailable.
    EmbedderLost(DegradedWhy),
}

/// The next state: absent, then building, then ready; ready goes stale or lexical-only;
/// stale and lexical-only build again when the embedder returns (`Begin`). Events that do not
/// apply leave the state as it is.
pub fn step(state: IndexState, event: IndexEvent) -> IndexState {
    match (state, event) {
        (
            IndexState::Absent
            | IndexState::Stale(_)
            | IndexState::LexicalOnly(_)
            | IndexState::Ready,
            IndexEvent::Begin { total },
        ) => IndexState::Building {
            done: Count(0),
            total,
        },
        (IndexState::Building { total, .. }, IndexEvent::Progress { done }) => {
            IndexState::Building { done, total }
        }
        (IndexState::Building { .. }, IndexEvent::Finished) => IndexState::Ready,
        (IndexState::Ready, IndexEvent::CardChanged) => {
            IndexState::Stale(StaleWhy::EmbedderChanged)
        }
        (IndexState::Ready, IndexEvent::FormatChanged) => {
            IndexState::Stale(StaleWhy::FormatChanged)
        }
        (IndexState::Ready, IndexEvent::TruthChanged) => IndexState::Stale(StaleWhy::TruthNewer),
        (IndexState::Ready | IndexState::Building { .. }, IndexEvent::EmbedderLost(why)) => {
            IndexState::LexicalOnly(why)
        }
        (unchanged, _) => unchanged,
    }
}
