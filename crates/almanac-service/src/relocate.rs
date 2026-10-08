//! A desktop-wide Space was removed: its memories move to the App Space of the app that wrote
//! them (pending ones stay pending), or, when the person chose it, are deleted with the Space.
//!
//! Safe to repeat and to resume after a crash: facts are copied by id (a fact already in the
//! App Space is left alone) and only then is the Space deleted, so a run that stopped half way
//! finishes the next time it is asked.

use crate::backend::Backend;
use crate::docs::fact_doc;
use crate::events::ServiceEvent;
use crate::open::{Cx, Open, Stored, failed, files_refusal};
use crate::rehome::{Held, Parcel, rehomed, sort_into_parcels};
use crate::service::MemoryService;
use almanac_core::{
    Caller, Count, FactId, FactState, MemoryFate, MemoryOp, Refusal, Relocation, SpaceId, SpaceKind,
};
use almanac_seal::{KeyError, KeyStore};
use memfiles::{Vault, VaultPath};
use std::collections::BTreeSet;

fn count(n: usize) -> Count {
    Count(u32::try_from(n).unwrap_or(u32::MAX))
}

impl<B: Backend> Open<B> {
    /// What this Space holds that is worth moving.
    fn held(&self) -> Result<Held, Refusal> {
        let vault = self.rt.store.vault();
        let procedures = vault
            .list(&VaultPath::procedures_dir())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| Some((path.clone(), vault.read(&path).ok()?)))
            .collect();
        Ok(Held {
            facts: self.stored()?,
            procedures,
        })
    }

    /// Takes in what a removed Space left for this one, skipping what it already holds.
    async fn receive(
        &mut self,
        cx: &Cx<'_, B>,
        from: &SpaceId,
        parcel: Parcel,
    ) -> Result<(), Refusal> {
        let here = self.space().clone();
        let have: BTreeSet<FactId> = self.stored()?.into_iter().map(|s| s.fact.id).collect();
        let mut facts = parcel.facts;
        facts.sort_by(|a, b| (a.fact.recorded, &a.fact.id).cmp(&(b.fact.recorded, &b.fact.id)));
        self.guard_topics()?;
        let mut indexed = Vec::new();
        for Stored { topic, fact, state } in
            facts.into_iter().filter(|s| !have.contains(&s.fact.id))
        {
            let fact = rehomed(fact, from, &here);
            match state {
                FactState::Pending => self.rt.store.stage(fact, topic).map_err(files_refusal)?,
                FactState::Active | FactState::Superseded { .. } => {
                    self.rt
                        .store
                        .append(&topic, fact.clone())
                        .map_err(files_refusal)?;
                    self.audit(
                        cx.now(),
                        MemoryOp::FactAdded {
                            fact: fact.id.clone(),
                            topic,
                        },
                    )?;
                    if state == FactState::Active {
                        indexed.push(fact_doc(&fact));
                    }
                }
            }
        }
        self.index_put(cx, indexed).await;
        let vault = self.rt.store.vault();
        let there = vault.list(&VaultPath::procedures_dir()).unwrap_or_default();
        for (path, bytes) in parcel.procedures.iter().filter(|(p, _)| !there.contains(p)) {
            vault.write_atomic(path, bytes).map_err(failed)?;
        }
        self.accept_topics()?;
        self.outbox.push(ServiceEvent::PendingChanged(here));
        Ok(())
    }
}

impl<B: Backend> MemoryService<B> {
    /// Settles the memories of the removed desktop-wide Space `space` as `fate` says.
    pub(crate) async fn remove_space(
        &self,
        caller: &Caller,
        space: &SpaceId,
        fate: MemoryFate,
    ) -> Result<Relocation, Refusal> {
        if !matches!(space.kind(), SpaceKind::Linked(_)) {
            return Err(Refusal::Invalid(
                "only a desktop-wide Space is removed this way".to_owned(),
            ));
        }
        if !self.has_meta(space) && self.keys_destroyed(space).await {
            // An earlier run got as far as destroying the key: only the directories are left.
            self.backend().remove_space(space).map_err(failed)?;
            return Ok(Relocation::NONE);
        }
        if !self.has_meta(space) {
            return Ok(Relocation::NONE);
        }
        let held = {
            let mut lease = self.checkout(caller, space).await?;
            lease.open().ok_or(Refusal::Busy)?.held()?
        };
        let moved = match fate {
            MemoryFate::MoveToApps => self.move_to_apps(caller, space, held).await?,
            MemoryFate::Delete => Relocation {
                deleted: count(held.facts.len()),
                ..Relocation::NONE
            },
        };
        let lease = self.checkout(caller, space).await?;
        self.delete_space(caller, space, lease).await?;
        Ok(moved)
    }

    async fn keys_destroyed(&self, space: &SpaceId) -> bool {
        matches!(
            self.backend().keys().get(space).await,
            Err(KeyError::Missing)
        )
    }

    async fn move_to_apps(
        &self,
        caller: &Caller,
        from: &SpaceId,
        held: Held,
    ) -> Result<Relocation, Refusal> {
        let parcels = sort_into_parcels(held, self.fallback_owner().as_ref())?;
        let mut report = Relocation::NONE;
        for (home, parcel) in parcels {
            let (all, pending) = (
                parcel.facts.len(),
                parcel
                    .facts
                    .iter()
                    .filter(|s| s.state == FactState::Pending)
                    .count(),
            );
            let mut lease = self.checkout(caller, &home).await?;
            let cx = self.cx(caller);
            let open = lease.open().ok_or(Refusal::Busy)?;
            open.receive(&cx, from, parcel).await?;
            self.raise_all(open.outbox.drain(..));
            report.moved = count(report.moved.0 as usize + all);
            report.kept_pending = count(report.kept_pending.0 as usize + pending);
        }
        Ok(report)
    }
}
