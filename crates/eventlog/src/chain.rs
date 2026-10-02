//! Entries and the chain check.

use crate::header::{Header, body_digest, link};
use almanac_core::{Break, ChainReport, Checkpoint, Count, EventBody, Head, Seq};
use almanac_seal::SubKey;

/// Whether an entry still has its body.
// `Present` is the stored body as it is; boxing it would complicate every match for no saving.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyState {
    /// The body.
    Present(EventBody),
    /// Forgotten, expired or never stored; the header and its digest remain.
    Erased,
}

/// One log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The chained header.
    pub header: Header,
    /// Its link.
    pub link: almanac_core::Link32,
    /// Its body.
    pub body: BodyState,
}

/// Checks `entries` (ascending, the first following `from`): sequence numbers gapless, every
/// header's link recomputed and `prev` pointing at the entry before, every present body
/// matching its keyed digest. Erased bodies are counted, not checked.
pub fn verify_chain(from: &Checkpoint, entries: &[Entry], digest_key: &SubKey) -> ChainReport {
    let mut expected = Seq(from.cut.0 + 1);
    let mut prev = from.link;
    let mut head = Head {
        seq: from.cut,
        link: from.link,
    };
    let mut erased: u32 = 0;
    for (index, entry) in entries.iter().enumerate() {
        let at = entry.header.seq;
        let broken = |why| ChainReport::Broken { at, why };
        if at != expected {
            return broken(Break::Gap);
        }
        if entry.header.prev != prev {
            return broken(if index == 0 {
                Break::CheckpointMismatch
            } else {
                Break::LinkMismatch
            });
        }
        if link(&entry.header) != entry.link {
            return broken(Break::LinkMismatch);
        }
        match &entry.body {
            BodyState::Present(body)
                if body_digest(digest_key, body) != entry.header.body_digest =>
            {
                return broken(Break::BodyDigestMismatch);
            }
            BodyState::Present(_) => {}
            BodyState::Erased => erased += 1,
        }
        prev = entry.link;
        head = Head {
            seq: at,
            link: entry.link,
        };
        expected = Seq(at.0 + 1);
    }
    ChainReport::Intact {
        head,
        erased: Count(erased),
    }
}
