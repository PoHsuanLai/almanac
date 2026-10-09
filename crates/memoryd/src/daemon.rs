//! `Daemon`: the bus handler. A call comes in with its sender's unique name; the sender becomes a
//! `Caller` (`Peers`), the call a `MemoryRequest` (`decode_request`), the request waits its turn
//! in its Space's queue (`Serialised`), and the reply goes back as the member's outputs
//! (`encode_reply`). After each request the daemon persists the files only it writes
//! (`spaces.toml`, `memory.toml`) and tells the bus what changed.

use crate::peers::Peers;
use crate::queue::Serialised;
use crate::removals::Removals;
use crate::signals::{FollowUp, follow_ups};
use almanac_core::{
    Caller, DesktopSpace, Dirs, MemoryReply, MemoryRequest, PlanToken, Removal, SpaceId, SpaceKind,
};
use almanac_dbus::{Call, MemoryError, Serve, Signal, decode_request, emit, encode_reply};
use almanac_service::{
    Backend, MemoryService, ServiceEvent, SpacesFile, locked_status, rules_to_toml, spaces_to_toml,
};
use porter_core::SpaceChange;
use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::OwnedFd;
use std::sync::{Mutex, OnceLock, PoisonError};

/// The daemon over a backend and a way to tell who is calling.
pub struct Daemon<B: Backend, P: Peers> {
    queue: Serialised<B>,
    peers: P,
    dirs: Dirs,
    bus: OnceLock<zbus::Connection>,
    /// The plans made and the Space each belongs to (`Forget` names only the token).
    plans: Mutex<BTreeMap<PlanToken, SpaceId>>,
    saved_spaces: Mutex<String>,
    saved_rules: Mutex<String>,
    removals: Removals,
}

impl<B: Backend, P: Peers> std::fmt::Debug for Daemon<B, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon").finish_non_exhaustive()
    }
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Writes `text` to `path` unless it is what the file already says (created with its parent).
fn write_if_changed(path: &std::path::Path, text: &str, last: &Mutex<String>) {
    if *locked(last) == text {
        return;
    }
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, text));
    match written {
        Ok(()) => *locked(last) = text.to_owned(),
        // The daemon's one log path is standard error, prefixed with its name.
        Err(e) => eprintln!("memoryd: could not write {}: {e}", path.display()),
    }
}

impl<B: Backend, P: Peers> Daemon<B, P> {
    /// A daemon over `service` in `dirs`. Nothing is on the bus until [`Daemon::attach`].
    pub fn new(service: MemoryService<B>, peers: P, dirs: Dirs) -> Self {
        Self {
            removals: Removals::load(dirs.memory().join("removals.toml")),
            queue: Serialised::new(service),
            peers,
            dirs,
            bus: OnceLock::new(),
            plans: Mutex::new(BTreeMap::new()),
            saved_spaces: Mutex::new(String::new()),
            saved_rules: Mutex::new(String::new()),
        }
    }

    /// The queues in front of the service.
    pub fn queue(&self) -> &Serialised<B> {
        &self.queue
    }

    /// Who is calling.
    pub fn peers(&self) -> &P {
        &self.peers
    }

    /// The connection signals go out on.
    pub fn attach(&self, connection: zbus::Connection) {
        drop(self.bus.set(connection));
    }

    /// Writes `spaces.toml` and `memory.toml` if the service changed what they say.
    pub fn persist(&self) {
        let service = self.queue.service();
        let spaces = SpacesFile {
            spaces: service.metas(),
        };
        if let Ok(text) = spaces_to_toml(&spaces) {
            write_if_changed(&self.dirs.spaces_toml(), &text, &self.saved_spaces);
        }
        if let Ok(text) = rules_to_toml(&service.rules()) {
            write_if_changed(&self.dirs.memory_toml(), &text, &self.saved_rules);
        }
    }

    async fn signal(&self, signal: Signal) {
        if let Some(bus) = self.bus.get() {
            // A signal nobody hears, or a bus that went away, does not fail the request.
            drop(emit(bus, &signal).await);
        }
    }

