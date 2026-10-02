//! `MemoryLog`: the log in a vector, for tests and almanac-fake. The contract every log meets.

use crate::chain::{BodyState, Entry};
use crate::filter::passes;
use crate::header::{NewHeader, body_digest, genesis_link, link};
use crate::traits::{LogError, LogRead, LogWrite, PageQuery, RoleFilter};
use almanac_core::{
    Checkpoint, Count, EventBody, Head, ReplicaId, Seq, SpaceId, ThingRef, ThingRole,
};
use almanac_seal::SubKey;

/// An in-memory log with the same behaviour as `SqliteLog`.
#[derive(Debug)]
pub struct MemoryLog {
    digest_key: SubKey,
    start: Checkpoint,
    replica: ReplicaId,
    entries: Vec<Entry>,
}

impl MemoryLog {
    /// An empty log for `space` on `replica`, whose bodies are digested with `digest_key`.
    pub fn new(space: &SpaceId, replica: ReplicaId, digest_key: SubKey) -> Self {
        Self {
            digest_key,
            start: Checkpoint {
                cut: Seq(0),
                link: genesis_link(space, &replica),
            },
            replica,
            entries: Vec::new(),
        }
    }

    fn tip(&self) -> Head {
        self.entries.last().map_or(
            Head {
                seq: self.start.cut,
                link: self.start.link,
            },
            |e| Head {
                seq: e.header.seq,
                link: e.link,
            },
        )
    }
}

impl LogRead for MemoryLog {
    fn head(&self) -> Result<Head, LogError> {
        Ok(self.tip())
    }

    fn checkpoint(&self) -> Result<Checkpoint, LogError> {
        Ok(self.start)
    }

    fn page(&self, q: &PageQuery) -> Result<Vec<Entry>, LogError> {
        let limit = usize::try_from(q.limit.0).unwrap_or(usize::MAX);
        Ok(self
            .entries
            .iter()
            .rev()
            .filter(|e| q.before.is_none_or(|c| e.header.seq < c.0))
            .filter(|e| passes(&q.filter, e))
            .take(limit)
            .cloned()
            .collect())
    }

    fn touching(&self, thing: &ThingRef, role: RoleFilter) -> Result<Vec<Seq>, LogError> {
        let wanted = |r: ThingRole| match role {
            RoleFilter::Either => true,
            RoleFilter::Subject => r == ThingRole::Subject,
            RoleFilter::Source => r == ThingRole::Source,
        };
        Ok(self
            .entries
            .iter()
            .filter(|e| match &e.body {
                BodyState::Present(body) => body
                    .things()
                    .iter()
                    .any(|(view, r)| &view.thing == thing && wanted(*r)),
                BodyState::Erased => false,
            })
            .map(|e| e.header.seq)
            .collect())
    }

    fn scan(&self, from: Seq) -> Result<Vec<Entry>, LogError> {
        Ok(self
            .entries
            .iter()
            .filter(|e| e.header.seq >= from)
            .cloned()
            .collect())
    }
}

impl LogWrite for MemoryLog {
    fn append(&mut self, header: NewHeader, body: Option<EventBody>) -> Result<Entry, LogError> {
        if let Some(body) = &body
            && body_digest(&self.digest_key, body) != header.body_digest
        {
            return Err(LogError::BadDigest);
        }
        let tip = self.tip();
        let chained = header.chained(Seq(tip.seq.0 + 1), self.replica, tip.link);
        let entry = Entry {
            link: link(&chained),
            header: chained,
            body: body.map_or(BodyState::Erased, BodyState::Present),
        };
        self.entries.push(entry.clone());
        Ok(entry)
    }

    fn erase_bodies(&mut self, seqs: &[Seq]) -> Result<Count, LogError> {
        let mut erased: u32 = 0;
        for entry in self
            .entries
            .iter_mut()
            .filter(|e| seqs.contains(&e.header.seq))
        {
            if matches!(entry.body, BodyState::Present(_)) {
                entry.body = BodyState::Erased;
                erased += 1;
            }
        }
        Ok(Count(erased))
    }

    fn prune_before(&mut self, cut: Seq) -> Result<Checkpoint, LogError> {
        let Some(at) = self.entries.iter().find(|e| e.header.seq == cut) else {
            return Err(LogError::NoSuchEntry(cut));
        };
        let checkpoint = Checkpoint { cut, link: at.link };
        self.entries.retain(|e| e.header.seq > cut);
        self.start = checkpoint;
        Ok(checkpoint)
    }
}
