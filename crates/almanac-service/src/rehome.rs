//! Where a removed desktop-wide Space's memories and events go: pure, so it is a table.
//!
//! Each fact goes to the App Space (the app's first, `SpaceId::for_app`) of the app that wrote
//! it, each event to the one of the app that recorded it. A fact or event no app wrote goes to
//! the fallback owner, and with none named the removal is refused before anything moves.

use crate::open::Stored;
use almanac_core::{
    Actor, AppName, Cause, Confidentiality, EventRef, Fact, Label, Link, LocalSpace, Refusal,
    SpaceId,
};
use almanac_store::{Entry, VaultPath};
use std::collections::BTreeMap;

/// An app's first Space: where its memories land (quire's Spaces kit numbers from 0).
pub(crate) const FIRST_SPACE: LocalSpace = LocalSpace(0);

/// What a Space held that is worth moving.
#[derive(Debug, Default)]
pub(crate) struct Held {
    pub facts: Vec<Stored>,
    pub procedures: Vec<(VaultPath, Vec<u8>)>,
}

/// What one App Space receives.
#[derive(Debug, Default)]
pub(crate) struct Parcel {
    pub facts: Vec<Stored>,
    pub procedures: Vec<(VaultPath, Vec<u8>)>,
}

/// Where each event of a removed log now is, by its old address.
pub(crate) type EventMap = BTreeMap<EventRef, EventRef>;

/// The app an actor acted through, when it was an app.
fn writer_app(actor: &Actor) -> Option<AppName> {
    match actor {
        Actor::User { via } => Some(via.clone()),
        Actor::App { app } | Actor::ThirdParty { app, .. } => Some(app.clone()),
        Actor::Companion { .. }
        | Actor::Mcp { .. }
        | Actor::Acp { .. }
        | Actor::Cli
        | Actor::System { .. }
        | Actor::Unknown => None,
    }
}

/// The app a procedure belongs to: `procedures/<app>/<name>.md`.
fn procedure_app(path: &VaultPath) -> Option<AppName> {
    let rest = path.as_str().strip_prefix("procedures/")?;
    AppName::parse(rest.split('/').next()?).ok()
}

fn home_of(owner: Option<AppName>, fallback: Option<&AppName>) -> Result<SpaceId, Refusal> {
    let app = owner.or_else(|| fallback.cloned()).ok_or_else(|| {
        Refusal::Invalid("no app is named to take memories that no app wrote".to_owned())
    })?;
    SpaceId::for_app(&app, FIRST_SPACE, None).map_err(|e| Refusal::Invalid(e.to_string()))
}

/// Every parcel, by the Space that receives it.
pub(crate) fn sort_into_parcels(
    held: Held,
    fallback: Option<&AppName>,
) -> Result<BTreeMap<SpaceId, Parcel>, Refusal> {
    let mut parcels: BTreeMap<SpaceId, Parcel> = BTreeMap::new();
    for stored in held.facts {
        let home = home_of(writer_app(&stored.fact.by), fallback)?;
        parcels.entry(home).or_default().facts.push(stored);
    }
    for (path, bytes) in held.procedures {
        let home = home_of(procedure_app(&path), fallback)?;
        parcels
            .entry(home)
            .or_default()
            .procedures
            .push((path, bytes));
    }
    Ok(parcels)
}

/// Every event of a removed log, by the Space that receives it, oldest first.
pub(crate) fn sort_events(
    entries: Vec<Entry>,
    fallback: Option<&AppName>,
) -> Result<BTreeMap<SpaceId, Vec<Entry>>, Refusal> {
    let mut by_home: BTreeMap<SpaceId, Vec<Entry>> = BTreeMap::new();
    for entry in entries {
        let home = home_of(writer_app(&entry.header.actor), fallback)?;
        by_home.entry(home).or_default().push(entry);
    }
    Ok(by_home)
}

/// The label as the new Space holds it: what was private to the removed Space is private to
/// the new one.
pub(crate) fn rehomed_label(mut label: Label, from: &SpaceId, to: &SpaceId) -> Label {
    if let Confidentiality::Private(spaces) = &mut label.confidentiality
        && spaces.remove(from)
    {
        spaces.insert(to.clone());
    }
    label
}

/// The cause as the new log holds it: an event that moved is named by its new address, one of
/// the removed log that did not (not yet moved, or gone) is dropped.
pub(crate) fn rehomed_cause(cause: Cause, from: &SpaceId, moved: &EventMap) -> Cause {
    match cause {
        Cause::Event(event) if &event.space == from => {
            moved.get(&event).cloned().map_or(Cause::None, Cause::Event)
        }
        other => other,
    }
}

/// The fact as its new Space holds it: what was private to the removed Space is private to the
/// new one, and a link to an event of the removed log follows the event if it moved and is
/// dropped if it did not. Its integrity is untouched, so a pending fact stays pending.
pub(crate) fn rehomed(mut fact: Fact, from: &SpaceId, to: &SpaceId, moved: &EventMap) -> Fact {
    fact.label = rehomed_label(fact.label, from, to);
    fact.links = fact
        .links
        .into_iter()
        .filter_map(|link| match link {
            Link::Event(event) if &event.space == from => {
                moved.get(&event).cloned().map(Link::Event)
            }
            other => Some(other),
        })
        .collect();
    fact
}