    async fn follow(&self, request: &MemoryRequest, reply: &MemoryReply) {
        match (request, reply) {
            (MemoryRequest::PlanForget(space, _), MemoryReply::Plan(plan)) => {
                locked(&self.plans).insert(plan.token.clone(), space.clone());
            }
            (MemoryRequest::Forget(token), MemoryReply::Forgot(report)) => {
                let space = locked(&self.plans).remove(token);
                if let Some(space) = space {
                    let report = serde_json::to_string(report).unwrap_or_default();
                    self.signal(Signal::Forgotten {
                        space: space.to_string(),
                        report,
                    })
                    .await;
                }
            }
            _ => {}
        }
        for step in follow_ups(request, reply) {
            match step {
                FollowUp::Emit(signal) => self.signal(signal).await,
                FollowUp::PendingChanged(space) => self.pending_changed(space).await,
                FollowUp::StatusChanged(space) => self.status_changed(space).await,
            }
        }
        self.flush_events().await;
    }

    /// Tells the bus what the service says happened since it last looked: pending facts settled
    /// or aged out, Spaces locked or open again. Called after every request and on the timers.
    pub async fn flush_events(&self) {
        loop {
            let events = self.queue.service().take_events();
            if events.is_empty() {
                return;
            }
            for event in events {
                match event {
                    ServiceEvent::PendingChanged(space) => self.pending_changed(space).await,
                    ServiceEvent::StatusChanged(space) => self.status_changed(space).await,
                    ServiceEvent::ConsolidationChanged(space, run) => {
                        let (space, run) = (space.to_string(), run.to_string());
                        self.signal(Signal::ConsolidationReady { space, run }).await;
                    }
                    ServiceEvent::Locked(space) => {
                        let status = serde_json::to_string(&locked_status()).unwrap_or_default();
                        let space = space.to_string();
                        self.signal(Signal::StatusChanged { space, status }).await;
                    }
                }
            }
        }
    }

    async fn pending_changed(&self, space: SpaceId) {
        let asked = MemoryRequest::Pending(space.clone());
        if let MemoryReply::Pending(pending) = self.queue.handle(&Caller::ShellUi, asked).await {
            let count = u32::try_from(pending.len()).unwrap_or(u32::MAX);
            let space = space.to_string();
            self.signal(Signal::PendingChanged { space, count }).await;
        }
    }

    async fn status_changed(&self, space: SpaceId) {
        let asked = MemoryRequest::Status(space.clone());
        if let MemoryReply::Status(status) = self.queue.handle(&Caller::ShellUi, asked).await {
            let status = serde_json::to_string(&status).unwrap_or_default();
            let space = space.to_string();
            self.signal(Signal::StatusChanged { space, status }).await;
        }
    }

    /// The timer's work: the key check, then the events it caused.
    pub async fn check_keys(&self) {
        self.queue.check_keys().await;
        self.flush_events().await;
    }

    /// The key check on every lock change the keyring announces, until the connection goes.
    pub async fn follow_keyring(&self, mut changes: crate::keyring::LockChanges) {
        while changes.next().await.is_some() {
            self.check_keys().await;
        }
    }

    /// The daily sweep over every Space, then the events it caused (aged-out pending facts).
    pub async fn sweep_all(
        &self,
    ) -> Vec<(
        SpaceId,
        Result<almanac_core::SweepReport, almanac_core::Refusal>,
    )> {
        let swept = self.queue.sweep_all().await;
        self.flush_events().await;
        swept
    }

    /// The nightly consolidation of every Space the service knows (the person's setting
    /// `memory.consolidation.when = "nightly"` is the caller's to check), each run announced the
    /// way a requested one is. A Space whose run is refused (a locked key) is skipped until the next
    /// night.
    pub async fn consolidate_all(&self) -> Vec<(SpaceId, MemoryReply)> {
        let spaces: Vec<SpaceId> = self
            .queue
            .service()
            .metas()
            .into_iter()
            .map(|meta| meta.id)
            .collect();
        let mut out = Vec::with_capacity(spaces.len());
        for space in spaces {
            let request = MemoryRequest::RunConsolidation(space.clone());
            let reply = self.queue.handle(&Caller::ShellUi, request.clone()).await;
            self.persist();
            self.follow(&request, &reply).await;
            out.push((space, reply));
        }
        out
    }

    /// Serves `call` as `caller`.
    pub async fn serve_as(
        &self,
        caller: &Caller,
        call: Call,
        fd: Option<OwnedFd>,
    ) -> Result<Vec<String>, MemoryError> {
        let request = decode_request(&call)?;
        let reply = match (&request, fd) {
            (MemoryRequest::Export(options), Some(fd)) => {
                let mut out = std::fs::File::from(fd);
                self.queue.export(caller, options, &mut out).await
            }
            (MemoryRequest::Export(_), None) => almanac_core::MemoryReply::Refused(
                almanac_core::Refusal::Invalid("Export needs the stream it writes to".into()),
            ),
            (MemoryRequest::RemoveSpace(space, removal), None)
                if *caller == Caller::ShellUi && matches!(space.kind(), SpaceKind::Linked(_)) =>
            {
                self.settle(space, *removal).await
            }
            _ => self.queue.handle(caller, request.clone()).await,
        };
        self.persist();
        self.follow(&request, &reply).await;
        encode_reply(&call, &reply)
    }
}

