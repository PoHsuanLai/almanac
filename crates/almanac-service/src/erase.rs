//! Forgetting, as the service runs it: make a plan, then apply exactly that plan.

use crate::backend::Backend;
use crate::forget::{
    ApplyStep, Plan, PlanEffect, PlanEvent, PlanState, plan_forget, plan_step, token_for,
};
use crate::open::{Cx, Open, failed, files_refusal, log_refusal};
use almanac_core::{
    FactId, ForgetPlanView, ForgetReport, ForgetScope, MemoryOp, PlanToken, Refusal,
};
use eventlog::LogWrite;
use memfiles::Vault;

impl<B: Backend> Open<B> {
    /// Computes what forgetting `scope` removes and keeps the plan for `Forget(token)`.
    pub(crate) fn plan(
        &mut self,
        cx: &Cx<'_, B>,
        scope: ForgetScope,
    ) -> Result<ForgetPlanView, Refusal> {
        let now = cx.now();
        self.entries()?;
        let graph = self.fact_graph()?;
        let plan = plan_forget(self.space(), &scope, &self.rt.log, &graph);
        let digest = plan.digest();
        let token = token_for(&digest).ok_or_else(|| failed("no token for the plan"))?;
        let state = PlanState::planned(digest, now);
        let expires = match state {
            PlanState::Planned { expires, .. } => expires,
            _ => now,
        };
        self.rt.plans.retain(|_, (_, s)| {
            !matches!(plan_step(*s, PlanEvent::Tick { now }).0, PlanState::Expired)
        });
        let stored = self.stored()?;
        let doomed: Vec<&crate::open::Stored> = stored
            .iter()
            .filter(|s| plan.facts.contains(&s.fact.id))
            .collect();
        let facts = self.fact_views(cx, &doomed)?;
        let counts = plan.counts();
        self.rt.plans.insert(token.clone(), (plan, state));
        Ok(ForgetPlanView {
            token,
            expires,
            events: counts.events,
            facts,
            procedures: counts.procedures,
            index_docs: counts.index_docs,
            pending: counts.pending,
        })
    }

    /// Applies the plan `token` names, after checking that what it would remove is still
    /// exactly what was previewed: the stored plan against its digest, and the closure now
    /// against the same digest (`plan_step`). Then the steps run in `ApplyStep::ORDER`.
    pub(crate) fn forget(
        &mut self,
        cx: &Cx<'_, B>,
        token: &PlanToken,
    ) -> Result<(ForgetReport, ForgetScope), Refusal> {
        let now = cx.now();
        let (plan, state) = self
            .rt
            .plans
            .get(token)
            .cloned()
            .ok_or_else(|| failed("no such plan"))?;
        let PlanState::Planned { digest, .. } = state else {
            return Err(Refusal::PlanStale);
        };
        if plan.digest() != digest {
            self.rt
                .plans
                .insert(token.clone(), (plan, PlanState::Stale));
            return Err(Refusal::PlanStale);
        }
        self.entries()?;
        let graph = self.fact_graph()?;
        let current = plan_forget(self.space(), &plan.scope, &self.rt.log, &graph).digest();
        let (next, effects) = plan_step(state, PlanEvent::Forget { current, now });
        self.rt.plans.insert(token.clone(), (plan.clone(), next));
        if let Some(PlanEffect::Refuse(why)) =
            effects.iter().find(|e| matches!(e, PlanEffect::Refuse(_)))
        {
            return Err(why.clone());
        }
        let counts = plan.counts();
        for apply in ApplyStep::ORDER {
            self.apply_step(now, apply, &plan, &digest, counts)?;
        }
        self.rt.plans.remove(token);
        Ok((
            ForgetReport {
                plan: digest,
                counts,
            },
            plan.scope,
        ))
    }

    fn apply_step(
        &mut self,
        now: almanac_core::UnixSeconds,
        step: ApplyStep,
        plan: &Plan,
        digest: &almanac_core::PlanDigest,
        counts: almanac_core::ForgetCounts,
    ) -> Result<(), Refusal> {
        match step {
            ApplyStep::Index => self.index_drop(&plan.index_docs),
            ApplyStep::Memfiles => {
                let ids: Vec<FactId> = plan.facts.iter().chain(&plan.pending).cloned().collect();
                self.rt.store.remove(&ids).map_err(files_refusal)?;
                self.prune_flags(&ids)?;
                self.scrub_runs(&crate::runfile::Gone {
                    facts: ids.clone(),
                    events: plan.events.clone(),
                })?;
                self.forget_in_baseline(&ids)?;
                for path in &plan.procedures {
                    self.rt.store.vault().remove(path).map_err(failed)?;
                }
            }
            ApplyStep::EventBodies => {
                self.rt
                    .log
                    .erase_bodies(&plan.events)
                    .map_err(log_refusal)?;
                self.notes.forgotten.extend(plan.events.iter().copied());
            }
            ApplyStep::AuditEntry => {
                self.audit(
                    now,
                    MemoryOp::Forgot {
                        plan: *digest,
                        counts,
                    },
                )?;
            }
            // The log checkpoints its write-ahead log inside `erase_bodies`.
            ApplyStep::WalTruncate => {}
        }
        Ok(())
    }
}
