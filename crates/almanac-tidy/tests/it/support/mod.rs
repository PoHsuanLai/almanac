use almanac_core::*;
use std::collections::BTreeSet;

pub const NOW: UnixSeconds = UnixSeconds(1_790_000_000);

pub fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space")
}

pub fn label(integrity: Integrity) -> Label {
    Label {
        integrity,
        confidentiality: Confidentiality::Public,
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::User]),
    }
}

pub fn planner() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-1").expect("s"),
        role: AgentRole::Planner,
    }
}
