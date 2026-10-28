//! The index machine over core's `IndexView` (the pure half of memory's section 4.5).

use almanac_core::{Count, DegradedWhy, IndexView, StaleWhy};

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
pub fn step(state: IndexView, event: IndexEvent) -> IndexView {
    match (state, event) {
        (
            IndexView::Absent | IndexView::Stale(_) | IndexView::LexicalOnly(_) | IndexView::Ready,
            IndexEvent::Begin { total },
        ) => IndexView::Building {
            done: Count(0),
            total,
        },
        (IndexView::Building { total, .. }, IndexEvent::Progress { done }) => {
            IndexView::Building { done, total }
        }
        (IndexView::Building { .. }, IndexEvent::Finished) => IndexView::Ready,
        (IndexView::Ready, IndexEvent::CardChanged) => IndexView::Stale(StaleWhy::EmbedderChanged),
        (IndexView::Ready, IndexEvent::FormatChanged) => IndexView::Stale(StaleWhy::FormatChanged),
        (IndexView::Ready, IndexEvent::TruthChanged) => IndexView::Stale(StaleWhy::TruthNewer),
        (IndexView::Ready | IndexView::Building { .. }, IndexEvent::EmbedderLost(why)) => {
            IndexView::LexicalOnly(why)
        }
        (unchanged, _) => unchanged,
    }
}
