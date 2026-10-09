//! Routing: one request to the Space it names (or the Spaces it finds), and the export.

use crate::backend::Backend;
use crate::clock::Clock;
use crate::export::{EventLine, ExportWriter};
use crate::open::{Cx, Open, failed, log_refusal};
use crate::record::Stored;
use crate::service::{Lease, MemoryService};
use almanac_core::{
    Caller, Count, EXPORT_FORMAT, ExportCounts, ExportManifest, ExportOptions, ExportedSpace,
    FactState, ForgetScope, Head, MemoryOp, MemoryReply, MemoryRequest, Refusal, RunId, SpaceId,
    VerificationKey, hex_of,
};
use eventlog::LogRead;
use memfiles::{Vault, VaultPath};
use std::io::Write;

fn count(n: usize) -> Count {
    Count(u32::try_from(n).unwrap_or(u32::MAX))
}

/// A search over the Spaces that found nothing: `Busy` when a Space was out on another request
/// (it may hold the answer, so the caller should retry), else the not-found refusal.
fn not_found_or_busy(busy: bool, not_found: Refusal) -> Refusal {
    match busy {
        true => Refusal::Busy,
        false => not_found,
    }
}

/// One Space's share of an export, collected while the Space is out.
struct Gathered {
    space: SpaceId,
    head: Head,
    lines: Vec<EventLine>,
    files: Vec<(VaultPath, Vec<u8>)>,
    counts: ExportCounts,
    key: Option<String>,
}

impl<B: Backend> MemoryService<B> {
    pub(crate) async fn dispatch(
        &self,
        caller: &Caller,
        request: MemoryRequest,
    ) -> Result<MemoryReply, Refusal> {
        use MemoryRequest as R;
        match request {
            R::Record(record) => Ok(match self.record_one(caller, record).await? {
                Stored::Event(event) => MemoryReply::Recorded(event),
                Stored::Dropped(_) => MemoryReply::Ok,
            }),
            R::RecordDurable(record) => self.record_durable(caller, record).await,
            R::RecordBatch(records) => {
                let mut first = None;
                let mut stored = 0usize;
                for record in records {
                    if let Stored::Event(event) = self.record_one(caller, record).await? {
                        first.get_or_insert(event);
                        stored += 1;
                    }
                }
                Ok(first.map_or(MemoryReply::Ok, |e| {
                    MemoryReply::RecordedBatch(e, count(stored))
                }))
            }
            R::Spaces => Ok(MemoryReply::Spaces(self.summaries())),
            R::RemoveSpace(space, removal) => self
                .remove_space(caller, &space, removal)
                .await
                .map(MemoryReply::Relocated),
            R::Rules => Ok(MemoryReply::Rules(self.rules())),
            R::SetRule(rule) => {
                let id = rule.id.clone();
                self.set_rules(|rules| {
                    rules.rules.retain(|r| r.id != id);
                    rules.rules.push(rule);
                });
                self.audit_open(
                    self.backend().clock().now(),
                    &MemoryOp::RuleChanged { rule: id },
                );
                Ok(MemoryReply::Ok)
            }
            R::RemoveRule(id) => {
                self.set_rules(|rules| rules.rules.retain(|r| r.id != id));
                self.audit_open(
                    self.backend().clock().now(),
                    &MemoryOp::RuleChanged { rule: id },
                );
                Ok(MemoryReply::Ok)
            }
            R::Export(_) => Err(failed(
                "an export needs a stream: use MemoryService::export",
            )),
            R::Forget(token) => {
                let id = self
                    .space_of_plan(&token)
                    .ok_or_else(|| failed("no such plan"))?;
                self.forget_in(caller, &id, &token).await
            }
            R::Settle(fact, settlement) => {
                let mut busy = false;
                for id in self.all_spaces() {
                    let mut lease = match self.checkout(caller, &id).await {
                        Ok(lease) => lease,
                        Err(Refusal::Busy) => {
                            busy = true;
                            continue;
                        }
                        Err(_) => continue,
                    };
                    let cx = self.cx(caller);
                    let Some(open) = lease.open() else { continue };
                    let known = open.stored()?.iter().any(|s| s.fact.id == fact);
                    if known {
                        open.guard_topics()?;
                        open.settle(&cx, &fact, settlement).await?;
                        open.accept_topics()?;
                        self.raise_all(open.outbox.drain(..));
                        return Ok(MemoryReply::Ok);
                    }
                }
                Err(not_found_or_busy(busy, Refusal::NoSuchFact))
            }
            R::Revert(run) => {
                let id = self
                    .space_of_run(&run)
                    .ok_or_else(|| failed("no such run"))?;
                let mut lease = self.checkout(caller, &id).await?;
                let cx = self.cx(caller);
                let open = lease.open().ok_or(Refusal::Busy)?;
                open.guard_topics()?;
                open.revert(&cx, &run).await?;
                open.accept_topics()?;
                Ok(MemoryReply::Ok)
            }
            R::ApplyConsolidation(run) => {
                let id = self.find_run(caller, &run).await?;
                let mut lease = self.checkout(caller, &id).await?;
                let cx = self.cx(caller);
                let open = lease.open().ok_or(Refusal::Busy)?;
                open.guard_topics()?;
                let view = open.apply_consolidation(&cx, &run).await?;
                open.accept_topics()?;
                self.raise_all(open.outbox.drain(..));
                Ok(MemoryReply::Consolidation(view))
            }
            R::DiscardConsolidation(run) => {
                let id = self.find_run(caller, &run).await?;
                let mut lease = self.checkout(caller, &id).await?;
                let cx = self.cx(caller);
                let open = lease.open().ok_or(Refusal::Busy)?;
                let view = open.discard_consolidation(&cx, &run)?;
                self.raise_all(open.outbox.drain(..));
                Ok(MemoryReply::Consolidation(view))
            }
            other => self.in_space(caller, other).await,
        }
    }

