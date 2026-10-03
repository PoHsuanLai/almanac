//! The shell's control requests that read or change one Space: status, timeline, pause and
//! resume, verification, and why a body is gone.

use crate::backend::Backend;
use crate::open::{Cx, Open, log_refusal};
use crate::retention::{SourceState, Sweep, sweep_body};
use crate::space::{SpaceEffect, SpaceEvent, step};
use crate::timeline::timeline_entry;
use almanac_core::{
    Bytes, ChainHealth, ChainReport, Count, Cursor, DegradedWhy, EraseCause, FALLBACK_DAYS,
    IndexView, MemoryOp, Refusal, Retention, Seq, SpaceStatus, StaleWhy, TimelinePage,
    TimelineQuery, UnixSeconds,
};
use eventlog::{Entry, LogRead, verify_chain};
use memfiles::{Vault, VaultPath};
use recall::IndexState;

fn index_view(state: IndexState) -> IndexView {
    let n = |c: recall::Count| Count(c.0);
    match state {
        IndexState::Absent => IndexView::Absent,
        IndexState::Building { done, total } => IndexView::Building {
            done: n(done),
            total: n(total),
        },
        IndexState::Ready => IndexView::Ready,
        IndexState::Stale(why) => IndexView::Stale(match why {
            recall::StaleWhy::EmbedderChanged => StaleWhy::EmbedderChanged,
            recall::StaleWhy::FormatChanged => StaleWhy::FormatChanged,
            recall::StaleWhy::TruthNewer => StaleWhy::TruthNewer,
        }),
        IndexState::LexicalOnly(why) => IndexView::LexicalOnly(match why {
            recall::DegradedWhy::EmbedderUnavailable => DegradedWhy::EmbedderUnavailable,
            recall::DegradedWhy::EmbedderRefused => DegradedWhy::EmbedderRefused,
        }),
    }
}

impl<B: Backend> Open<B> {
    /// Why an erased body is gone: this run knows what it forgot and what was only ever a
    /// header; otherwise retention says whether it expired, and anything else was forgotten.
    pub(crate) fn erase_cause(&self, cx: &Cx<'_, B>, entry: &Entry) -> EraseCause {
        let seq = entry.header.seq;
        if self.notes.header_only.contains(&seq) {
            return EraseCause::HeaderOnly;
        }
        if self.notes.forgotten.contains(&seq) {
            return EraseCause::Forgotten;
        }
        if self.notes.expired.contains(&seq) {
            return EraseCause::Expired;
        }
        let kind = entry.header.kind.as_str();
        let retention = cx
            .rules
            .defaults
            .iter()
            .filter(|d| d.kind.covers(kind))
            .max_by_key(|d| d.kind.specificity())
            .map_or(Retention::Days(FALLBACK_DAYS), |d| d.retention);
        match sweep_body(
            retention,
            entry.header.occurred,
            cx.now(),
            SourceState::Exists,
        ) {
            Sweep::EraseBody => EraseCause::Expired,
            Sweep::Keep => EraseCause::Forgotten,
        }
    }

    /// A page of the timeline, newest first.
    pub(crate) fn timeline(
        &self,
        cx: &Cx<'_, B>,
        q: &TimelineQuery,
    ) -> Result<TimelinePage, Refusal> {
        let entries = self.rt.log.page(q).map_err(log_refusal)?;
        let stored = self.stored()?;
        let rows: Vec<_> = entries
            .iter()
            .map(|e| {
                timeline_entry(
                    self.space(),
                    e,
                    self.erase_cause(cx, e),
                    self.derived_from(&stored, &self.event_ref(e)),
                )
            })
            .collect();
        let full = usize::try_from(q.limit.0).unwrap_or(usize::MAX) <= entries.len();
        let next = entries
            .last()
            .filter(|_| full)
            .map(|e| Cursor(e.header.seq));
        Ok(TimelinePage {
            entries: rows,
            next,
        })
    }

    pub(crate) fn status(&self) -> Result<SpaceStatus, Refusal> {
        let head = self.rt.log.head().map_err(log_refusal)?;
        let cut = self.rt.log.checkpoint().map_err(log_refusal)?.cut;
        let (facts, pending) = self.fact_counts()?;
        let files = [VaultPath::facts_dir(), VaultPath::pending_dir()]
            .iter()
            .flat_map(|dir| self.rt.store.vault().list(dir).unwrap_or_default())
            .map(|p| self.rt.store.vault().read(&p).map_or(0, |b| b.len() as u64))
            .sum::<u64>();
        Ok(SpaceStatus {
            state: self.rt.state,
            index: index_view(self.rt.index.state()),
            chain: self.chain,
            usage: Bytes(files),
            events: Count(u32::try_from(head.seq.0.saturating_sub(cut.0)).unwrap_or(u32::MAX)),
            facts,
            pending,
            last_run: self.last.as_ref().map(|l| l.run.clone()),
        })
    }

    pub(crate) fn pause(&mut self, cx: &Cx<'_, B>, until: UnixSeconds) -> Result<(), Refusal> {
        self.tick(cx)?;
        let (state, effects) = step(self.rt.state, SpaceEvent::Pause { until });
        self.rt.state = state;
        self.apply_space_effects(cx, effects)
    }

    pub(crate) fn resume(&mut self, cx: &Cx<'_, B>) -> Result<(), Refusal> {
        let (state, effects) = step(self.rt.state, SpaceEvent::Resume);
        self.rt.state = state;
        self.apply_space_effects(cx, effects)
    }

    fn apply_space_effects(
        &mut self,
        cx: &Cx<'_, B>,
        effects: Vec<SpaceEffect>,
    ) -> Result<(), Refusal> {
        for effect in effects {
            match effect {
                SpaceEffect::LogPaused { until } => {
                    self.audit(cx.now(), MemoryOp::Paused { until })?;
                }
                SpaceEffect::LogResumed => {
                    self.audit(cx.now(), MemoryOp::Resumed)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Checks the hash chain from the checkpoint to the head.
    pub(crate) fn verify(&mut self) -> Result<ChainReport, Refusal> {
        let from = self.rt.log.checkpoint().map_err(log_refusal)?;
        let entries = self.rt.log.scan(Seq(from.cut.0 + 1)).map_err(log_refusal)?;
        let report = verify_chain(&from, &entries, &self.digest);
        self.chain = ChainHealth::Checked(report);
        Ok(report)
    }
}
