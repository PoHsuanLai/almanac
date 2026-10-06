//! Consolidation as the service runs it: gather, draft, check, apply what is mechanical, keep
//! enough to revert (the facts a run added, what it superseded, and the files it rewrote as they
//! were). Every kind of hunk is applied: Promote and Supersede here, Tidy, Stamp, ExternalEdit and
//! Flag in `hunks.rs`.

use crate::backend::Backend;
use crate::consolidation::{
    ConsolidationInput, Consolidator, Draft, InputEvent, RunEffect, RunEvent, check_draft, step,
};
use crate::docs::{fact_doc, fact_doc_id};
use crate::events::ServiceEvent;
use crate::facts::lands_for;
use crate::grounds::Grounds;
use crate::hunks::PreImage;
use crate::open::{Cx, LastRun, Open, failed, files_refusal};
use crate::revert_guard::{self, RevertGuard};
use crate::settings::{ConsolidateApply, ConsolidateWhen};
use almanac_core::{
    DraftView, Fact, FactId, FactState, Hunk, Lands, MemoryOp, Refusal, RunId, RunState, Seq,
    SkipReason, SkippedHunk, TopicPath, hex_of,
};
use eventlog::BodyState;

/// Where promoted facts land: the model names no topic.
const TOPIC: &str = "consolidated";

/// What a run changed so far.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    added: Vec<FactId>,
    superseded: Vec<FactId>,
    pre_images: Vec<PreImage>,
    topics: Vec<TopicPath>,
    hunks: u32,
    /// The hunks that were applied, and those left out with the reason.
    done: Vec<Hunk>,
    skipped: Vec<SkippedHunk>,
}

impl<B: Backend> Open<B> {
    /// Runs consolidation now (the nightly tick's work, on demand).
    pub(crate) async fn run_consolidation(&mut self, cx: &Cx<'_, B>) -> Result<DraftView, Refusal> {
        if cx.settings.consolidate == ConsolidateWhen::Never {
            return Err(failed(
                "consolidation is turned off (memory.consolidation.when)",
            ));
        }
        let now = cx.now();
        let run =
            RunId::parse(&format!("c-{}", hex_of(&self.entropy(cx, b"run")))).map_err(failed)?;
        let cut = self.last.as_ref().map_or(Seq(0), |l| l.cut);
        let mut state = RunState::Due;
        let (s, _) = step(state, RunEvent::Start);
        state = s;
        let entries = self.entries()?;
        let head = entries.last().map_or(cut, |e| e.header.seq);
        let input = ConsolidationInput {
            space: self.space().clone(),
            run: run.clone(),
            now,
            facts: self
                .stored()?
                .into_iter()
                .filter(|s| s.state == FactState::Active)
                .map(|s| s.fact)
                .collect(),
            events: entries
                .iter()
                .filter(|e| e.header.seq > cut)
                .filter_map(|e| match &e.body {
                    BodyState::Present(b)
                        if !matches!(b, almanac_core::EventBody::Memory { .. }) =>
                    {
                        Some(InputEvent {
                            event: self.event_ref(e),
                            kind: e.header.kind.clone(),
                            things: b.things().into_iter().map(|(v, _)| v.clone()).collect(),
                            label: e.header.label.clone(),
                        })
                    }
                    _ => None,
                })
                .collect(),
            topics: self.input_topics()?,
        };
        let (s, _) = step(state, RunEvent::InputReady);
        state = s;
        let draft: Draft = match cx.backend.consolidator().draft(input.clone()).await {
            Ok(mut d) => {
                d.hunks.extend(self.stamp_hunks()?);
                d.hunks.extend(self.external_edit_hunks()?);
                d
            }
            Err(e) => {
                self.run = step(state, RunEvent::DraftFailed(e.into())).0;
                return Err(match e {
                    crate::consolidation::ConsolidateError::Unparseable => {
                        failed("the consolidation draft is unreadable")
                    }
                    _ => Refusal::Busy,
                });
            }
        };
        state = step(state, RunEvent::DraftOk).0;
        let checked = check_draft(&input, draft);
        state = step(state, RunEvent::ChecksDone).0;
        let (state, applied, cut) = match cx.settings.apply {
            ConsolidateApply::Auto => {
                let (state, applied) = self.proceed(cx, &run, &checked.kept).await?;
                (state, applied, head)
            }
            // The proposal waits: nothing is applied, and the cut stays where it was so the
            // next run (or the apply) still sees the events this one read.
            ConsolidateApply::Review => (state, Applied::default(), cut),
        };
        let (hunks, skipped) = match cx.settings.apply {
            ConsolidateApply::Auto => (applied.done.clone(), applied.skipped.clone()),
            ConsolidateApply::Review => (checked.kept, Vec::new()),
        };
        let view = DraftView {
            run: run.clone(),
            hunks,
            state,
            skipped,
        };
        let last = LastRun {
            run: run.clone(),
            view: view.clone(),
            added: applied.added,
            superseded: applied.superseded,
            pre_images: applied.pre_images,
            revert: RevertGuard::default(),
            topics: applied.topics,
            cut,
            head,
        };
        // A proposal is a file before it is anything else: if it cannot be written the run
        // fails (nothing was applied). An applied run is already in the Space and the log; its
        // record is best effort.
        match state {
            RunState::Proposed => self.write_run(&last, now)?,
            _ => drop(self.write_run(&last, now)),
        }
        self.supersede_others(&run, now);
        self.run = state;
        self.last = Some(last);
        Ok(view)
    }