    /// The Space holding the proposal `run`: an open Space's last run, else the Spaces are opened
    /// one by one (after a restart the proposals are in their files until a Space opens).
    async fn find_run(&self, caller: &Caller, run: &RunId) -> Result<SpaceId, Refusal> {
        if let Some(id) = self.space_of_run(run) {
            return Ok(id);
        }
        let mut busy = false;
        for id in self.all_spaces() {
            match self.checkout(caller, &id).await {
                // The lease goes back before the Space is asked about the run.
                Ok(lease) => drop(lease),
                Err(Refusal::Busy) => busy = true,
                Err(_) => continue,
            }
            if self.space_of_run(run).as_ref() == Some(&id) {
                return Ok(id);
            }
        }
        Err(not_found_or_busy(busy, failed("no such run")))
    }

    async fn forget_in(
        &self,
        caller: &Caller,
        id: &SpaceId,
        token: &almanac_core::PlanToken,
    ) -> Result<MemoryReply, Refusal> {
        let mut lease = self.checkout(caller, id).await?;
        let cx = self.cx(caller);
        let open = lease.open().ok_or(Refusal::Busy)?;
        open.guard_topics()?;
        let (report, scope) = open.forget(&cx, token)?;
        match scope {
            ForgetScope::Space => self.delete_space(caller, id, lease).await?,
            _ => open.accept_topics()?,
        }
        Ok(MemoryReply::Forgot(report))
    }

    /// The Space's deletion finishes: its head is anchored, its key destroyed, its stores and
    /// directories removed.
    pub(crate) async fn delete_space(
        &self,
        caller: &Caller,
        id: &SpaceId,
        mut lease: Lease<'_, B>,
    ) -> Result<(), Refusal> {
        let open = lease.open().ok_or(Refusal::Busy)?;
        {
            use crate::space::{SpaceEffect, SpaceEvent, step};
            let final_head = open.rt.log.head().map_err(log_refusal)?;
            let (deleting, _) = step(open.rt.state, SpaceEvent::ForgetConfirmed);
            let (gone, effects) = step(deleting, SpaceEvent::Done);
            open.rt.state = gone;
            for effect in effects {
                match effect {
                    SpaceEffect::AnchorFinalHead => {
                        self.anchor_final_head(caller, id, final_head).await;
                    }
                    SpaceEffect::DestroyKey => {
                        let _ = almanac_seal::KeyStore::destroy(self.backend().keys(), id).await;
                    }
                    _ => {}
                }
            }
            self.forget_meta(id);
            // The stores close before their directories go.
            lease.discard();
            self.backend().remove_space(id).map_err(failed)
        }
    }

