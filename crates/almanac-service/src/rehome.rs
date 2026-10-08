//! Where a removed desktop-wide Space's memories go: pure, so it is a table.
//!
//! Each fact goes to the App Space (the app's first, `SpaceId::for_app`) of the app that wrote
//! it. A fact no app wrote goes to the fallback owner, and with none named the removal is
//! refused before anything moves.

use crate::open::Stored;
use almanac_core::{Actor, AppName, Confidentiality, Fact, Link, LocalSpace, Refusal, SpaceId};
use memfiles::VaultPath;
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

/// The fact as its new Space holds it: what was private to the removed Space is private to the
/// new one, and a link to an event of the removed log (which goes with it) is dropped. Its
/// integrity is untouched, so a pending fact stays pending.
pub(crate) fn rehomed(mut fact: Fact, from: &SpaceId, to: &SpaceId) -> Fact {
    if let Confidentiality::Private(spaces) = &mut fact.label.confidentiality {
        if spaces.remove(from) {
            spaces.insert(to.clone());
        }
    }
    fact.links
        .retain(|link| !matches!(link, Link::Event(event) if &event.space == from));
    fact
}
