//! The retention sweep: bodies that have outlived their keep are erased (their headers stay),
//! and a prefix of headers whose bodies are gone and which are a year old is pruned behind a
//! checkpoint. Run by the `Sweep` request and by memoryd's timer.

use crate::backend::Backend;
use crate::docs::{indexed_docs, newest_episodes};
use crate::open::{Cx, Open, log_refusal};
use crate::retention::{SourceState, Sweep, header_expired, sweep_body};
use almanac_core::{
    Count, EventBody, FileChange, MemoryOp, Record, Refusal, Retention, Seq, SpaceState,
    SweepReport, Verb,
};
use eventlog::{BodyState, Entry, LogWrite};

fn count(n: usize) -> Count {
    Count(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Whether what the entry at `at` is about is gone: it says so itself (a deletion) or a later
/// entry deleted it.
pub(crate) fn source_state(entries: &[Entry], at: usize) -> SourceState {
    let (BodyState::Present(body), later) = (&entries[at].body, &entries[at + 1..]) else {
        return SourceState::Exists;
    };
    let says_deleted = matches!(
        body,
        EventBody::Thing {
            verb: Verb::Deleted,
            ..
        } | EventBody::File {
            change: FileChange::Deleted,
            ..
        }
    );
    if says_deleted {
        return SourceState::Gone;
    }
    let deleted = |other: &Entry| match (&other.body, body) {
        (
            BodyState::Present(EventBody::File { change, file, .. }),
            EventBody::File { file: mine, .. },
        ) => *change == FileChange::Deleted && file.path == mine.path,
        (
            BodyState::Present(EventBody::Thing { verb, thing, .. }),
            EventBody::Thing { thing: mine, .. },
        ) => *verb == Verb::Deleted && thing.thing == mine.thing,
        _ => false,
    };
    if later.iter().any(deleted) {
        SourceState::Gone
    } else {
        SourceState::Exists
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
            match almanac_core::admit(&record, &cx.rules, &SpaceState::Open, &marks) {
                almanac_core::Admission::Keep { retention }
                | almanac_core::Admission::HeaderOnly { retention } => retention,
                almanac_core::Admission::Drop(_) => {
                    almanac_core::default_retention(&record, &cx.rules)
                }
            },
        )
    }

    /// Erases the bodies that have expired and prunes the headers that have too.
    pub(crate) fn sweep(&mut self, cx: &Cx<'_, B>) -> Result<SweepReport, Refusal> {
        let now = cx.now();
        self.age_pending(now)?;
        let entries = self.entries()?;
        let expired: Vec<(Seq, &Entry)> = entries
            .iter()
            .enumerate()
            .filter_map(|(at, e)| {
                let retention = self.entry_retention(cx, e)?;
                (sweep_body(
                    retention,
                    e.header.occurred,
                    now,
                    source_state(&entries, at),
                ) == Sweep::EraseBody)
                    .then_some((e.header.seq, e))
            })
            .collect();
        let seqs: Vec<Seq> = expired.iter().map(|(s, _)| *s).collect();
        if !seqs.is_empty() {
            let newest = newest_episodes(&entries);
            let ids: Vec<String> = expired
                .iter()
                .flat_map(|(_, e)| indexed_docs(self.space(), e, &newest))
                .map(|d| d.id)
                .collect();
            self.index_drop(&ids);
            self.rt.log.erase_bodies(&seqs).map_err(log_refusal)?;
            self.notes.expired.extend(seqs.iter().copied());
        }
        let headers = self.prune_expired_headers(now, &entries, &seqs)?;
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
        entries: &[Entry],
        just_erased: &[Seq],
    ) -> Result<Count, Refusal> {
        let prefix: Vec<&Entry> = entries
            .iter()
            .take_while(|e| {
                let gone =
                    matches!(e.body, BodyState::Erased) || just_erased.contains(&e.header.seq);
                gone && header_expired(e.header.occurred, now)
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
