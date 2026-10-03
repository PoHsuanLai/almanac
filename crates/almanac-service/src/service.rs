//! `MemoryService`: memoryd's core, over a [`Backend`]: every request is authorised with
//! [`allowed`], routed to its Space and run through the pure machines; their effects are applied
//! here. A Space is checked out of the service for the length of a request (its index is written
//! across an await), so a second request for the same Space meanwhile is answered `Busy`.

use crate::auth::{Allowed, allowed};
use crate::backend::Backend;
use crate::clock::Clock;
use crate::events::ServiceEvent;
use crate::forget::{Plan, PlanState};
use crate::open::{Cx, ErasedNotes, Open};
use crate::record::Stored;
use almanac_core::{
    Caller, ChainHealth, Count, ExportOptions, Marks, MemoryReply, MemoryRequest, PlanToken,
    Record, Refusal, RuleSet, RunState, SpaceId, SpaceMeta, SpaceState, SpaceSummary, VaultKind,
};
use almanac_seal::{KeyError, KeyStore, Purpose, derive};
use memfiles::Store;
use recall::Index;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::io::Write;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// One open Space.
pub struct SpaceRuntime<B: Backend> {
    /// Its `spaces.toml` entry.
    pub meta: SpaceMeta,
    /// Where it is in its lifecycle.
    pub state: SpaceState,
    /// Its event log.
    pub log: B::Log,
    /// Its facts.
    pub store: Store<B::Files>,
    /// Its index.
    pub index: Index<B::Vectors>,
    /// Things marked "do not remember".
    pub marks: Marks,
    /// Records waiting while it is locked (at most `BUFFER_LIMIT`).
    pub buffer: Vec<Record>,
    /// Forget plans made and not yet applied or lapsed.
    pub plans: BTreeMap<PlanToken, (Plan, PlanState)>,
}

impl<B: Backend> std::fmt::Debug for SpaceRuntime<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpaceRuntime")
            .field("meta", &self.meta)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// A Space either sits in the service or is out on a request.
enum Slot<B: Backend> {
    Ready(Box<Open<B>>),
    Busy,
}

/// memoryd's core: every request goes through [`allowed`], then the machines.
pub struct MemoryService<B: Backend> {
    backend: B,
    rules: Mutex<RuleSet>,
    spaces: Mutex<BTreeMap<SpaceId, Slot<B>>>,
    metas: Mutex<BTreeMap<SpaceId, SpaceMeta>>,
    buffers: Mutex<BTreeMap<SpaceId, Vec<Record>>>,
    /// Spaces whose lost key was announced, until they open again.
    locked: Mutex<BTreeSet<SpaceId>>,
    events: Mutex<Vec<ServiceEvent>>,
}

impl<B: Backend> std::fmt::Debug for MemoryService<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryService").finish_non_exhaustive()
    }
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key_refusal(e: KeyError) -> Refusal {
    match e {
        KeyError::Locked => Refusal::SpaceLocked,
        KeyError::Missing | KeyError::Exists | KeyError::Store(_) => Refusal::Busy,
    }
}

/// A Space out on a request; it goes back into the service when the lease drops.
pub(crate) struct Lease<'a, B: Backend> {
    service: &'a MemoryService<B>,
    id: SpaceId,
    open: Option<Box<Open<B>>>,
}

impl<B: Backend> Lease<'_, B> {
    pub(crate) fn open(&mut self) -> Option<&mut Open<B>> {
        self.open.as_deref_mut()
    }

    /// The Space is gone: it does not go back.
    pub(crate) fn discard(mut self) {
        self.open = None;
        locked(&self.service.spaces).remove(&self.id);
    }
}

impl<B: Backend> Drop for Lease<'_, B> {
    fn drop(&mut self) {
        if let Some(open) = self.open.take() {
            locked(&self.service.spaces).insert(self.id.clone(), Slot::Ready(open));
        }
    }
}