    /// Records a deleted Space's final head in the `desktop` Space's log (`SpaceEffect::
    /// AnchorFinalHead`). Best effort: the person's deletion goes ahead if the desktop log is
    /// not available (locked or itself being deleted).
    async fn anchor_final_head(&self, caller: &Caller, space: &SpaceId, head: Head) {
        let desktop = SpaceId::desktop();
        if space == &desktop {
            return;
        }
        let Ok(mut lease) = self.checkout(caller, &desktop).await else {
            return;
        };
        let now = self.backend().clock().now();
        if let Some(open) = lease.open() {
            let _ = open.audit(
                now,
                MemoryOp::SpaceDeleted {
                    space: space.clone(),
                    head,
                },
            );
        }
    }

    /// A request that names one Space.
    async fn in_space(
        &self,
        caller: &Caller,
        request: MemoryRequest,
    ) -> Result<MemoryReply, Refusal> {
        let id = request
            .space()
            .cloned()
            .ok_or_else(|| failed("no Space named"))?;
        if matches!(&request, MemoryRequest::PlanForget(_, ForgetScope::Space))
            && id == SpaceId::desktop()
        {
            // The `desktop` Space holds memory outside every Space, and the final head of each
            // Space that is deleted: it is always there.
            return Err(failed("the desktop Space cannot be deleted"));
        }
        let mut lease = self.checkout(caller, &id).await?;
        let cx = self.cx(caller);
        let open: &mut Open<B> = lease.open().ok_or(Refusal::Busy)?;
        let reply = run_in_space(open, &cx, request).await;
        self.raise_all(open.outbox.drain(..));
        reply
    }

    pub(crate) async fn export_to(
        &self,
        caller: &Caller,
        options: &ExportOptions,
        out: &mut (impl Write + Send),
    ) -> Result<ExportManifest, Refusal> {
        let ids = if options.spaces.is_empty() {
            self.all_spaces()
        } else {
            options.spaces.clone()
        };
        let mut gathered = Vec::new();
        for id in ids {
            let mut lease = self.checkout(caller, &id).await?;
            let cx = self.cx(caller);
            let open = lease.open().ok_or(Refusal::Busy)?;
            let g = gather(open, options.verification_key)?;
            open.audit(cx.now(), MemoryOp::Exported { counts: g.counts })?;
            gathered.push(g);
        }
        let now = self.backend().clock().now();
        let total = |f: fn(&ExportCounts) -> Count| {
            count(gathered.iter().map(|g| f(&g.counts).0 as usize).sum())
        };
        let manifest = ExportManifest {
            format: EXPORT_FORMAT.to_owned(),
            spaces: gathered
                .iter()
                .map(|g| ExportedSpace {
                    space: g.space.clone(),
                    head: Some(g.head),
                    counts: g.counts,
                })
                .collect(),
            created: now,
            counts: ExportCounts {
                events: total(|c| c.events),
                facts: total(|c| c.facts),
                pending: total(|c| c.pending),
                procedures: total(|c| c.procedures),
            },
        };
        let io = |e: std::io::Error| failed(e);
        let mut writer = ExportWriter::new(&mut *out, now);
        writer.manifest(&manifest).map_err(io)?;
        for g in &gathered {
            writer.events(&g.space, &g.lines).map_err(io)?;
            for (path, bytes) in &g.files {
                writer.file(&g.space, path, bytes).map_err(io)?;
            }
            if let Some(key) = &g.key {
                writer.digest_key(&g.space, key).map_err(io)?;
            }
        }
        let toml = crate::config::rules_to_toml(&self.rules()).map_err(failed)?;
        writer.rules(&toml).map_err(io)?;
        writer.finish().map_err(io)?;
        Ok(manifest)
    }
}