    /// The person's go-ahead for a proposed run (`memory.consolidation.apply = review`): runs
    /// the machine's `Proceed` step over the hunks the proposal kept, each checked again against
    /// what the Space holds now. The run must be the last one and still `Proposed`.
    pub(crate) async fn apply_consolidation(
        &mut self,
        cx: &Cx<'_, B>,
        run: &RunId,
    ) -> Result<DraftView, Refusal> {
        let last = self.waiting(run, "apply")?;
        let (state, applied) = self.proceed(cx, run, &last.view.hunks).await?;
        let view = DraftView {
            state,
            hunks: applied.done.clone(),
            skipped: applied.skipped.clone(),
            ..last.view
        };
        let next = LastRun {
            run: run.clone(),
            view: view.clone(),
            added: applied.added,
            superseded: applied.superseded,
            pre_images: applied.pre_images,
            revert: RevertGuard::default(),
            topics: applied.topics,
            cut: last.head,
            head: last.head,
        };
        // The Space has changed and the log says so; a record that cannot be written is
        // reported by the next start (the file still says `proposed`, and apply re-checks).
        drop(self.write_run(&next, cx.now()));
        self.run = state;
        self.last = Some(next);
        self.outbox.push(ServiceEvent::ConsolidationChanged(
            self.space().clone(),
            run.clone(),
        ));
        Ok(view)
    }

    /// The machine's `Proceed` step: applies `hunks`, then logs the run.
    async fn proceed(
        &mut self,
        cx: &Cx<'_, B>,
        run: &RunId,
        hunks: &[Hunk],
    ) -> Result<(RunState, Applied), Refusal> {
        let (state, effects) = step(RunState::Proposed, RunEvent::Proceed);
        let mut applied = Applied::default();
        for effect in effects {
            match effect {
                RunEffect::ApplyHunks => {
                    let grounds = self.grounds()?;
                    for hunk in hunks {
                        let left_out = match grounds.why_not(hunk) {
                            Some(reason) => Some(reason),
                            None => self.apply_hunk(cx, run, hunk, &mut applied).await?,
                        };
                        match left_out {
                            Some(reason) => applied.skipped.push(SkippedHunk {
                                hunk: hunk.clone(),
                                reason,
                            }),
                            None => applied.done.push(hunk.clone()),
                        }
                    }
                }
                RunEffect::LogConsolidated => {
                    self.audit(
                        cx.now(),
                        MemoryOp::Consolidated {
                            run: run.clone(),
                            hunks: almanac_core::Count(applied.hunks),
                        },
                    )?;
                }
                _ => {}
            }
        }
        Ok((state, applied))
    }

