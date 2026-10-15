//! The retention sweep: bodies that have outlived their keep are erased (their headers stay),
//! and a prefix of headers whose bodies are gone and which are a year old is pruned behind a
//! checkpoint. Run by the `Sweep` request and by memoryd's timer.

use crate::backend::Backend;
use crate::docs::{indexed_docs, newest_episodes};
use crate::open::{Cx, Open, log_refusal};
use crate::retention::{SourceState, Sweep, header_expired_after, sweep_body};
use almanac_core::{
    Count, EventBody, FileChange, MemoryOp, Record, Refusal, Retention, Seq, SpacePath, SpaceState,
    SweepReport, ThingRef, Verb,
};
use almanac_store::{BodyState, Entry, LogWrite};
use std::collections::HashMap;

fn count(n: usize) -> Count {
    Count(u32::try_from(n).unwrap_or(u32::MAX))
}

/// How many bodies one erase call takes: a big Space is swept in batches, so no single
/// transaction (and no single index drop) grows with the log.
pub(crate) const SWEEP_BATCH: usize = 512;

/// The last entry that deleted each file path and each thing, built once per pass so that asking
/// whether an entry's source is gone costs a lookup, not a scan of the entries after it.
pub(crate) struct Deletions {
    files: HashMap<SpacePath, usize>,
    things: HashMap<ThingRef, usize>,
}

impl Deletions {
    /// The deletions in `entries` (oldest first), in one pass.
    pub(crate) fn of(entries: &[Entry]) -> Self {
        let mut files = HashMap::new();
        let mut things = HashMap::new();
        for (at, entry) in entries.iter().enumerate() {
            match &entry.body {
                BodyState::Present(EventBody::File {
                    change: FileChange::Deleted,
                    file,
                    ..
                }) => {
                    files.insert(file.path.clone(), at);
                }
                BodyState::Present(EventBody::Thing {
                    verb: Verb::Deleted,
                    thing,
                    ..
                }) => {
                    things.insert(thing.thing.clone(), at);
                }
                _ => {}
            }
        }
        Self { files, things }
    }

    /// Whether what the entry at `at` is about is gone: it says so itself (a deletion) or a
    /// later entry deleted it.
    pub(crate) fn source_state(&self, entries: &[Entry], at: usize) -> SourceState {
        let BodyState::Present(body) = &entries[at].body else {
            return SourceState::Exists;
        };
        let gone = match body {
            EventBody::Thing {
                verb: Verb::Deleted,
                ..
            }
            | EventBody::File {
                change: FileChange::Deleted,
                ..
            } => true,
            EventBody::File { file, .. } => self.files.get(&file.path).is_some_and(|d| *d > at),
            EventBody::Thing { thing, .. } => {
                self.things.get(&thing.thing).is_some_and(|d| *d > at)
            }
            _ => false,
        };
        match gone {
            true => SourceState::Gone,
            false => SourceState::Exists,
        }
    }
}

impl<B: Backend> Open<B> {
    /// How long the body of `entry` is kept: what admission would say for it now (a rule, else
    /// the default of its kind). `None` when the body is already gone.
    pub(crate) fn entry_retention(&self, cx: &Cx<'_, B>, entry: &Entry) -> Option<Retention> {
        let BodyState::Present(body) = &entry.body else {
            return None;
        };
        let h = &entry.header;
        let record = Record {
            space: self.space().clone(),
            occurred: h.occurred,
            actor: h.actor.clone(),
            effect: h.effect,
            label: h.label.clone(),
            body: body.clone(),
            cause: h.cause.clone(),
        };
        let marks = almanac_core::Marks::default();
        Some(
            match almanac_core::admit_with(
                &record,
                &cx.rules,
                &SpaceState::Open,
                &marks,
                cx.settings.retention.file_unexplained,
            ) {
                almanac_core::Admission::Keep { retention }
                | almanac_core::Admission::HeaderOnly { retention } => retention,
                almanac_core::Admission::Drop(_) => almanac_core::default_retention_with(
                    &record,
                    &cx.rules,
                    cx.settings.retention.file_unexplained,
                ),
            },
        )
    }

    /// Erases the bodies that have expired and prunes the headers that have too.
    pub(crate) fn sweep(&mut self, cx: &Cx<'_, B>) -> Result<SweepReport, Refusal> {
        let now = cx.now();
        self.age_pending(now, cx.settings.pending_ttl)?;
        let entries = self.entries()?;
        let deletions = Deletions::of(&entries);
        let expired: Vec<(Seq, &Entry)> = entries
            .iter()
            .enumerate()
            .filter_map(|(at, e)| {
                let retention = self.entry_retention(cx, e)?;
                (sweep_body(
                    retention,
                    e.header.occurred,
                    now,
                    deletions.source_state(&entries, at),
                ) == Sweep::EraseBody)
                    .then_some((e.header.seq, e))
            })
            .collect();
        let seqs: Vec<Seq> = expired.iter().map(|(s, _)| *s).collect();
        if !seqs.is_empty() {
            let newest = newest_episodes(&entries);
            for batch in expired.chunks(SWEEP_BATCH) {
                let ids: Vec<String> = batch
                    .iter()
                    .flat_map(|(_, e)| indexed_docs(self.space(), e, &newest))
                    .map(|d| d.id)
                    .collect();
                let batch_seqs: Vec<Seq> = batch.iter().map(|(s, _)| *s).collect();
                self.index_drop(&ids);
                self.rt.log.erase_bodies(&batch_seqs).map_err(log_refusal)?;
                self.notes.expired.extend(batch_seqs);
            }
            // What the Space holds changed: its usage is counted afresh at the next record.
            self.usage = None;
        }
        let headers =
            self.prune_expired_headers(now, cx.settings.retention.audit_header, &entries, &seqs)?;
        Ok(SweepReport {
            bodies: count(seqs.len()),
            headers,
        })
    }

    /// Prunes the longest prefix of entries whose bodies are gone and whose headers are a year
    /// old, behind a checkpoint, and says so in the audit.
    fn prune_expired_headers(
        &mut self,
        now: almanac_core::UnixSeconds,
        header_days: almanac_core::DayCount,
        entries: &[Entry],
        just_erased: &[Seq],
    ) -> Result<Count, Refusal> {
        let prefix: Vec<&Entry> = entries
            .iter()
            .take_while(|e| {
                let gone =
                    matches!(e.body, BodyState::Erased) || just_erased.contains(&e.header.seq);
                gone && header_expired_after(header_days, e.header.occurred, now)
            })
            .collect();
        let Some(last) = prefix.last() else {
            return Ok(Count(0));
        };
        let cut = last.header.seq;
        let checkpoint = self.rt.log.prune_before(cut).map_err(log_refusal)?;
        self.audit(
            now,
            MemoryOp::Checkpoint {
                cut,
                link: checkpoint.link,
            },
        )?;
        Ok(count(prefix.len()))
    }
}
