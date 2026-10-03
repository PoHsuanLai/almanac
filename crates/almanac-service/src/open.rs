//! One open Space as the service holds it: the frozen `SpaceRuntime` plus what only the service
//! needs (the digest subkey, the last verification, why bodies are gone, the last consolidation),
//! and the helpers every request shares: appending, auditing, the fact snapshot, the index.

use crate::backend::Backend;
use crate::clock::Clock;
use crate::docs::{event_ref, fact_doc, indexed_docs, newest_episodes};
use crate::forget::{FactGraph, FactNode};
use crate::service::SpaceRuntime;
use almanac_core::{
    Actor, Caller, ChainHealth, Count, Effect, EventBody, EventRef, Fact, FactId, FactState, Label,
    Link, MemoryOp, Record, Refusal, RuleSet, RunId, RunState, Seq, SpaceId, SystemPart, TopicPath,
    UnixSeconds,
};
use almanac_seal::SubKey;
use eventlog::{BodyState, Entry, LogError, LogRead, LogWrite, NewHeader};
use memfiles::{Block, MemfilesError, Vault, VaultPath};
use recall::{Doc, DocId};
use std::collections::{BTreeMap, BTreeSet};

/// What one request runs against: the seams, the rules as they are now and who is asking.
pub(crate) struct Cx<'a, B: Backend> {
    pub backend: &'a B,
    pub rules: RuleSet,
    pub caller: &'a Caller,
}

impl<B: Backend> Cx<'_, B> {
    pub(crate) fn now(&self) -> UnixSeconds {
        self.backend.clock().now()
    }
}

/// Why bodies in this Space are gone, as far as this run of the daemon knows.
#[derive(Debug, Default)]
pub(crate) struct ErasedNotes {
    pub forgotten: BTreeSet<Seq>,
    pub expired: BTreeSet<Seq>,
    pub header_only: BTreeSet<Seq>,
}

/// The last consolidation run: what the person can review and revert.
#[derive(Debug, Clone)]
pub(crate) struct LastRun {
    pub run: RunId,
    pub view: almanac_core::DraftView,
    pub added: Vec<FactId>,
    pub superseded: Vec<FactId>,
    /// Files the run rewrote, as they were.
    pub pre_images: Vec<crate::hunks::PreImage>,
    /// Topics whose facts the run reworded.
    pub topics: Vec<TopicPath>,
    pub cut: Seq,
}

/// A Space the service has open.
pub(crate) struct Open<B: Backend> {
    pub rt: SpaceRuntime<B>,
    pub digest: SubKey,
    pub chain: ChainHealth,
    pub minted: u64,
    pub notes: ErasedNotes,
    pub run: RunState,
    pub last: Option<LastRun>,
    /// Topic files as the service last left them (`baseline.rs`).
    pub baseline: crate::baseline::Baseline,
    /// Topics found edited by someone else when the request began.
    pub dirty: BTreeSet<TopicPath>,
    /// What happened during the request that the bus should hear of (drained by the service).
    pub outbox: Vec<crate::events::ServiceEvent>,
}

/// A fact as the files hold it.
#[derive(Debug, Clone)]
pub(crate) struct Stored {
    pub topic: TopicPath,
    pub fact: Fact,
    pub state: FactState,
}

pub(crate) fn failed(e: impl std::fmt::Display) -> Refusal {
    Refusal::Invalid(e.to_string())
}

pub(crate) fn log_refusal(e: LogError) -> Refusal {
    match e {
        LogError::Locked => Refusal::SpaceLocked,
        other => failed(other),
    }
}

pub(crate) fn files_refusal(e: MemfilesError) -> Refusal {
    failed(e)
}

impl<B: Backend> Open<B> {
    pub(crate) fn space(&self) -> &SpaceId {
        &self.rt.meta.id
    }