    async fn apply_hunk(
        &mut self,
        cx: &Cx<'_, B>,
        run: &RunId,
        hunk: &Hunk,
        applied: &mut Applied,
    ) -> Result<Option<SkipReason>, Refusal> {
        let (fact, to, old): (&Fact, Lands, Option<&FactId>) = match hunk {
            Hunk::Promote { fact, to } => (fact, *to, None),
            Hunk::Supersede { old, new } => (new, Lands::Active, Some(old)),
            Hunk::Tidy(tidy) => {
                if let Some(pre) = self.apply_tidy(tidy)? {
                    applied.pre_images.push(pre);
                    applied.topics.push(tidy.topic.clone());
                    applied.hunks += 1;
                    self.reindex_topic(cx, &tidy.topic).await?;
                    return Ok(None);
                }
                return Ok(Some(SkipReason::FileChanged));
            }
            Hunk::Stamp { topic, text } => {
                if let Some((pre, fact)) = self.apply_stamp(cx, topic, text)? {
                    applied.pre_images.push(pre);
                    applied.added.push(fact.id.clone());
                    applied.hunks += 1;
                    self.index_put(cx, vec![fact_doc(&fact)]).await;
                    return Ok(None);
                }
                return Ok(Some(SkipReason::FileChanged));
            }
            Hunk::ExternalEdit { topic, before, .. } => {
                self.apply_external_edit(cx, topic, before).await?;
                self.absorb_edit(topic);
                applied.hunks += 1;
                return Ok(None);
            }
            Hunk::Flag { facts, note } => {
                self.apply_flag(run, facts, note)?;
                applied.hunks += 1;
                return Ok(None);
            }
        };
        let topic = TopicPath::parse(TOPIC).map_err(failed)?;
        match (to, lands_for(&fact.label)) {
            (Lands::Active, Lands::Active) => {
                applied.pre_images.push(self.topic_pre_image(&topic));
                self.rt
                    .store
                    .append(&topic, fact.clone())
                    .map_err(files_refusal)?;
                if let Some(old) = old {
                    self.index_drop(&[fact_doc_id(old)]);
                    applied.superseded.push(old.clone());
                }
                self.index_put(cx, vec![fact_doc(fact)]).await;
            }
            _ => self
                .rt
                .store
                .stage(fact.clone(), topic)
                .map_err(files_refusal)?,
        }
        applied.added.push(fact.id.clone());
        applied.hunks += 1;
        Ok(None)
    }

    /// What the Space holds now, for the check that a proposed hunk still stands.
    fn grounds(&self) -> Result<Grounds, Refusal> {
        let stored = self.stored()?;
        Ok(Grounds {
            events: self
                .entries()?
                .iter()
                .filter(|e| matches!(e.body, BodyState::Present(_)))
                .map(|e| self.event_ref(e))
                .collect(),
            active: stored
                .iter()
                .filter(|s| s.state == FactState::Active)
                .map(|s| s.fact.id.clone())
                .collect(),
            known: stored.iter().map(|s| s.fact.id.clone()).collect(),
        })
    }

    /// Indexes the active facts of one topic again.
    pub(crate) async fn reindex_topic(
        &mut self,
        cx: &Cx<'_, B>,
        topic: &TopicPath,
    ) -> Result<(), Refusal> {
        let docs = self
            .stored()?
            .into_iter()
            .filter(|s| &s.topic == topic && s.state == FactState::Active)
            .map(|s| fact_doc(&s.fact))
            .collect();
        self.index_put(cx, docs).await;
        Ok(())
    }

    /// Puts back what the run changed: the files it rewrote are restored from their pre-images,
    /// its facts go, what it superseded is indexed again.
    pub(crate) async fn revert(&mut self, cx: &Cx<'_, B>, run: &RunId) -> Result<(), Refusal> {
        let Some(last) = self.last.clone().filter(|l| &l.run == run) else {
            return Err(failed("no such run"));
        };
        if last.view.state != RunState::Applied {
            return Err(failed("the run was not applied"));
        }
        if last.revert == RevertGuard::Forgotten {
            return Err(failed(revert_guard::REFUSED));
        }
        self.restore(&last.pre_images)?;
        self.rt
            .store
            .remove(
                &last.added,
                &crate::forget::Plan {
                    space: self.space().clone(),
                    scope: almanac_core::ForgetScope::Space,
                    events: vec![],
                    facts: last.added.clone(),
                    pending: vec![],
                    procedures: vec![],
                    index_docs: vec![],
                }
                .digest(),
            )
            .map_err(files_refusal)?;
        self.index_drop(&last.added.iter().map(fact_doc_id).collect::<Vec<_>>());
        let stored = self.stored()?;
        let again: Vec<_> = stored
            .iter()
            .filter(|s| {
                (last.superseded.contains(&s.fact.id) || last.topics.contains(&s.topic))
                    && s.state == FactState::Active
            })
            .map(|s| fact_doc(&s.fact))
            .collect();
        self.index_put(cx, again).await;
        self.run = step(RunState::Applied, RunEvent::Revert).0;
        self.audit(cx.now(), MemoryOp::Reverted { run: run.clone() })?;
        if let Some(l) = self.last.as_mut() {
            l.view.state = RunState::Reverted;
        }
        if let Some(l) = self.last.clone() {
            self.write_run(&l, cx.now())?;
        }
        Ok(())
    }
}