impl<B: Backend> MemoryService<B> {
    /// A service over `backend` with the person's rules. No Space is open yet.
    pub fn new(backend: B, rules: RuleSet) -> Self {
        Self {
            backend,
            rules: Mutex::new(rules),
            spaces: Mutex::new(BTreeMap::new()),
            metas: Mutex::new(BTreeMap::new()),
            buffers: Mutex::new(BTreeMap::new()),
            locked: Mutex::new(BTreeSet::new()),
            events: Mutex::new(Vec::new()),
        }
    }

    fn raise(&self, event: ServiceEvent) {
        locked(&self.events).push(event);
    }

    pub(crate) fn raise_all(&self, events: impl IntoIterator<Item = ServiceEvent>) {
        locked(&self.events).extend(events);
    }

    /// A Space found without its key: said once, until it opens again.
    fn note_locked(&self, id: &SpaceId) {
        if locked(&self.locked).insert(id.clone()) {
            self.raise(ServiceEvent::Locked(id.clone()));
        }
    }

    /// What happened since the last call that the bus should hear of, oldest first: pending
    /// facts settled or aged out, Spaces locked or open again. memoryd calls it after every
    /// request and on its timers.
    pub fn take_events(&self) -> Vec<ServiceEvent> {
        std::mem::take(&mut locked(&self.events))
    }

    /// The timer's key check: an open Space whose key the store no longer gives is closed and
    /// announced as locked (the keyring locked); a locked one whose key is back is opened again,
    /// which flushes the records it buffered. Spaces out on a request are left to it.
    pub async fn check_keys(&self) {
        let open: Vec<SpaceId> = locked(&self.spaces)
            .iter()
            .filter(|(_, slot)| matches!(slot, Slot::Ready(_)))
            .map(|(id, _)| id.clone())
            .collect();
        for id in open {
            if matches!(self.backend.keys().get(&id).await, Err(KeyError::Locked)) {
                let mut spaces = locked(&self.spaces);
                if matches!(spaces.get(&id), Some(Slot::Ready(_))) {
                    spaces.remove(&id);
                    drop(spaces);
                    self.note_locked(&id);
                }
            }
        }
        let waiting: Vec<SpaceId> = locked(&self.locked).iter().cloned().collect();
        for id in waiting {
            if self.backend.keys().get(&id).await.is_ok() {
                drop(self.checkout(&Caller::ShellUi, &id).await);
            }
        }
    }

    /// The backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Tells the service about a Space from `spaces.toml`. A Space named before it is
    /// registered is provisioned on first use (a key, a sealed vault, a fresh replica); memoryd
    /// persists [`MemoryService::metas`].
    pub fn register(&self, meta: SpaceMeta) {
        locked(&self.metas).insert(meta.id.clone(), meta);
    }

    /// Every Space the service knows, registered or provisioned.
    pub fn metas(&self) -> Vec<SpaceMeta> {
        locked(&self.metas).values().cloned().collect()
    }

    /// The rules now (memoryd persists them after a `SetRule` or `RemoveRule`).
    pub fn rules(&self) -> RuleSet {
        locked(&self.rules).clone()
    }

    pub(crate) fn cx<'a>(&'a self, caller: &'a Caller) -> Cx<'a, B> {
        Cx {
            backend: &self.backend,
            rules: self.rules(),
            caller,
        }
    }

    fn known(&self) -> Vec<SpaceId> {
        let mut ids: Vec<SpaceId> = locked(&self.metas).keys().cloned().collect();
        ids.extend(locked(&self.spaces).keys().cloned());
        ids.sort();
        ids.dedup();
        ids
    }