    /// Entropy for an id: a keyed hash over the Space's digest subkey, a counter, the time, the
    /// backend's random bytes and what the id is for. No ambient randomness (CONVENTIONS 4: the
    /// bytes come through `Backend::random`); ids from two machines differ because their Space
    /// keys do, and a restarted daemon's differ because its random bytes do.
    pub(crate) fn entropy(&mut self, cx: &Cx<'_, B>, what: &[u8]) -> [u8; 10] {
        let now = cx.now();
        self.minted += 1;
        let head = self.rt.log.head().map(|h| h.seq.0).unwrap_or_default();
        let mut hasher = blake3::Hasher::new_keyed(self.digest.expose());
        hasher.update(b"QID1");
        hasher.update(&self.minted.to_be_bytes());
        hasher.update(&now.0.to_be_bytes());
        hasher.update(&head.to_be_bytes());
        hasher.update(&cx.backend.random());
        hasher.update(what);
        let mut out = [0u8; 10];
        out.copy_from_slice(&hasher.finalize().as_bytes()[..10]);
        out
    }

    /// Chains `record` into the log with `body` (or as a header only).
    pub(crate) fn append(
        &mut self,
        now: UnixSeconds,
        record: &Record,
        body: Option<EventBody>,
    ) -> Result<Entry, Refusal> {
        let header = NewHeader::of(record, now, &self.digest);
        self.rt.log.append(header, body).map_err(log_refusal)
    }

    /// Logs memoryd's own `op`, whatever the admission rules say.
    pub(crate) fn audit(&mut self, now: UnixSeconds, op: MemoryOp) -> Result<Entry, Refusal> {
        let effect = match op {
            MemoryOp::Read { .. } => Effect::Read,
            MemoryOp::Forgot { .. } => Effect::Destructive,
            _ => Effect::UndoableWrite,
        };
        let record = Record {
            space: self.space().clone(),
            occurred: now,
            actor: Actor::System {
                part: SystemPart::Memory,
            },
            effect,
            label: Label::trusted_user(),
            body: EventBody::Memory { op },
            cause: almanac_core::Cause::None,
        };
        let body = record.body.clone();
        self.append(now, &record, Some(body))
    }

    pub(crate) fn event_ref(&self, entry: &Entry) -> EventRef {
        event_ref(self.space(), entry)
    }

    /// The entry with sequence number `seq`, if the log still has it.
    pub(crate) fn entry_at(&self, seq: Seq) -> Option<Entry> {
        let q = almanac_core::TimelineQuery {
            before: Some(almanac_core::Cursor(Seq(seq.0 + 1))),
            limit: Count(1),
            filter: any_filter(),
        };
        self.rt
            .log
            .page(&q)
            .ok()?
            .into_iter()
            .find(|e| e.header.seq == seq)
    }

    /// Every retained entry, oldest first.
    pub(crate) fn entries(&self) -> Result<Vec<Entry>, Refusal> {
        self.rt.log.scan(Seq(0)).map_err(log_refusal)
    }

    /// Every fact the files hold: active (or superseded by a newer active one) and pending.
    pub(crate) fn stored(&self) -> Result<Vec<Stored>, Refusal> {
        let store = &self.rt.store;
        let mut active: Vec<(TopicPath, Fact)> = Vec::new();
        for topic in store.topics().map_err(files_refusal)? {
            let file = store.read(&topic).map_err(files_refusal)?;
            active.extend(file.blocks.into_iter().filter_map(|b| match b {
                Block::Fact(f) => Some((topic.clone(), f)),
                Block::Unstamped(_) | Block::Verbatim(_) => None,
            }));
        }
        let replaced: BTreeMap<FactId, FactId> = active
            .iter()
            .flat_map(|(_, f)| f.supersedes.iter().map(|old| (old.clone(), f.id.clone())))
            .collect();
        let pending = store.pending().map_err(files_refusal)?;
        Ok(active
            .into_iter()
            .map(|(topic, fact)| {
                let state = match replaced.get(&fact.id) {
                    Some(by) => FactState::Superseded { by: by.clone() },
                    None => FactState::Active,
                };
                Stored { topic, fact, state }
            })
            .chain(pending.into_iter().map(|(topic, fact)| Stored {
                topic,
                fact,
                state: FactState::Pending,
            }))
            .collect())
    }

