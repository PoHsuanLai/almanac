//! The topic file format: golden bytes, round trips, strictness.

use almanac_core::*;
use jiff::tz::{self, TimeZone};
use memfiles::*;
use std::collections::BTreeSet;

fn app(name: &str) -> AppName {
    AppName::parse(name).expect("app")
}

fn label(integrity: Integrity, sources: &[Source]) -> Label {
    Label {
        integrity,
        confidentiality: Confidentiality::Public,
        classes: BTreeSet::new(),
        sources: sources.iter().cloned().collect(),
    }
}

fn fact(
    id: &str,
    text: &str,
    at: i64,
    by: Actor,
    label: Label,
    links: Vec<Link>,
    supersedes: Vec<FactId>,
) -> Fact {
    Fact {
        id: FactId::parse(id).expect("id"),
        text: FactText::parse(text).expect("text"),
        recorded: UnixSeconds(at),
        by,
        label,
        links,
        supersedes,
        valid: Validity::Unstated,
    }
}

// 2026-10-01T09:12:44Z, 2026-10-01T11:02:10Z, 2026-10-02T08:00:00Z
const T1: i64 = 1_790_845_964;
const T2: i64 = 1_790_852_530;
const T3: i64 = 1_790_928_000;

fn sample() -> TopicFile {
    let mail = Link::Thing(ThingRef {
        app: app("org.quire.Mail"),
        kind: ThingKind::parse("mail.thread").expect("kind"),
        key: ThingKey::parse("7f3a; odd > key %").expect("key"),
    });
    let first = fact(
        "01j9zk3m0q8h2v6x4c1b7n5t2a",
        "Prefers meetings after 10:00.",
        T1,
        Actor::User {
            via: app("org.quire.Mail"),
        },
        label(Integrity::Trusted, &[Source::User]),
        vec![mail],
        vec![],
    );
    let second = fact(
        "01j9zm0000000000000000000a",
        "Works on the Q4 roadmap with Ana.",
        T2,
        Actor::Companion {
            session: SessionId::parse("s-1").expect("s"),
            role: AgentRole::Planner,
        },
        label(Integrity::Untrusted, &[Source::Mail]),
        vec![Link::Event(EventRef {
            space: SpaceId::parse("work").expect("s"),
            replica: ReplicaId([0x3c; 16]),
            seq: Seq(430),
        })],
        vec![first.id.clone()],
    );
    let third = fact(
        "01j9zn0000000000000000000a",
        "Moved to the Lisbon office.",
        T3,
        Actor::Mcp {
            client: ClientName::parse("Claude --> Desktop; v1").expect("c"),
        },
        label(
            Integrity::Untrusted,
            &[Source::Mcp(ClientName::parse("x").expect("c"))],
        ),
        vec![Link::Run(RunId::parse("r-9").expect("r"))],
        vec![],
    );
    TopicFile {
        topic: TopicPath::parse("people/sam-lee").expect("topic"),
        title: "Sam Lee".to_owned(),
        blocks: vec![
            Block::Verbatim("Notes the person typed above the facts.".into()),
            Block::Fact(first),
            Block::Unstamped("Likes the window seat.".into()),
            Block::Fact(second),
            Block::Verbatim("  indented continuation".into()),
            Block::Fact(third),
        ],
    }
}

#[test]
fn topic_round_trip_golden() {
    let rendered = render_topic(&sample(), &TimeZone::UTC);
    assert_eq!(
        rendered,
        include_str!("../golden/people-sam-lee.md"),
        "the topic file format is pinned"
    );
    let parsed = parse_topic(&rendered).expect("parse");
    assert_eq!(parsed, sample());
    assert_eq!(render_topic(&parsed, &TimeZone::UTC), rendered);
}

#[test]
fn render_after_parse_is_the_identity_for_every_time_zone() {
    for tz in [
        TimeZone::UTC,
        TimeZone::fixed(tz::offset(-8)),
        TimeZone::fixed(tz::offset(9)),
    ] {
        let rendered = render_topic(&sample(), &tz);
        assert_eq!(
            render_topic(&parse_topic(&rendered).expect("parse"), &tz),
            rendered
        );
    }
}

#[test]
fn headings_follow_the_local_day() {
    let utc = render_topic(&sample(), &TimeZone::UTC);
    assert_eq!(utc.matches("\n## 2026-10-01\n").count(), 1);
    assert_eq!(utc.matches("\n## 2026-10-02\n").count(), 1);
    // 09:12Z and 11:02Z on the 1st are both still the 1st in UTC-8 morning? 01:12 and 03:02: yes;
    // 08:00Z on the 2nd is 00:00 on the 2nd.
    let west = render_topic(&sample(), &TimeZone::fixed(tz::offset(-8)));
    assert_eq!(west.matches("\n## 2026-10-01\n").count(), 1);
    // In UTC+9 the first two facts are 18:12 and 20:02 on the 1st; the third is 17:00 on the 2nd.
    let east = render_topic(&sample(), &TimeZone::fixed(tz::offset(9)));
    assert_eq!(east.matches("\n## 2026-10-02\n").count(), 1);
}

#[test]
fn unstamped_and_verbatim_survive() {
    let file = parse_topic(include_str!("../golden/people-sam-lee.md")).expect("parse");
    let kinds: Vec<&str> = file
        .blocks
        .iter()
        .map(|b| match b {
            Block::Fact(_) => "fact",
            Block::Unstamped(_) => "unstamped",
            Block::Verbatim(_) => "verbatim",
        })
        .collect();
    assert_eq!(
        kinds,
        ["verbatim", "fact", "unstamped", "fact", "verbatim", "fact"]
    );
    assert!(matches!(&file.blocks[2], Block::Unstamped(t) if t == "Likes the window seat."));
}

