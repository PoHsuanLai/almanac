//! Consolidation as the service runs it: gather, draft, check, apply what is mechanical, keep
//! enough to revert (the facts a run added, what it superseded, and the files it rewrote as they
//! were). Every kind of hunk is applied: Promote and Supersede here, Tidy, Stamp, ExternalEdit and
//! Flag in `hunks.rs`.

use crate::backend::Backend;
use crate::consolidation::{
    ConsolidationInput, Consolidator, Draft, InputEvent, RunEffect, RunEvent, check_draft, step,
};
use crate::docs::{fact_doc, fact_doc_id};
use crate::facts::lands_for;
use crate::hunks::PreImage;
use crate::open::{Cx, LastRun, Open, failed, files_refusal};
use almanac_core::{
    DraftView, Fact, FactId, FactState, Hunk, Lands, MemoryOp, Refusal, RunId, RunState, Seq,
    TopicPath, hex_of,
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
}

impl<B: Backend> Open<B> {
    /// Runs consolidation now (the nightly tick's work, on demand).
    pub(crate) async fn run_consolidation(&mut self, cx: &Cx<'_, B>) -> Result<DraftView, Refusal> {
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
        let (state, effects) = step(state, RunEvent::Proceed);
        let mut applied = Applied::default();
        for effect in effects {
            match effect {
                RunEffect::ApplyHunks => {
                    for hunk in &checked.kept {
                        self.apply_hunk(cx, &run, hunk, &mut applied).await?;
                    }
                }
                RunEffect::LogConsolidated => {
                    self.audit(
                        now,
                        MemoryOp::Consolidated {
                            run: run.clone(),
                            hunks: almanac_core::Count(applied.hunks),
                        },
                    )?;
                }
                _ => {}
            }
        }
        let view = DraftView {
            run: run.clone(),
            hunks: checked.kept,
            state,
        };
        self.run = state;
        self.last = Some(LastRun {
            run,
            view: view.clone(),
            added: applied.added,
            superseded: applied.superseded,
            pre_images: applied.pre_images,
            topics: applied.topics,
            cut: head,
        });
        Ok(view)
    }

    async fn apply_hunk(
        &mut self,
        cx: &Cx<'_, B>,
        run: &RunId,
        hunk: &Hunk,
        applied: &mut Applied,
    ) -> Result<(), Refusal> {
        let (fact, to, old): (&Fact, Lands, Option<&FactId>) = match hunk {
            Hunk::Promote { fact, to } => (fact, *to, None),
            Hunk::Supersede { old, new } => (new, Lands::Active, Some(old)),
            Hunk::Tidy(tidy) => {
                if let Some(pre) = self.apply_tidy(tidy)? {
                    applied.pre_images.push(pre);
                    applied.topics.push(tidy.topic.clone());
                    applied.hunks += 1;
                    self.reindex_topic(cx, &tidy.topic).await?;
                }
                return Ok(());
            }
            Hunk::Stamp { topic, text } => {
                if let Some((pre, fact)) = self.apply_stamp(cx, topic, text)? {
                    applied.pre_images.push(pre);
                    applied.added.push(fact.id.clone());
                    applied.hunks += 1;
                    self.index_put(cx, vec![fact_doc(&fact)]).await;
                }
                return Ok(());
            }
            Hunk::ExternalEdit { topic, before, .. } => {
                self.apply_external_edit(cx, topic, before).await?;
                self.absorb_edit(topic);
                applied.hunks += 1;
                return Ok(());
            }
            Hunk::Flag { facts, note } => {
                self.apply_flag(run, facts, note)?;
                applied.hunks += 1;
                return Ok(());
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
        Ok(())
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
        Ok(())
    }
}
