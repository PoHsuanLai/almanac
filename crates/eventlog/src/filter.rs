//! The timeline filter applied to one entry: shared by every log implementation that filters
//! after reading.

use crate::chain::{BodyState, Entry};
use almanac_core::{ActorFilter, ActorKind, Integrity, TimelineFilter, TrustFilter};

/// Whether `entry` passes `filter`.
pub fn passes(filter: &TimelineFilter, entry: &Entry) -> bool {
    let h = &entry.header;
    let actor_ok = match filter.actors {
        ActorFilter::Everyone => true,
        ActorFilter::You => h.actor.kind() == ActorKind::User,
        ActorFilter::Companion => matches!(h.actor.kind(), ActorKind::Companion | ActorKind::Cua),
        ActorFilter::Terminal => h.actor.kind() == ActorKind::Cli,
        ActorFilter::Apps => matches!(h.actor.kind(), ActorKind::App | ActorKind::ThirdParty),
        ActorFilter::Unknown => h.actor.kind() == ActorKind::Unknown,
    };
    let kind_ok = filter.kinds.is_empty() || filter.kinds.iter().any(|k| k.covers(h.kind.as_str()));
    let trust_ok = match filter.trust {
        TrustFilter::Any => true,
        TrustFilter::TrustedOnly => h.label.integrity == Integrity::Trusted,
        TrustFilter::UntrustedOnly => h.label.integrity == Integrity::Untrusted,
    };
    let range_ok = filter
        .range
        .is_none_or(|(from, to)| h.occurred >= from && h.occurred <= to);
    let app_ok = filter.apps.is_empty()
        || match &entry.body {
            BodyState::Present(body) => body
                .things()
                .iter()
                .any(|(view, _)| filter.apps.contains(&view.thing.app)),
            BodyState::Erased => false,
        };
    actor_ok && kind_ok && trust_ok && range_ok && app_ok
}