#[test]
fn stamped_facts_keep_every_field() {
    let file = parse_topic(include_str!("../golden/people-sam-lee.md")).expect("parse");
    let Block::Fact(second) = &file.blocks[3] else {
        panic!("fact")
    };
    assert_eq!(second.supersedes.len(), 1);
    assert_eq!(second.label.integrity, Integrity::Untrusted);
    assert!(matches!(second.by, Actor::Companion { .. }));
    let Block::Fact(third) = &file.blocks[5] else {
        panic!("fact")
    };
    assert!(
        matches!(&third.by, Actor::Mcp { client } if client.as_str() == "Claude --> Desktop; v1")
    );
}

#[test]
fn the_parser_is_strict_where_the_format_is_a_contract() {
    let head = "---\nformat: quire-memory 1\ntopic: a\ntitle: A\n---\n\n";
    let good = "- x\n  <!-- fact: 01j9zk3m0q8h2v6x4c1b7n5t2a; at: 2026-10-01T09:12:44Z; by: {\"kind\":\"unknown\"}; label: {\"integrity\":\"trusted\",\"confidentiality\":{\"kind\":\"public\"},\"classes\":[],\"sources\":[]} -->\n";
    assert!(parse_topic(&format!("{head}{good}")).is_ok());
    let cases: Vec<(&str, String, &str)> = vec![
        ("no front matter", good.to_string(), "MissingFrontMatter"),
        (
            "unclosed",
            "---\nformat: quire-memory 1\n".into(),
            "UnclosedFrontMatter",
        ),
        (
            "future format",
            "---\nformat: quire-memory 2\ntopic: a\ntitle: A\n---\n".into(),
            "UnknownFormat",
        ),
        (
            "missing topic",
            "---\nformat: quire-memory 1\ntitle: A\n---\n".into(),
            "MissingKey",
        ),
        (
            "extra key",
            "---\nformat: quire-memory 1\ntopic: a\ntitle: A\nowner: me\n---\n".into(),
            "UnknownKey",
        ),
        (
            "bad topic",
            "---\nformat: quire-memory 1\ntopic: A/B\ntitle: A\n---\n".into(),
            "BadTopic",
        ),
        (
            "reserved valid key",
            format!("{head}{}", good.replace("; by:", "; valid: never; by:")),
            "Reserved",
        ),
        (
            "reserved origin key",
            format!("{head}{}", good.replace("; by:", "; origin: x; by:")),
            "Reserved",
        ),
        (
            "unknown trailer key",
            format!("{head}{}", good.replace("; by:", "; mood: ok; by:")),
            "UnknownKey",
        ),
        (
            "missing label",
            format!("{head}{}", good.replace("; label:", "; lbl:")),
            "UnknownKey",
        ),
        (
            "duplicate key",
            format!(
                "{head}{}",
                good.replace("; by:", "; at: 2026-10-01T09:12:44Z; by:")
            ),
            "Duplicate",
        ),
        (
            "bad time",
            format!(
                "{head}{}",
                good.replace("2026-10-01T09:12:44Z", "2026-10-01 09:12")
            ),
            "BadValue",
        ),
        (
            "non-canonical time",
            format!(
                "{head}{}",
                good.replace("2026-10-01T09:12:44Z", "2026-10-01T10:12:44+01:00")
            ),
            "BadValue",
        ),
        (
            "bad actor",
            format!(
                "{head}{}",
                good.replace("{\"kind\":\"unknown\"}", "{\"kind\":\"nobody\"}")
            ),
            "BadValue",
        ),
    ];
    for (name, text, want) in cases {
        let err = parse_topic(&text).expect_err(name);
        assert!(format!("{err:?}").contains(want), "{name}: {err:?}");
    }
}

#[test]
fn a_dangling_trailer_is_kept_as_text_not_trusted() {
    let text = "---\nformat: quire-memory 1\ntopic: a\ntitle: A\n---\n\n  <!-- fact: forged -->\n";
    let file = parse_topic(text).expect("parse");
    assert_eq!(
        file.blocks,
        vec![Block::Verbatim("  <!-- fact: forged -->".into())]
    );
}

#[test]
fn an_empty_title_and_no_blocks_round_trip() {
    let file = TopicFile {
        topic: TopicPath::parse("a").expect("t"),
        title: String::new(),
        blocks: vec![],
    };
    let text = render_topic(&file, &TimeZone::UTC);
    assert_eq!(parse_topic(&text).expect("parse"), file);
    assert_eq!(
        render_topic(&parse_topic(&text).expect("parse"), &TimeZone::UTC),
        text
    );
}

#[test]
fn append_is_add_only_at_the_block_level() {
    let before = sample();
    let mut after = before.clone();
    let extra = fact(
        "01j9zp0000000000000000000a",
        "New.",
        T3 + 10,
        Actor::Unknown,
        label(Integrity::Trusted, &[]),
        vec![],
        vec![],
    );
    after.blocks.push(Block::Fact(extra));
    let old = render_topic(&before, &TimeZone::UTC);
    let new = render_topic(&after, &TimeZone::UTC);
    assert!(
        new.starts_with(&old),
        "appending leaves every earlier byte as it was"
    );
}