impl<B: Backend + 'static, P: Peers> Serve for Daemon<B, P> {
    async fn serve(
        &self,
        sender: &str,
        call: Call,
        fd: Option<OwnedFd>,
    ) -> Result<Vec<String>, MemoryError> {
        let caller = self
            .peers
            .caller_of(sender)
            .await
            .map_err(|refusal| MemoryError::from(&refusal))?;
        self.serve_as(&caller, call, fd).await
    }
}

impl<B: Backend, P: Peers> Daemon<B, P> {
    /// Settles `space`, a desktop-wide Space the registry no longer has, when no one asked the
    /// person: a choice the shell announced earlier (`RemoveSpace`, noted and not yet finished)
    /// stands, else the memories move to the App Space of the app that wrote them and the
    /// history to that of the app that recorded it ([`Removal::KEEP_ALL`]).
    pub async fn settle_removed(&self, space: &SpaceId) -> MemoryReply {
        let removal = self.removals.begin_unasked(space);
        self.settle(space, removal).await
    }

    /// Settles `space` as `removal` says. Noted first and struck out when done, so
    /// [`Daemon::resume_removals`] finishes what a crash or a busy Space interrupted.
    async fn settle(&self, space: &SpaceId, removal: Removal) -> MemoryReply {
        self.removals.begin(space, removal);
        let request = MemoryRequest::RemoveSpace(space.clone(), removal);
        let reply = self.queue.handle(&Caller::ShellUi, request.clone()).await;
        self.persist();
        if matches!(reply, MemoryReply::Relocated(_)) {
            self.removals.finish(space);
        }
        self.follow(&request, &reply).await;
        reply
    }

    /// Finishes every removal that was begun and not struck out.
    pub async fn resume_removals(&self) -> Vec<(SpaceId, MemoryReply)> {
        let mut out = Vec::new();
        for (space, removal) in self.removals.open() {
            out.push((space.clone(), self.settle(&space, removal).await));
        }
        out
    }

    /// Brings memory in line with the registry's list: a removal left unfinished is finished,
    /// and a desktop-wide Space memory holds that the registry does not (an id from before
    /// Spaces were per app that nothing adopted) is settled as a removed one, so no memory
    /// stays under a key nobody can resolve.
    pub async fn reconcile(&self, registry: &[DesktopSpace]) -> Vec<(SpaceId, MemoryReply)> {
        let known: BTreeSet<SpaceId> = registry.iter().map(SpaceId::linked).collect();
        for meta in self.queue.service().metas() {
            let orphan =
                matches!(meta.id.kind(), SpaceKind::Linked(_)) && !known.contains(&meta.id);
            if orphan {
                self.removals.begin_unasked(&meta.id);
            }
        }
        self.resume_removals().await
    }

    /// One change the registry announced.
    pub async fn on_change(&self, space: &DesktopSpace, change: SpaceChange) {
        if change == SpaceChange::Removed {
            drop(self.settle_removed(&SpaceId::linked(space)).await);
        }
    }

    /// The registry's changes, until the stream ends: each removal is settled. The list is
    /// read after subscribing, so a removal in between is caught by [`Daemon::reconcile`].
    pub async fn follow_spaces(&self, spaces: porter_client::Spaces) {
        let Ok(mut changes) = spaces.watch().await else {
            return;
        };
        if let Ok(listed) = spaces.list().await {
            let ids: Vec<DesktopSpace> = listed.into_iter().map(|record| record.id).collect();
            for (space, reply) in self.reconcile(&ids).await {
                if let MemoryReply::Refused(why) = reply {
                    eprintln!("memoryd: settling {space}: {why:?}");
                }
            }
        }
        while let Some(item) = std::future::poll_fn(|cx| {
            porter_dbus::BusStream::poll_next(std::pin::Pin::new(&mut changes), cx)
        })
        .await
        {
            if let Ok((space, change)) = item {
                self.on_change(&space, change).await;
            }
        }
    }
}
