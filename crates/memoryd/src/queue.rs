//! One queue per Space in front of `MemoryService`.
//!
//! The service checks a Space out for the length of a request (its index is written across an
//! await), so a second request for the same Space that overlapped the first would be answered
//! `Busy`. [`Serialised`] makes them wait their turn instead: a request that names Spaces holds
//! their queues (a fair mutex each, taken in id order so two multi-Space requests cannot
//! deadlock); a request that reaches every Space, or one it cannot name up front (`Forget`,
//! `Settle`, `Revert`, `Export`, the rules, the timer's sweep), holds them all. `Busy` therefore
//! never reaches a client, and requests for different Spaces still run side by side.

use almanac_core::{
    Caller, EventBody, ExportOptions, MemoryReply, MemoryRequest, Record, SpaceId, SweepReport,
};
use almanac_service::{Backend, MemoryService};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

/// What a request needs exclusive use of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// Nothing: it only reads the service's lists (`Spaces`, `Rules`).
    Nothing,
    /// These Spaces' queues.
    Spaces(BTreeSet<SpaceId>),
    /// Every Space.
    Everything,
}

/// The Space a record lands in: a message goes to the receiver's Space.
fn landing(record: &Record) -> SpaceId {
    match &record.body {
        EventBody::Message(m) => m.to.space.clone(),
        _ => record.space.clone(),
    }
}

/// What `request` must hold. Pure, so it is a table.
pub fn claim_of(request: &MemoryRequest) -> Claim {
    use MemoryRequest as R;
    match request {
        R::Spaces | R::Rules => Claim::Nothing,
        R::Record(r) => Claim::Spaces(BTreeSet::from([landing(r)])),
        R::RecordBatch(rs) => Claim::Spaces(rs.iter().map(landing).collect()),
        R::Forget(_)
        | R::Settle(..)
        | R::Revert(_)
        | R::SetRule(_)
        | R::RemoveRule(_)
        | R::Export(_) => Claim::Everything,
        other => other.space().map_or(Claim::Everything, |s| {
            Claim::Spaces(BTreeSet::from([s.clone()]))
        }),
    }
}

/// A [`MemoryService`] whose requests queue per Space.
pub struct Serialised<B: Backend> {
    service: MemoryService<B>,
    everything: RwLock<()>,
    queues: Mutex<BTreeMap<SpaceId, Arc<AsyncMutex<()>>>>,
}

impl<B: Backend> std::fmt::Debug for Serialised<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Serialised").finish_non_exhaustive()
    }
}

impl<B: Backend> Serialised<B> {
    /// Queues in front of `service`.
    pub fn new(service: MemoryService<B>) -> Self {
        Self {
            service,
            everything: RwLock::new(()),
            queues: Mutex::new(BTreeMap::new()),
        }
    }

    /// The service, for what needs no queue (its configuration and its backend).
    pub fn service(&self) -> &MemoryService<B> {
        &self.service
    }

    fn queue_of(&self, space: &SpaceId) -> Arc<AsyncMutex<()>> {
        self.queues
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(space.clone())
            .or_default()
            .clone()
    }

    /// Runs `work` holding what `claim` names.
    async fn holding<T>(&self, claim: Claim, work: impl std::future::Future<Output = T>) -> T {
        match claim {
            Claim::Nothing => work.await,
            Claim::Everything => {
                let _all = self.everything.write().await;
                work.await
            }
            Claim::Spaces(spaces) => {
                let _some = self.everything.read().await;
                let queues: Vec<Arc<AsyncMutex<()>>> =
                    spaces.iter().map(|s| self.queue_of(s)).collect();
                let mut held = Vec::with_capacity(queues.len());
                for queue in &queues {
                    held.push(queue.lock().await);
                }
                work.await
            }
        }
    }

    /// Answers `request` from `caller`, after the requests ahead of it on the same Spaces.
    pub async fn handle(&self, caller: &Caller, request: MemoryRequest) -> MemoryReply {
        let claim = claim_of(&request);
        self.holding(claim, self.service.handle(caller, request))
            .await
    }

    /// Writes an export into `out` (it reads every Space it names, so it holds them all).
    pub async fn export(
        &self,
        caller: &Caller,
        options: &ExportOptions,
        out: &mut (impl Write + Send),
    ) -> MemoryReply {
        self.holding(Claim::Everything, self.service.export(caller, options, out))
            .await
    }

    /// The retention sweep over every Space (the daily timer).
    pub async fn sweep_all(&self) -> Vec<(SpaceId, Result<SweepReport, almanac_core::Refusal>)> {
        self.holding(Claim::Everything, self.service.sweep_all())
            .await
    }

    /// The key check over every Space (the minute timer): a lost key closes its Space, a
    /// returned one opens it again.
    pub async fn check_keys(&self) {
        self.holding(Claim::Everything, self.service.check_keys())
            .await;
    }
}
