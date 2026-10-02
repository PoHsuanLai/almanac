#![allow(dead_code)]

use almanac_core::*;
use std::collections::BTreeSet;

pub const NOW: UnixSeconds = UnixSeconds(1_790_000_000);

pub fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space")
}

pub fn app(name: &str) -> AppName {
    AppName::parse(name).expect("app")
}

pub fn caller_app(name: &str) -> Caller {
    Caller::App(AppId {
        name: app(name),
        isolation: Isolation::Unsandboxed,
    })
}

pub fn thing(app_name: &str, key: &str) -> ThingRef {
    ThingRef {
        app: app(app_name),
        kind: ThingKind::parse("mail.thread").expect("kind"),
        key: ThingKey::parse(key).expect("key"),
    }
}

pub fn view(app_name: &str, key: &str) -> ThingView {
    ThingView {
        thing: thing(app_name, key),
        title: "t".into(),
        subtitle: "".into(),
    }
}

pub fn label(integrity: Integrity) -> Label {
    Label {
        integrity,
        confidentiality: Confidentiality::Public,
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::User]),
    }
}

pub fn user(via: &str) -> Actor {
    Actor::User { via: app(via) }
}

pub fn planner() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-1").expect("s"),
        role: AgentRole::Planner,
    }
}

pub fn cua_actor() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-1").expect("s"),
        role: AgentRole::Cua {
            run: RunId::parse("r-1").expect("r"),
        },
    }
}

pub fn thing_body(app_name: &str, key: &str) -> EventBody {
    EventBody::Thing {
        verb: Verb::Archived,
        thing: view(app_name, key),
        sources: vec![],
    }
}

pub fn area_body(area: AreaTag) -> EventBody {
    EventBody::Area(AreaPayload {
        area,
        kind: KindTag::parse("cua.step").expect("kind"),
        json: JsonText::parse("{}").expect("json"),
        things: vec![],
    })
}

pub fn record(body: EventBody, actor: Actor) -> Record {
    Record {
        space: space("work"),
        occurred: NOW,
        actor,
        effect: Effect::UndoableWrite,
        label: label(Integrity::Trusted),
        body,
        cause: Cause::None,
    }
}
