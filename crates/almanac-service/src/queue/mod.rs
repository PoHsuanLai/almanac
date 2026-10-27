//! The service's request queue: one line per service, a place per request.
//!
//! The service checks a Space out for the length of a request (its index is written across an
//! await), so a second request for the same Space that overlapped the first would have to be
//! answered `Busy`. Instead every entry point here takes its place in the [`Line`] first: a
//! request that names Spaces waits for those Spaces; one that reaches every Space, or that cannot
//! name them up front (`Forget`, `Settle`, `Revert`, `Export`, the rules, the key check), waits
//! for all of them. `Busy` therefore never reaches a caller for an overlap, and requests for
//! different Spaces still run side by side. A request the service will refuse anyway holds
//! nothing (see [`claim_for`]).

mod claim;
mod line;

pub use claim::{Claim, claim_for, claim_of};
pub(crate) use line::Line;

use crate::auth::{Allowed, allowed};
use crate::backend::Backend;
use crate::caller::Caller;
use crate::service::MemoryService;
use almanac_core::{ExportOptions, MemoryReply, MemoryRequest, Refusal, SpaceId, SweepReport};
use claim::claim_under;
use std::collections::BTreeSet;
use std::future::Future;
use std::io::Write;

impl<B: Backend> MemoryService<B> {
    /// Answers one request from `caller`, after the requests ahead of it on the same Spaces.
    /// Never fails: a refusal is a [`MemoryReply::Refused`].
    pub fn handle(
        &self,
        caller: &Caller,
        request: MemoryRequest,
    ) -> impl Future<Output = MemoryReply> + Send {
        let verdict = allowed(caller, &request);
        let claim = claim_under(&verdict, &request);
        async move {
            let _place = self.line.enter(claim).await;
            let result = match verdict {
                Allowed::No(refusal) => Err(refusal),
                Allowed::Yes => self.dispatch(caller, request).await,
            };
            result.unwrap_or_else(MemoryReply::Refused)
        }
    }

    /// Runs the retention sweep in every Space the service knows (memoryd's timer, daily), one
    /// Space at a time, so requests for the others (and for this one between passes) are not kept
    /// waiting for the whole round. A Space whose key is locked is skipped until the next round.
    pub async fn sweep_all(&self) -> Vec<(SpaceId, Result<SweepReport, Refusal>)> {
        let mut out = Vec::new();
        for id in self.all_spaces() {
            let swept = self.sweep_space(&id).await;
            out.push((id, swept));
        }
        out
    }

    /// Sweeps one Space, holding only that Space. A Space the service no longer knows is
    /// `SpaceUnknown`: the sweep never provisions one.
    pub async fn sweep_space(&self, id: &SpaceId) -> Result<SweepReport, Refusal> {
        if !self.known().contains(id) {
            return Err(Refusal::SpaceUnknown);
        }
        let _place = self
            .line
            .enter(Claim::Spaces(BTreeSet::from([id.clone()])))
            .await;
        let caller = Caller::ShellUi;
        let mut lease = self.checkout(&caller, id).await?;
        let cx = self.cx(&caller);
        let open = lease.open().ok_or(Refusal::Busy)?;
        let report = open.sweep(&cx);
        self.raise_all(open.outbox.drain(..));
        report
    }

    /// Every Space the service knows.
    pub fn space_ids(&self) -> Vec<SpaceId> {
        self.known()
    }

    /// Writes the export of `options` as a tar stream into `out`, and returns its manifest. It
    /// reads every Space it names, so it holds them all.
    pub fn export(
        &self,
        caller: &Caller,
        options: &ExportOptions,
        out: &mut (impl Write + Send),
    ) -> impl Future<Output = MemoryReply> + Send {
        let verdict = allowed(caller, &MemoryRequest::Export(options.clone()));
        let claim = match &verdict {
            Allowed::Yes => Claim::Everything,
            Allowed::No(_) => Claim::Nothing,
        };
        async move {
            let _place = self.line.enter(claim).await;
            let result = match verdict {
                Allowed::No(refusal) => Err(refusal),
                Allowed::Yes => self.export_to(caller, options, out).await,
            };
            result.map_or_else(MemoryReply::Refused, MemoryReply::Exported)
        }
    }
}
