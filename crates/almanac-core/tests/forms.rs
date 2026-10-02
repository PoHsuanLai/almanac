//! The stored forms, kind tags and pinned JSON.

mod common;

use almanac_core::*;
use common::*;

#[test]
fn stored_forms_round_trip() {
    round_trips(&[
        archived_body(),
        file_body(FileWhy::Unexplained),
        file_body(FileWhy::Explained {
            cause: thing("org.quire.Mail", "mail.message", "m1"),
            verb: Verb::Downloaded,
            by: user(),
        }),
        EventBody::Search {
            app: app("org.quire.Mail"),
            text: "lisbon".into(),
            scope: ThingKind::parse("mail.thread").expect("k"),
            results: Count(12),
        },
        EventBody::Memory {
            op: MemoryOp::Forgot {
                plan: PlanDigest([1; 32]),
                counts: ForgetCounts {
                    events: Count(1),
                    facts: Count(1),
                    pending: Count(0),
                    procedures: Count(0),
                    index_docs: Count(1),
                },
            },
        },
        EventBody::Memory {
            op: MemoryOp::Checkpoint {
                cut: Seq(5),
                link: Link32([4; 32]),
            },
        },
        area_body(AreaTag::Cua, "cua.step"),
    ]);
    round_trips(&[record(archived_body(), companion())]);
    round_trips(&[fact()]);
    round_trips(&[
        Cause::None,
        Cause::Event(event_ref(1)),
        Cause::Undo("t".into()),
        Cause::Plan(SessionId::parse("s-1").expect("s")),
    ]);
    let scopes = [
        RuleScope::Space(space("work")),
        RuleScope::App(app("org.quire.Mail")),
        RuleScope::Kind(KindPattern::any()),
        RuleScope::Path(PathGlob::parse("/a/**").expect("g")),
        RuleScope::Thing(thing("org.quire.Mail", "mail.thread", "k")),
        RuleScope::Actor(ActorKind::Cua),
    ];
    let rules: Vec<RememberRule> = scopes
        .into_iter()
        .map(|s| rule(s, RememberMode::Never))
        .collect();
    round_trips(&rules);
    round_trips(&[
        Retention::Days(DayCount(3)),
        Retention::WhileSourceExists,
        Retention::UntilForgotten,
    ]);
    round_trips(&[SpaceMeta {
        id: space("work"),
        created: NOW,
        replica: ReplicaId([3; 16]),
        vault: VaultKind::Plain,
        format: 1,
    }]);
    round_trips(&[Caller::Router, Caller::Cuad, Caller::ShellUi]);
}

#[test]
fn kind_tags_are_one_match() {
    let cases: Vec<(EventBody, &str)> = vec![
        (archived_body(), "thing.archived"),
        (file_body(FileWhy::Unexplained), "file.created"),
        (
            EventBody::Search {
                app: app("org.quire.Mail"),
                text: "x".into(),
                scope: ThingKind::parse("mail.thread").expect("k"),
                results: Count(0),
            },
            "search.performed",
        ),
        (
            EventBody::Memory {
                op: MemoryOp::Resumed,
            },
            "memory.resumed",
        ),
        (
            EventBody::Memory {
                op: MemoryOp::FactAdded {
                    fact: fact().id,
                    topic: TopicPath::parse("a").expect("t"),
                },
            },
            "memory.fact_added",
        ),
        (area_body(AreaTag::Docket, "policy.ruled"), "policy.ruled"),
    ];
    for (body, tag) in cases {
        assert_eq!(body.kind().as_str(), tag);
    }
    for verb in Verb::ALL {
        let body = EventBody::Thing {
            verb: *verb,
            thing: view("org.quire.Mail", "mail.thread", "k", "t"),
            sources: vec![],
        };
        assert!(KindTag::parse(body.kind().as_str()).is_ok(), "{verb:?}");
    }
}

