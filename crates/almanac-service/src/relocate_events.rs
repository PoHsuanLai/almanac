//! A removed Space's event history, taken in by the App Space of the app that recorded it.
//!
//! Each event is matched against what the log already holds (same times, actor, kind, label
//! and body digest) before it is appended, so a move that stopped half way finishes without
//! duplicating what it had already moved.

use crate::backend::Backend;
use crate::docs::event_ref;
use crate::open::{Cx, Open, append_refusal};
use crate::rehome::{EventMap, rehomed_cause, rehomed_label};
use crate::relocate::count;
use crate::service::MemoryService;
use almanac_core::{Caller, Count, Refusal, SpaceId};
use eventlog::{BodyState, Entry, LogWrite, NewHeader, body_digest};

/// The header `entry` gets in the log of `here`, apart from its cause.
fn moved_header(
    entry: &Entry,
    digest: Option<almanac_core::Digest32>,
    from: &SpaceId,
    here: &SpaceId,
) -> NewHeader {
    let h = &entry.header;
    NewHeader {
        occurred: h.occurred,
        recorded: h.recorded,
        actor: h.actor.clone(),
        kind: h.kind.clone(),
        effect: h.effect,
        label: rehomed_label(h.label.clone(), from, here),
        cause: h.cause.clone(),
        body_digest: digest.unwrap_or(h.body_digest),
    }
}

/// Whether `held` is `wanted` already (the cause is not compared: it follows from the rest).
fn is_same(wanted: &NewHeader, held: &Entry) -> bool {
    let h = &held.header;
    (
        wanted.occurred,
        wanted.recorded,
        wanted.effect,
        wanted.body_digest,
    ) == (h.occurred, h.recorded, h.effect, h.body_digest)
        && (&wanted.actor, &wanted.kind, &wanted.label) == (&h.actor, &h.kind, &h.label)
}

impl<B: Backend> Open<B> {
    /// Takes in the events a removed Space's log held for this one, oldest first, and notes
    /// where each now is.
    pub(crate) async fn receive_events(
        &mut self,
        cx: &Cx<'_, B>,
        from: &SpaceId,
        events: Vec<Entry>,
        moved: &mut EventMap,
    ) -> Result<(), Refusal> {
        let here = self.space().clone();
        let mut free: Vec<Option<Entry>> = self.entries()?.into_iter().map(Some).collect();
        for entry in events {
            let digest = match &entry.body {
                BodyState::Present(body) => Some(body_digest(&self.digest, body)),
                BodyState::Erased => None,
            };
            let mut wanted = moved_header(&entry, digest, from, &here);
            let held = free
                .iter_mut()
                .find(|slot| slot.as_ref().is_some_and(|e| is_same(&wanted, e)))
                .and_then(Option::take);
            let landed = match held {
                Some(held) => held,
                None => {
                    wanted.cause = rehomed_cause(wanted.cause, from, moved);
                    let body = match entry.body {
                        BodyState::Present(ref body) => Some(body.clone()),
                        BodyState::Erased => None,
                    };
                    let landed = self.rt.log.append(wanted, body).map_err(append_refusal)?;
                    self.index_entry(cx, &landed).await;
                    landed
                }
            };
            moved.insert(event_ref(from, &entry), event_ref(&here, &landed));
        }
        Ok(())
    }
}

impl<B: Backend> MemoryService<B> {
    /// Moves the events of the removed Space `from` to the App Spaces of the apps that recorded
    /// them. Answers how many, and where each went.
    pub(crate) async fn move_events(
        &self,
        caller: &Caller,
        from: &SpaceId,
        events: Vec<Entry>,
    ) -> Result<(Count, EventMap), Refusal> {
        let homes = crate::rehome::sort_events(events, self.fallback_owner().as_ref())?;
        let mut moved = EventMap::new();
        for (home, events) in homes {
            let mut lease = self.checkout(caller, &home).await?;
            let cx = self.cx(caller);
            let open = lease.open().ok_or(Refusal::Busy)?;
            open.receive_events(&cx, from, events, &mut moved).await?;
        }
        Ok((count(moved.len()), moved))
    }
}
