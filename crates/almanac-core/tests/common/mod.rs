//! Sample values for the shape tests. Labels are built from fields: `Label`'s constructors are
//! stubs in porter's `prov` until its fill wave.
#![allow(dead_code)]

mod samples;
#[allow(unused_imports)]
pub use samples::*;

use almanac_core::*;
use std::collections::BTreeSet;

pub const NOW: UnixSeconds = UnixSeconds(1_790_000_000);

pub fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space id")
}

pub fn app(name: &str) -> AppName {
    AppName::parse(name).expect("app name")
}

pub fn thing(app_name: &str, kind: &str, key: &str) -> ThingRef {
    ThingRef {
        app: app(app_name),
        kind: ThingKind::parse(kind).expect("kind"),
        key: ThingKey::parse(key).expect("key"),
    }
}

pub fn view(app_name: &str, kind: &str, key: &str, title: &str) -> ThingView {
    ThingView {
        thing: thing(app_name, kind, key),
        title: title.into(),
        subtitle: "".into(),
    }
}

pub fn user_label() -> Label {
    Label {
        integrity: Integrity::Trusted,
        confidentiality: Confidentiality::Public,
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::User]),
    }
}

pub fn mail_label(space_id: &str) -> Label {
    Label {
        integrity: Integrity::Untrusted,
        confidentiality: Confidentiality::Private(BTreeSet::from([space(space_id)])),
        classes: BTreeSet::from([DataClass::Mail]),
        sources: BTreeSet::from([Source::Mail]),
    }
}

pub fn user() -> Actor {
    Actor::User {
        via: app("org.quire.Mail"),
    }
}

pub fn companion() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-1").expect("session"),
        role: AgentRole::Planner,
    }
}

pub fn archived_body() -> EventBody {
    EventBody::Thing {
        verb: Verb::Archived,
        thing: view("org.quire.Mail", "mail.thread", "7f3a", "Q4 budget"),
        sources: vec![view(
            "org.quire.Mail",
            "mail.message",
            "m1",
            "Re: Q4 budget",
        )],
    }
}

pub fn record(body: EventBody, actor: Actor) -> Record {
    Record {
        space: space("work"),
        occurred: NOW,
        actor,
        effect: Effect::UndoableWrite,
        label: user_label(),
        body,
        cause: Cause::None,
    }
}

pub fn area_body(area: AreaTag, kind: &str) -> EventBody {
    EventBody::Area(AreaPayload {
        area,
        kind: KindTag::parse(kind).expect("kind tag"),
        json: JsonText::parse(r#"{"ruling":"ask"}"#).expect("json"),
        things: vec![(
            view("org.quire.Mail", "mail.thread", "7f3a", "Q4 budget"),
            ThingRole::Subject,
        )],
    })
}

pub fn file_body(why: FileWhy) -> EventBody {
    EventBody::File {
        change: FileChange::Created,
        file: FileView {
            path: SpacePath::parse("/home/u/Downloads/receipt.pdf").expect("path"),
            inode: 4242,
            content: ContentDigest([7; 32]),
        },
        why,
    }
}

pub fn fact() -> Fact {
    Fact {
        id: FactId::parse("01j9zk3m0q8h2v6x4c1b7n5t2a").expect("fact id"),
        text: FactText::parse("Prefers meetings after 10:00.").expect("text"),
        recorded: NOW,
        by: user(),
        label: user_label(),
        links: vec![Link::Thing(thing("org.quire.Mail", "mail.thread", "7f3a"))],
        supersedes: Vec::new(),
        valid: Validity::Unstated,
    }
}

pub fn event_ref(seq: u64) -> EventRef {
    EventRef {
        space: space("work"),
        replica: ReplicaId([9; 16]),
        seq: Seq(seq),
    }
}

pub fn rule(scope: RuleScope, mode: RememberMode) -> RememberRule {
    RememberRule {
        id: RuleId::parse("r-1").expect("rule id"),
        scope,
        mode,
        retention: Retention::Days(DayCount(30)),
    }
}

pub fn round_trips<T>(values: &[T])
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    for value in values {
        let json = serde_json::to_string(value).expect("serialise");
        let back: T = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(&back, value, "{json}");
    }
}