#[test]
fn pinned_json() {
    let cases: Vec<(String, &str)> = vec![
        (
            serde_json::to_string(&Refusal::PlanStale).expect("j"),
            r#"{"kind":"plan_stale"}"#,
        ),
        (
            serde_json::to_string(&Refusal::Invalid("x".into())).expect("j"),
            r#"{"kind":"invalid","v":"x"}"#,
        ),
        (
            serde_json::to_string(&MemoryRequest::Spaces).expect("j"),
            r#"{"kind":"spaces"}"#,
        ),
        (
            serde_json::to_string(&MemoryRequest::Primer(space("work"))).expect("j"),
            r#"{"kind":"primer","v":"work"}"#,
        ),
        (
            serde_json::to_string(&Retention::Days(DayCount(30))).expect("j"),
            r#"{"kind":"days","v":30}"#,
        ),
        (
            serde_json::to_string(&Cause::None).expect("j"),
            r#"{"kind":"none"}"#,
        ),
        (
            serde_json::to_string(&FileChange::Renamed {
                from: SpacePath::parse("/a").expect("p"),
            })
            .expect("j"),
            r#"{"kind":"renamed","v":{"from":"/a"}}"#,
        ),
        (
            serde_json::to_string(&Verb::Forwarded).expect("j"),
            r#""forwarded""#,
        ),
        (
            serde_json::to_string(&RememberMode::HeaderOnly).expect("j"),
            r#""header_only""#,
        ),
        (
            serde_json::to_string(&EntryBody::Erased {
                by: EraseCause::HeaderOnly,
            })
            .expect("j"),
            r#"{"kind":"erased","v":{"by":"header_only"}}"#,
        ),
        (
            serde_json::to_string(&Admission::Drop(DropReason::Paused)).expect("j"),
            r#"{"kind":"drop","v":{"kind":"paused"}}"#,
        ),
        (serde_json::to_string(&Seq(7)).expect("j"), "7"),
        (
            serde_json::to_string(&KindTag::parse("thing.archived").expect("k")).expect("j"),
            r#""thing.archived""#,
        ),
    ];
    for (got, want) in cases {
        assert_eq!(got, want);
    }
}

#[test]
fn every_slug_enum_has_its_slug_as_its_serde_form() {
    for verb in Verb::ALL {
        assert_eq!(
            serde_json::to_string(verb).expect("j"),
            format!("\"{}\"", verb.slug())
        );
    }
    for tag in AreaTag::ALL {
        assert_eq!(
            serde_json::to_string(tag).expect("j"),
            format!("\"{}\"", tag.slug())
        );
    }
}

#[test]
fn directory_layout_is_pinned() {
    let dirs = Dirs::new("/d".into(), "/c".into(), "/cfg".into(), "/run".into());
    let w = space("work");
    let topic = TopicPath::parse("people/sam-lee").expect("t");
    let paths = [
        dirs.spaces_toml(),
        dirs.system_events_db(),
        dirs.events_db(&w),
        dirs.primer(&w),
        dirs.topic(&w, &topic),
        dirs.pending(&w),
        dirs.procedures(&w),
        dirs.consolidation(&w),
        dirs.index_db(&w),
        dirs.memory_toml(),
        dirs.edit(&w),
    ];
    let want = [
        "/d/quire/memory/spaces.toml",
        "/d/quire/memory/system/events.db",
        "/d/quire/memory/work/events.db",
        "/d/quire/memory/work/facts/INDEX.md",
        "/d/quire/memory/work/facts/people/sam-lee.md",
        "/d/quire/memory/work/pending",
        "/d/quire/memory/work/procedures",
        "/d/quire/memory/work/consolidation",
        "/c/quire/memory/work/index.db",
        "/cfg/quire/memory.toml",
        "/run/quire/memory/edit/work",
    ];
    for (got, want) in paths.iter().zip(want) {
        assert_eq!(got.to_str(), Some(want));
    }
}