    /// Takes `id` out of the service, opening it first if need be.
    pub(crate) async fn checkout(
        &self,
        caller: &Caller,
        id: &SpaceId,
    ) -> Result<Lease<'_, B>, Refusal> {
        let previous = locked(&self.spaces).insert(id.clone(), Slot::Busy);
        match previous {
            Some(Slot::Ready(open)) => Ok(Lease {
                service: self,
                id: id.clone(),
                open: Some(open),
            }),
            Some(Slot::Busy) => Err(Refusal::Busy),
            None => match self.open_space(caller, id).await {
                Ok(open) => {
                    if locked(&self.locked).remove(id) {
                        self.raise(ServiceEvent::StatusChanged(id.clone()));
                    }
                    Ok(Lease {
                        service: self,
                        id: id.clone(),
                        open: Some(Box::new(open)),
                    })
                }
                Err(e) => {
                    locked(&self.spaces).remove(id);
                    if e == Refusal::SpaceLocked {
                        self.note_locked(id);
                    }
                    Err(e)
                }
            },
        }
    }

    async fn open_space(&self, caller: &Caller, id: &SpaceId) -> Result<Open<B>, Refusal> {
        let keys = self.backend.keys();
        let key = match keys.get(id).await {
            Ok(key) => key,
            Err(KeyError::Missing) => keys.create(id).await.map_err(key_refusal)?,
            Err(e) => return Err(key_refusal(e)),
        };
        let digest = derive(&key, id, Purpose::Digest);
        let meta = locked(&self.metas)
            .entry(id.clone())
            .or_insert_with(|| {
                let mut replica = [0u8; 16];
                replica.copy_from_slice(
                    &blake3::keyed_hash(digest.expose(), b"QREPLICA1").as_bytes()[..16],
                );
                SpaceMeta {
                    id: id.clone(),
                    created: self.backend.clock().now(),
                    replica: almanac_core::ReplicaId(replica),
                    vault: VaultKind::Sealed,
                    format: 1,
                }
            })
            .clone();
        let log = self
            .backend
            .open_log(id, meta.replica, &key)
            .map_err(crate::open::failed)?;
        let files = self
            .backend
            .open_files(&meta, &key)
            .map_err(crate::open::failed)?;
        let index = self
            .backend
            .open_index(id, &key)
            .map_err(crate::open::failed)?;
        let store = Store::new(files, id.clone(), jiff::tz::TimeZone::UTC);
        let marks = crate::marks::load(store.vault());
        let baseline = crate::baseline::Baseline::load(store.vault());
        let mut open = Open {
            rt: SpaceRuntime {
                meta,
                state: SpaceState::Open,
                log,
                store,
                index,
                marks,
                buffer: Vec::new(),
                plans: BTreeMap::new(),
            },
            digest,
            chain: ChainHealth::Unchecked,
            minted: 0,
            notes: ErasedNotes::default(),
            run: RunState::Idle,
            last: None,
            baseline,
            dirty: std::collections::BTreeSet::new(),
            outbox: Vec::new(),
        };
        let cx = self.cx(caller);
        open.sync_index(&cx).await?;
        open.guard_topics()?;
        let waiting = locked(&self.buffers).remove(id).unwrap_or_default();
        for record in waiting {
            open.record(&cx, record).await?;
        }
        Ok(open)
    }

    /// Answers one request from `caller`. Never fails: a refusal is a [`MemoryReply::Refused`].
    pub fn handle(
        &self,
        caller: &Caller,
        request: MemoryRequest,
    ) -> impl Future<Output = MemoryReply> + Send {
        let verdict = allowed(caller, &request);
        async move {
            let result = match verdict {
                Allowed::No(refusal) => Err(refusal),
                Allowed::Yes => self.dispatch(caller, request).await,
            };
            result.unwrap_or_else(MemoryReply::Refused)
        }
    }

    /// Runs the retention sweep in every Space the service knows (memoryd's timer, daily). A
    /// Space whose key is locked is skipped until the next round.
    pub async fn sweep_all(&self) -> Vec<(SpaceId, Result<almanac_core::SweepReport, Refusal>)> {
        let caller = Caller::ShellUi;
        let mut out = Vec::new();
        for id in self.all_spaces() {
            let swept = match self.checkout(&caller, &id).await {
                Ok(mut lease) => {
                    let cx = self.cx(&caller);
                    match lease.open() {
                        Some(open) => {
                            let report = open.sweep(&cx);
                            self.raise_all(open.outbox.drain(..));
                            report
                        }
                        None => Err(Refusal::Busy),
                    }
                }
                Err(e) => Err(e),
            };
            out.push((id, swept));
        }
        out
    }

    /// Writes the export of `options` as a tar stream into `out`, and returns its manifest.
    pub fn export(
        &self,
        caller: &Caller,
        options: &ExportOptions,
        out: &mut (impl Write + Send),
    ) -> impl Future<Output = MemoryReply> + Send {
        let verdict = allowed(caller, &MemoryRequest::Export(options.clone()));
        async move {
            let result = match verdict {
                Allowed::No(refusal) => Err(refusal),
                Allowed::Yes => self.export_to(caller, options, out).await,
            };
            result.map_or_else(MemoryReply::Refused, MemoryReply::Exported)
        }
    }

    /// One record into its Space; a locked Space buffers it.
    pub(crate) async fn record_one(
        &self,
        caller: &Caller,
        mut record: Record,
    ) -> Result<Stored, Refusal> {
        if let almanac_core::EventBody::Message(m) = &record.body {
            // A message lands in the receiving Space, with the sender's label.
            record.space = m.to.space.clone();
        }
        let id = record.space.clone();
        match self.checkout(caller, &id).await {
            Ok(mut lease) => {
                let cx = self.cx(caller);
                let open = lease.open().ok_or(Refusal::Busy)?;
                open.record(&cx, record).await
            }
            Err(Refusal::SpaceLocked) => self.buffer(&id, record),
            Err(other) => Err(other),
        }
    }

    fn buffer(&self, id: &SpaceId, record: Record) -> Result<Stored, Refusal> {
        use crate::space::{SpaceEffect, SpaceEvent, step};
        let mut buffers = locked(&self.buffers);
        let waiting = buffers.entry(id.clone()).or_default();
        let buffered = Count(u32::try_from(waiting.len()).unwrap_or(u32::MAX));
        let (_, effects) = step(SpaceState::Locked, SpaceEvent::Record { buffered });
        if effects.contains(&SpaceEffect::Buffer) {
            waiting.push(record);
            Ok(Stored::Dropped)
        } else {
            Err(Refusal::SpaceLocked)
        }
    }

    pub(crate) fn forget_meta(&self, id: &SpaceId) {
        locked(&self.metas).remove(id);
    }

    /// The summaries of every known Space.
    pub(crate) fn summaries(&self) -> Vec<SpaceSummary> {
        let metas = locked(&self.metas);
        let spaces = locked(&self.spaces);
        metas
            .values()
            .map(|m| SpaceSummary {
                id: m.id.clone(),
                state: match spaces.get(&m.id) {
                    Some(Slot::Ready(o)) => o.rt.state,
                    Some(Slot::Busy) => SpaceState::Open,
                    None => SpaceState::Locked,
                },
                vault: m.vault,
                created: m.created,
            })
            .collect()
    }

    /// The open Space that holds the plan `token`.
    pub(crate) fn space_of_plan(&self, token: &PlanToken) -> Option<SpaceId> {
        locked(&self.spaces)
            .iter()
            .find_map(|(id, slot)| match slot {
                Slot::Ready(o) if o.rt.plans.contains_key(token) => Some(id.clone()),
                _ => None,
            })
    }

    /// The open Space whose last consolidation run is `run`.
    pub(crate) fn space_of_run(&self, run: &almanac_core::RunId) -> Option<SpaceId> {
        locked(&self.spaces)
            .iter()
            .find_map(|(id, slot)| match slot {
                Slot::Ready(o) if o.last.as_ref().is_some_and(|l| &l.run == run) => {
                    Some(id.clone())
                }
                _ => None,
            })
    }

    /// Every Space the service knows, for the requests that name none.
    pub(crate) fn all_spaces(&self) -> Vec<SpaceId> {
        self.known()
    }

    /// Logs `op` in every open Space (rules are global).
    pub(crate) fn audit_open(&self, now: almanac_core::UnixSeconds, op: &almanac_core::MemoryOp) {
        for slot in locked(&self.spaces).values_mut() {
            if let Slot::Ready(open) = slot {
                let _ = open.audit(now, op.clone());
            }
        }
    }

    pub(crate) fn set_rules(&self, f: impl FnOnce(&mut RuleSet)) {
        f(&mut locked(&self.rules));
    }
}