    /// The facts and procedures as the planner reads them.
    pub(crate) fn fact_graph(&self) -> Result<FactGraph, Refusal> {
        let nodes = self
            .stored()?
            .into_iter()
            .map(|s| FactNode {
                id: s.fact.id,
                links: s.fact.links,
                state: s.state,
            })
            .collect();
        let procedures = VaultPath::parse("procedures")
            .map(|dir| self.rt.store.vault().list(&dir).unwrap_or_default())
            .unwrap_or_default();
        Ok(FactGraph { nodes, procedures })
    }

    /// Adds or replaces documents in the index. A failed index never fails a write: the files
    /// and the log are the truth and the index rebuilds from them.
    pub(crate) async fn index_put(&mut self, cx: &Cx<'_, B>, docs: Vec<Doc>) {
        if !docs.is_empty() {
            let _ = self.rt.index.upsert(&docs, cx.backend.embedder()).await;
        }
    }

    /// Removes documents by id.
    pub(crate) fn index_drop(&mut self, ids: &[String]) {
        let ids: Vec<DocId> = ids.iter().cloned().map(DocId).collect();
        if !ids.is_empty() {
            let _ = self.rt.index.remove(&ids);
        }
    }

    /// Everything the index holds, from the files and the log: active facts that nothing
    /// replaced, and the newest event of each episode.
    pub(crate) fn truth_docs(&self) -> Result<Vec<Doc>, Refusal> {
        let entries = self.entries()?;
        let newest = newest_episodes(&entries);
        let facts = self
            .stored()?
            .into_iter()
            .filter(|s| s.state == FactState::Active)
            .map(|s| fact_doc(&s.fact));
        let events = entries.iter().flat_map(|e| {
            indexed_docs(self.space(), e, &newest)
                .into_iter()
                .map(|d| d.into_doc(e.header.occurred))
        });
        Ok(facts.chain(events).collect())
    }

    /// Rebuilds the index from the truth.
    pub(crate) async fn rebuild_index(&mut self, cx: &Cx<'_, B>) -> Result<(), Refusal> {
        let docs = self.truth_docs()?;
        self.rt
            .index
            .rebuild(docs.into_iter(), cx.backend.embedder())
            .await
            .map_err(|e| match e {
                recall::IndexError::Embed(_) => Refusal::Busy,
                other => failed(other),
            })
    }

    /// Brings the index in line with the truth at open: an index the embedder's own is adopted
    /// (only what changed is embedded), anything else is rebuilt (`Index::sync`). An embedder
    /// that fails does not stop the Space opening: the index says why in its state and the next
    /// start tries again.
    pub(crate) async fn sync_index(&mut self, cx: &Cx<'_, B>) -> Result<(), Refusal> {
        let docs = self.truth_docs()?;
        match self
            .rt
            .index
            .sync(docs.into_iter(), cx.backend.embedder())
            .await
        {
            Ok(()) | Err(recall::IndexError::Embed(_)) => Ok(()),
            Err(other) => Err(failed(other)),
        }
    }

    /// The links of a fact that name events, for the timeline's derived counts.
    pub(crate) fn derived_from(&self, stored: &[Stored], event: &EventRef) -> Count {
        let n = stored
            .iter()
            .filter(|s| s.fact.links.contains(&Link::Event(event.clone())))
            .count();
        Count(u32::try_from(n).unwrap_or(u32::MAX))
    }
}

/// The filter that shows everything.
pub(crate) fn any_filter() -> almanac_core::TimelineFilter {
    almanac_core::TimelineFilter {
        actors: almanac_core::ActorFilter::Everyone,
        apps: Vec::new(),
        kinds: Vec::new(),
        trust: almanac_core::TrustFilter::Any,
        range: None,
    }
}

/// Whether the body of `entry` is still there.
pub(crate) fn present(entry: &Entry) -> Option<&EventBody> {
    match &entry.body {
        BodyState::Present(body) => Some(body),
        BodyState::Erased => None,
    }
}
