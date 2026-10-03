//! Consolidation as the service runs it: gather, draft, check, apply what is mechanical, keep
//! enough to revert. Tidy, Stamp, ExternalEdit and Flag hunks stay in the reviewable diff and
//! are not applied by the service yet (see FINDINGS).

use crate::backend::Backend;
use crate::consolidation::{
    ConsolidationInput, Consolidator, Draft, InputEvent, RunEffect, RunEvent, check_draft, step,
};
use crate::docs::{fact_doc, fact_doc_id};
use crate::facts::lands_for;
use crate::open::{Cx, LastRun, Open, failed, files_refusal};
use almanac_core::{
    DraftView, Fact, FactId, FactState, Hunk, Lands, MemoryOp, Refusal, RunId, RunState, Seq,
    TopicPath, hex_of,
};
use eventlog::BodyState;

/// Where promoted facts land: the model names no topic.
const TOPIC: &str = "consolidated";

impl<B: Backend> Open<B> {
    /// Runs consolidation now (the nightly tick's work, on demand).
    pub(crate) async fn run_consolidation(&mut self, cx: &Cx<'_, B>) -> Result<DraftView, Refusal> {
        let now = cx.now();
        let run =
            RunId::parse(&format!("c-{}", hex_of(&self.entropy(now, b"run")))).map_err(failed)?;
        let cut = self.last.as_ref().map_or(Seq(0), |l| l.cut);
        let mut state = RunState::Due;
        let (s, _) = step(state, RunEvent::Start);
        state = s;
        let entries = self.entries()?;
        let head = entries.last().map_or(cut, |e| e.header.seq);
        let input = ConsolidationInput {
            space: self.space().clone(),
            run: run.clone(),
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
        };
        let (s, _) = step(state, RunEvent::InputReady);
        state = s;
        let draft: Draft = match cx.backend.consolidator().draft(input.clone()).await {
            Ok(d) => d,
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
        let mut added = Vec::new();
        let mut superseded = Vec::new();
        for effect in effects {
            match effect {
                RunEffect::ApplyHunks => {
                    for hunk in &checked.kept {
                        self.apply_hunk(cx, hunk, &mut added, &mut superseded)
                            .await?;
                    }
                }
                RunEffect::LogConsolidated => {
                    self.audit(
                        now,
                        MemoryOp::Consolidated {
                            run: run.clone(),
                            hunks: almanac_core::Count(
                                u32::try_from(added.len() + superseded.len()).unwrap_or(u32::MAX),
                            ),
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
            added,
            superseded,
            cut: head,
        });
        Ok(view)
    }

    async fn apply_hunk(
        &mut self,
        cx: &Cx<'_, B>,
        hunk: &Hunk,
        added: &mut Vec<FactId>,
        superseded: &mut Vec<FactId>,
    ) -> Result<(), Refusal> {
        let (fact, to, old): (&Fact, Lands, Option<&FactId>) = match hunk {
            Hunk::Promote { fact, to } => (fact, *to, None),
            Hunk::Supersede { old, new } => (new, Lands::Active, Some(old)),
            Hunk::Tidy(_) | Hunk::Flag { .. } | Hunk::ExternalEdit { .. } | Hunk::Stamp { .. } => {
                return Ok(());
            }
        };
        let topic = TopicPath::parse(TOPIC).map_err(failed)?;
        match (to, lands_for(&fact.label)) {
            (Lands::Active, Lands::Active) => {
                self.rt
                    .store
                    .append(&topic, fact.clone())
                    .map_err(files_refusal)?;
                if let Some(old) = old {
                    self.index_drop(&[fact_doc_id(old)]);
                    superseded.push(old.clone());
                }
                self.index_put(cx, vec![fact_doc(fact)]).await;
            }
            _ => self
                .rt
                .store
                .stage(fact.clone(), topic)
                .map_err(files_refusal)?,
        }
        added.push(fact.id.clone());
        Ok(())
    }

    /// Puts back what the run changed: its facts go, what it superseded is indexed again.
    pub(crate) async fn revert(&mut self, cx: &Cx<'_, B>, run: &RunId) -> Result<(), Refusal> {
        let Some(last) = self.last.clone().filter(|l| &l.run == run) else {
            return Err(failed("no such run"));
        };
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
            .filter(|s| last.superseded.contains(&s.fact.id) && s.state == FactState::Active)
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
