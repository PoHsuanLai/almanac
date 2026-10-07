//! Which events recall may use: the "not for recall" kind class.
//!
//! A session log (`companion.session.*`) is kept verbatim so a session can be rebuilt after a
//! restart. It holds model output and text derived from untrusted reads, so it must never become a
//! search hit, an injected passage, a consolidated fact or a line of the primer. The class is a
//! rule over the event's kind, not a flag on the record: a writer cannot forget to set it, a new
//! session kind is covered at once, and the index and consolidation can be rebuilt from the log
//! alone because the rule is a pure function of what the log already stores.

use crate::event::EventBody;
use crate::ids::{KindPattern, KindTag};
use crate::slug::slug_enum;

slug_enum!(
    /// Whether recall (search, inject, the primer, consolidation, related events) may use an
    /// event. `Recent`, `Entries`, `Timeline` and export return every event either way.
    Recallable {
        /// Recall may use it.
        Yes => "yes",
        /// Kept verbatim and returned by `Recent`, `Entries` and export, never recalled.
        No => "no"
    }
);

/// The kind patterns that are not for recall.
const NOT_FOR_RECALL: [&str; 1] = ["companion.session.*"];

impl Recallable {
    /// The class of a kind.
    pub fn of_kind(kind: &KindTag) -> Recallable {
        let covered = NOT_FOR_RECALL
            .iter()
            .filter_map(|p| KindPattern::parse(p).ok())
            .any(|p| p.covers(kind.as_str()));
        if covered {
            Recallable::No
        } else {
            Recallable::Yes
        }
    }

    /// The class of a body, by its kind.
    pub fn of_body(body: &EventBody) -> Recallable {
        Recallable::of_kind(&body.kind())
    }
}