/// The request `request` against the open Space `open`.
async fn run_in_space<B: Backend>(
    open: &mut Open<B>,
    cx: &Cx<'_, B>,
    request: MemoryRequest,
) -> Result<MemoryReply, Refusal> {
    use MemoryRequest as R;
    open.tick(cx)?;
    match request {
        R::ExplainFile(claim) => open.explain(cx, claim).await.map(|_| MemoryReply::Ok),
        R::Mark(mark) => open.mark(mark).map(|()| MemoryReply::Ok),
        R::Search(q) => open.search(cx, q).await.map(MemoryReply::Hits),
        R::Inject(q) => open.inject(cx, q).await.map(MemoryReply::Hits),
        R::Recent(_, q) => open.recent(cx, q).map(MemoryReply::Recent),
        R::Entries(_, q) => open.entries_page(cx, q).map(MemoryReply::Entries),
        R::Facts(q) => open.facts(cx, &q).map(MemoryReply::Facts),
        R::Related(_, thing) => open.related(cx, &thing).map(MemoryReply::Related),
        R::Provenance(_, path) => open.provenance(cx, &path).map(MemoryReply::Provenance),
        R::Primer(_) => open.primer(cx).map(MemoryReply::Primer),
        R::Propose(_, draft) => {
            open.guard_topics()?;
            let proposed = open.propose(cx, draft).await?;
            open.accept_topics()?;
            Ok(MemoryReply::Proposed(proposed.0, proposed.1))
        }
        R::Status(_) => open.status().map(MemoryReply::Status),
        R::Timeline(_, q) => open.timeline(cx, &q).map(MemoryReply::Timeline),
        R::PlanForget(_, scope) => open.plan(cx, scope).map(MemoryReply::Plan),
        R::Pending(_) => open.pending(cx).map(MemoryReply::Pending),
        R::Consolidation(_) => open
            .last
            .as_ref()
            .map(|l| MemoryReply::Consolidation(l.view.clone()))
            .ok_or_else(|| failed("no consolidation run yet")),
        R::RunConsolidation(_) => {
            open.guard_topics()?;
            let view = open.run_consolidation(cx).await?;
            open.accept_topics()?;
            Ok(MemoryReply::Consolidation(view))
        }
        R::Pause(_, until) => open.pause(cx, until).map(|()| MemoryReply::Ok),
        R::Resume(_) => open.resume(cx).map(|()| MemoryReply::Ok),
        R::Verify(_) => open.verify().map(MemoryReply::Verified),
        R::Rebuild(_) => open.rebuild_index(cx).await.map(|()| MemoryReply::Ok),
        R::Sweep(_) => open.sweep(cx).map(MemoryReply::Swept),
        _ => Err(failed("not a Space request")),
    }
}

fn gather<B: Backend>(open: &Open<B>, key: VerificationKey) -> Result<Gathered, Refusal> {
    let head = open.rt.log.head().map_err(log_refusal)?;
    let lines: Vec<EventLine> = open.entries()?.iter().map(EventLine::of).collect();
    let vault = open.rt.store.vault();
    let mut files = Vec::new();
    let mut procedures = 0usize;
    for dir in [
        VaultPath::facts_dir(),
        VaultPath::pending_dir(),
        VaultPath::procedures_dir(),
    ] {
        let listed = vault.list(&dir).map_err(failed)?;
        if dir == VaultPath::procedures_dir() {
            procedures += listed.len();
        }
        for path in listed {
            let bytes = vault.read(&path).map_err(failed)?;
            files.push((path, bytes));
        }
    }
    let stored = open.stored()?;
    let n = |state: FactState| count(stored.iter().filter(|s| s.state == state).count());
    Ok(Gathered {
        space: open.space().clone(),
        head,
        counts: ExportCounts {
            events: count(lines.len()),
            facts: n(FactState::Active),
            pending: n(FactState::Pending),
            procedures: count(procedures),
        },
        lines,
        files,
        key: match key {
            VerificationKey::Include => Some(hex_of(open.digest.expose())),
            VerificationKey::Omit => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_space_turns_not_found_into_busy() {
        assert_eq!(not_found_or_busy(true, Refusal::NoSuchFact), Refusal::Busy);
        assert_eq!(
            not_found_or_busy(false, Refusal::NoSuchFact),
            Refusal::NoSuchFact
        );
    }
}
