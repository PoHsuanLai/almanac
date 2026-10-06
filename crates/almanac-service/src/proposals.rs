//! Consolidation proposals as files: the run file written when a run is drafted, read again when
//! the Space opens, rewritten with each outcome. Applying, discarding and superseding are steps of
//! the run machine (`consolidation::step`); this module carries them out and keeps the file and
//! `Open::last` in line.

use crate::backend::Backend;
use crate::consolidation::{RunEvent, step};
use crate::events::ServiceEvent;
use crate::open::{Cx, LastRun, Open, failed};
use crate::runfile::{self, Gone, RecordState, RunRecord};
use almanac_core::{DraftView, Refusal, RunId, RunState, UnixSeconds};

impl<B: Backend> Open<B> {
    /// Writes (or rewrites) the file of `last`: its state, the hunks as they stand, the skipped.
    pub(crate) fn write_run(&self, last: &LastRun, now: UnixSeconds) -> Result<(), Refusal> {
        let vault = self.rt.store.vault();
        let Some(state) = RecordState::of(last.view.state) else {
            return Ok(());
        };
        let (drafted, erased) =
            runfile::load(vault, &last.run).map_or((now, 0), |r| (r.drafted, r.erased));
        let mut rec = RunRecord::new(
            last.run.clone(),
            state,
            drafted,
            last.cut,
            last.head,
            last.view.hunks.clone(),
        )
        .settled_as(state, now);
        rec.skipped = last.view.skipped.clone();
        rec.erased = erased;
        runfile::save(vault, &rec).map_err(failed)
    }

    /// Marks every proposal other than `keep` superseded in its file. Best effort: a file that
    /// cannot be written stays `proposed` and is superseded by `restore_proposal` at the next start.
    pub(crate) fn supersede_others(&self, keep: &RunId, now: UnixSeconds) {
        let vault = self.rt.store.vault();
        for rec in runfile::load_all(vault) {
            let (state, _) = step(rec.state.run_state(), RunEvent::Supersede);
            if let Some(next) = RecordState::of(state).filter(|s| *s != rec.state)
                && &rec.run != keep
            {
                drop(runfile::save(vault, &rec.settled_as(next, now)));
            }
        }
    }

    /// At open: the newest proposal on disk becomes the Space's last run, so the person can apply
    /// or discard it after a restart (apply re-checks every hunk against the Space as it is).
    /// Older proposals still marked `proposed` (a crash between two writes) are superseded.
    pub(crate) fn restore_proposal(&mut self, now: UnixSeconds) {
        let mut proposals: Vec<RunRecord> = runfile::load_all(self.rt.store.vault())
            .into_iter()
            .filter(|r| r.state == RecordState::Proposed)
            .collect();
        proposals.sort_by(|a, b| {
            (a.drafted, a.head, &a.run.to_string()).cmp(&(b.drafted, b.head, &b.run.to_string()))
        });
        let Some(newest) = proposals.pop() else {
            return;
        };
        self.supersede_others(&newest.run, now);
        self.run = RunState::Proposed;
        self.last = Some(LastRun {
            run: newest.run.clone(),
            view: newest.view(),
            added: vec![],
            superseded: vec![],
            pre_images: vec![],
            revert: crate::revert_guard::RevertGuard::default(),
            topics: vec![],
            cut: newest.cut,
            head: newest.head,
        });
    }

    /// The person's "no" to a proposed run: it stays on disk marked discarded and can no longer
    /// be applied. Any other state is `Invalid`.
    pub(crate) fn discard_consolidation(
        &mut self,
        cx: &Cx<'_, B>,
        run: &RunId,
    ) -> Result<DraftView, Refusal> {
        let last = self.waiting(run, "discard")?;
        let (state, _) = step(RunState::Proposed, RunEvent::Discard);
        let view = DraftView {
            state,
            ..last.view.clone()
        };
        let next = LastRun {
            view: view.clone(),
            ..last
        };
        self.write_run(&next, cx.now())?;
        self.run = state;
        self.last = Some(next);
        self.outbox.push(ServiceEvent::ConsolidationChanged(
            self.space().clone(),
            run.clone(),
        ));
        Ok(view)
    }

    /// The last run, if it is `run` and still waiting for the person; else why not.
    pub(crate) fn waiting(&self, run: &RunId, verb: &str) -> Result<LastRun, Refusal> {
        match self.last.as_ref() {
            Some(l) if &l.run == run && l.view.state == RunState::Proposed => Ok(l.clone()),
            Some(l) if &l.run == run => Err(failed(format!(
                "cannot {verb} a run that is {}",
                state_word(l.view.state)
            ))),
            _ => Err(failed("no such run")),
        }
    }

    /// A forget took text out of the Space: the run files lose every hunk that carried it (the
    /// file keeps a count), and so does the last run in memory.
    pub(crate) fn scrub_runs(&mut self, gone: &Gone) -> Result<(), Refusal> {
        let vault = self.rt.store.vault();
        for mut rec in runfile::load_all(vault) {
            if rec.scrub(gone) {
                runfile::save(vault, &rec).map_err(failed)?;
            }
        }
        if let Some(last) = self.last.as_mut() {
            last.view.hunks.retain(|h| !runfile::must_go(h, gone));
            last.view
                .skipped
                .retain(|s| !runfile::must_go(&s.hunk, gone));
            let (guard, pre_images) = crate::revert_guard::after_forget(
                last.revert,
                std::mem::take(&mut last.pre_images),
                gone,
            );
            last.revert = guard;
            last.pre_images = pre_images;
        }
        Ok(())
    }
}

fn state_word(state: RunState) -> &'static str {
    match state {
        RunState::Applied => "applied",
        RunState::Reverted => "reverted",
        RunState::Discarded => "discarded",
        RunState::Superseded => "superseded",
        _ => "not proposed",
    }
}
