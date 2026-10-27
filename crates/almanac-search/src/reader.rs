//! What a search reads from one open Space.

use almanac_core::{EventRef, Fact, Refusal, Seq};
use almanac_store::{Entry, SearchIndex};

/// The narrow view of one Space that `Search` and `Inject` read: the index, the log entry at a
/// sequence number, and the facts the files hold.
pub trait SpaceReader {
    /// The Space's search index.
    type Index: SearchIndex;

    /// The index.
    fn index(&self) -> &Self::Index;

    /// The log entry with sequence number `seq`, if the log still has it.
    fn entry_at(&self, seq: Seq) -> Option<Entry>;

    /// The reference of an entry of this Space's log.
    fn event_ref(&self, entry: &Entry) -> EventRef;

    /// Every fact the files hold: active, superseded and pending.
    fn stored_facts(&self) -> Result<Vec<Fact>, Refusal>;
}
