//! The companion amendment: messages and episodes as events, their searchable text, the budgeted
//! recall and recent-activity shapes.

mod common;

use almanac_core::*;
use common::*;
use std::collections::BTreeSet;

fn sender() -> Address {
    Address::new(
        AgentRef::Worker {
            task: TaskId::parse("t-4").expect("task"),
        },
        space("work"),
    )
}

fn message(kind: MessageKind, label: Label) -> Message {
    Message {
        id: MessageId::parse("m-2").expect("id"),
        thread: ThreadId::parse("m-1").expect("id"),
        in_reply_to: Some(MessageId::parse("m-1").expect("id")),
        from: sender(),
        to: Address::new(AgentRef::Companion, space("work")),
        kind,
        parts: vec![
            Part::Text(MessageText::new("found 3 receipts")),
            Part::Entity(thing("org.quire.Mail", "mail.thread", "t9")),
            Part::Outcome(OutcomeRef::parse("out:41").expect("ref")),
            Part::Undo(UndoHandle::parse("u-3").expect("ref")),
            Part::Text(MessageText::new("done")),
        ],
        label,
        sent: NOW,
    }
}

fn worker_actor() -> Actor {
    Actor::Companion {
        session: SessionId::parse("s-2").expect("session"),
        role: AgentRole::Worker {
            task: TaskId::parse("t-4").expect("task"),
        },
    }
}

fn skeleton() -> Skeleton {
    Skeleton {
        label: user_label(),
        asked: vec![MessageText::new("archive the Lisbon receipts")],
        steps: vec![StepLine {
            action: ActionName::parse("mail.thread.archive").expect("action"),
            targets: vec![thing("org.quire.Mail", "mail.thread", "t9")],
            effect: Effect::UndoableWrite,
            outcome: StepOutcome::Done,
            undo: Some(UndoHandle::parse("u-3").expect("undo")),
        }],
        touched: vec![(
            view("org.quire.Mail", "mail.thread", "t9", "Lisbon receipts"),
            ThingRole::Subject,
        )],
        results: vec![
            ResultLine {
                name: ResultName::parse("archived").expect("name"),
                value: ResultValue::Count(Count(3)),
            },
            ResultLine {
                name: ResultName::parse("last").expect("name"),
                value: ResultValue::Thing(thing("org.quire.Mail", "mail.thread", "t9")),
            },
            ResultLine {
                name: ResultName::parse("report").expect("name"),
                value: ResultValue::Outcome(OutcomeRef::parse("out:41").expect("ref")),
            },
        ],
    }
}

fn episode(narrative: Option<Narrative>) -> Episode {
    Episode {
        id: EpisodeId::parse("t-4").expect("id"),
        agent: AgentRef::Worker {
            task: TaskId::parse("t-4").expect("task"),
        },
        kind: EpisodeKind::Task,
        parent: Some(EpisodeId::parse("t-1").expect("id")),
        space: space("work"),
        started: NOW,
        ended: UnixSeconds(NOW.0 + 60),
        outcome: EpisodeOutcome::Done,
        skeleton: skeleton(),
        narrative,
    }
}

fn narrative() -> Narrative {
    Narrative {
        text: "Archived three Lisbon receipts.".into(),
        label: mail_label("work"),
        by: ModelRole::Consolidator,
    }
}

#[test]
fn messages_and_episodes_are_event_bodies_that_round_trip() {
    let bodies = vec![
        EventBody::Message(Box::new(message(
            MessageKind::Report {
                status: ReportStatus::Progress,
            },
            mail_label("work"),
        ))),
        EventBody::Message(Box::new(message(MessageKind::Request, user_label()))),
        EventBody::Episode(Box::new(episode(None))),
        EventBody::Episode(Box::new(episode(Some(narrative())))),
    ];
    round_trips(&bodies);
    let all: Vec<Episode> = vec![
        Episode {
            outcome: EpisodeOutcome::Handed {
                to: AgentRef::Cua {
                    run: RunId::parse("r-7").expect("run"),
                },
            },
            kind: EpisodeKind::Side,
            ..episode(None)
        },
        Episode {
            outcome: EpisodeOutcome::Open,
            parent: None,
            ..episode(None)
        },
    ];
    round_trips(&all);
    for outcome in [
        EpisodeOutcome::Done,
        EpisodeOutcome::Failed,
        EpisodeOutcome::Cancelled,
    ] {
        round_trips(&[outcome]);
    }
    let json = serde_json::to_string(&bodies[3]).expect("json");
    assert!(
        json.starts_with(r#"{"kind":"episode","v":{"id":"t-4""#),
        "{json}"
    );
    assert_eq!(EpisodeKind::ALL.len(), 2);
    assert_eq!(StepOutcome::ALL.len(), 4);
}

#[test]
fn kind_tags_things_and_names() {
    let m = EventBody::Message(Box::new(message(MessageKind::Note, user_label())));
    let e = EventBody::Episode(Box::new(episode(None)));
    assert_eq!(m.kind().as_str(), "companion.message");
    assert_eq!(e.kind().as_str(), "companion.episode");
    // An episode's touched things are its `things`; a message has views for none.
    assert_eq!(e.things().len(), 1);
    assert!(m.things().is_empty());
    // But a message that references an entity is forgotten with it.
    let named = thing("org.quire.Mail", "mail.thread", "t9");
    assert_eq!(m.thing_refs(), vec![(named.clone(), ThingRole::Source)]);
    assert!(m.names(&named) && e.names(&named));
    assert!(!m.names(&thing("org.quire.Mail", "mail.thread", "other")));
}

#[test]
fn index_texts_are_a_pure_view_with_a_label_per_part() {
    let mail = mail_label("work");
    let m = EventBody::Message(Box::new(message(MessageKind::Note, mail.clone())));
    let texts = m.index_texts();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].part, IndexPart::Message);
    assert_eq!(texts[0].text, "found 3 receipts\ndone");
    assert_eq!(texts[0].label, mail);

    let plain = EventBody::Episode(Box::new(episode(None))).index_texts();
    assert_eq!(plain.len(), 1);
    assert_eq!(plain[0].part, IndexPart::Skeleton);
    assert_eq!(
        plain[0].text,
        "asked: archive the Lisbon receipts\nstep: mail.thread.archive mail.thread/t9 done\ntouched: Lisbon receipts"
    );
    assert_eq!(plain[0].label, user_label());

    let narrated = EventBody::Episode(Box::new(episode(Some(narrative())))).index_texts();
    assert_eq!(narrated.len(), 2);
    assert_eq!(narrated[1].part, IndexPart::Narrative);
    assert_eq!(narrated[1].text, "Archived three Lisbon receipts.");
    // The tainted narrative does not taint the skeleton's own hit.
    assert_eq!(narrated[0].label, user_label());
    assert_eq!(narrated[1].label, mail);

    assert!(
        EventBody::Memory {
            op: MemoryOp::Resumed
        }
        .index_texts()
        .is_empty()
    );
}

#[test]
fn doc_ids_and_facets_per_part_and_search_scope() {
    let event = event_ref(7);
    let replica = hex_of(&event.replica.0);
    assert_eq!(IndexPart::Message.doc_id(&event), format!("e:{replica}:7"));
    assert_eq!(IndexPart::Skeleton.doc_id(&event), format!("e:{replica}:7"));
    assert_eq!(
        IndexPart::Narrative.doc_id(&event),
        format!("n:{replica}:7")
    );
    let cases = [
        (RecallOver::Facts, vec!["fact"]),
        (RecallOver::Messages, vec!["message"]),
        (RecallOver::Episodes, vec!["episode", "narrative"]),
        (
            RecallOver::Events,
            vec!["event", "message", "episode", "narrative"],
        ),
        (
            RecallOver::Both,
            vec!["fact", "event", "message", "episode", "narrative"],
        ),
    ];
    for (over, want) in cases {
        assert_eq!(over.facet_kinds(), want.as_slice(), "{over:?}");
    }
    // Every part's facet is covered by `Events`.
    for part in [
        IndexPart::Message,
        IndexPart::Skeleton,
        IndexPart::Narrative,
    ] {
        assert!(
            RecallOver::Events
                .facet_kinds()
                .contains(&part.facet_kind())
        );
    }
    assert_eq!(RecallOver::ALL.len(), 5);
}

#[test]
fn a_narrated_episode_succeeds_its_skeleton_only_event() {
    let plain = episode(None);
    let narrated = episode(Some(narrative()));
    assert_eq!(narrated.narrates(&plain), Succession::Narrates);
    assert_eq!(plain.narrates(&narrated), Succession::Unrelated);
    assert_eq!(plain.narrates(&plain), Succession::Unrelated);
    assert_eq!(narrated.narrates(&narrated), Succession::Unrelated);
    let mut other_skeleton = narrated.clone();
    other_skeleton.skeleton.asked.clear();
    assert_eq!(other_skeleton.narrates(&plain), Succession::Unrelated);
    let mut other_id = narrated;
    other_id.id = EpisodeId::parse("t-5").expect("id");
    assert_eq!(other_id.narrates(&plain), Succession::Unrelated);
}

#[test]
fn messages_and_episodes_are_audit_class_and_kept_thirty_days() {
    let rules = RuleSet::standard();
    let paused = SpaceState::Paused {
        until: UnixSeconds(NOW.0 + 60),
    };
    let marks = Marks::default();
    for body in [
        EventBody::Message(Box::new(message(MessageKind::Note, user_label()))),
        EventBody::Episode(Box::new(episode(None))),
    ] {
        let r = record(body, worker_actor());
        assert!(is_audit_class(&r));
        assert_eq!(default_retention(&r, &rules), Retention::Days(DayCount(30)));
        // Pause keeps the header, never the body, and never drops it.
        assert_eq!(
            admit(&r, &rules, &paused, &marks),
            Admission::HeaderOnly {
                retention: Retention::Days(DayCount(30))
            }
        );
    }
    // The person's own turn to a subagent is a message from the user, still audit class.
    let mut turn = message(MessageKind::Request, user_label());
    turn.from = Address::new(AgentRef::User, space("work"));
    let r = record(EventBody::Message(Box::new(turn)), user());
    assert!(is_audit_class(&r));
}

#[test]
fn tokens_are_estimated_and_budgets_fit_in_ranked_order() {
    for (text, want) in [
        ("", 0),
        ("a", 1),
        ("abcd", 1),
        ("abcde", 2),
        ("héllo wörld", 3),
    ] {
        assert_eq!(estimate_tokens(text), Tokens(want), "{text:?}");
    }
    let cost = |n: &u32| Tokens(*n);
    // (items in rank order, k, budget, taken)
    let cases: Vec<(Vec<u32>, u32, u32, Vec<u32>)> = vec![
        (vec![], 5, 100, vec![]),
        (vec![10, 10, 10], 5, 100, vec![10, 10, 10]),
        (vec![10, 10, 10], 2, 100, vec![10, 10]),
        (vec![10, 10, 10], 5, 25, vec![10, 10]),
        // an item that does not fit is skipped; a later smaller one may
        (vec![60, 50, 30], 5, 100, vec![60, 30]),
        (vec![200, 5], 5, 100, vec![5]),
        (vec![10], 0, 100, vec![]),
        (vec![0, 0], 5, 0, vec![0, 0]),
        (vec![1, 1], 5, 0, vec![]),
        (vec![u32::MAX, 1], 5, u32::MAX, vec![u32::MAX]),
    ];
    for (items, k, budget, want) in cases {
        assert_eq!(
            fit_budget(items.clone(), Count(k), Tokens(budget), cost),
            want,
            "{items:?} k={k} budget={budget}"
        );
    }
}

#[test]
fn recent_and_inject_name_their_space_and_the_scopes_are_audited() {
    let w = space("work");
    let inject = MemoryRequest::Inject(InjectQuery {
        space: w.clone(),
        text: "x".into(),
        budget: Tokens(1500),
        k: Count(8),
        over: RecallOver::Episodes,
        trust: TrustFilter::TrustedOnly,
    });
    let recent = MemoryRequest::Recent(
        w.clone(),
        RecentQuery {
            since: NOW,
            kinds: vec![],
            trust: TrustFilter::Any,
            limit: Count(5),
            bodies: BodyMode::Without,
        },
    );
    assert_eq!(inject.space(), Some(&w));
    assert_eq!(recent.space(), Some(&w));
    let scopes: BTreeSet<&str> = [ReadScope::Inject, ReadScope::Recent]
        .iter()
        .map(|s| match s {
            ReadScope::Inject => "inject",
            ReadScope::Recent => "recent",
            _ => "other",
        })
        .collect();
    assert_eq!(scopes.len(), 2);
    round_trips(&[ReadScope::Inject, ReadScope::Recent]);
    let json = serde_json::to_string(&inject).expect("json");
    assert!(
        json.starts_with(r#"{"kind":"inject","v":{"space":"work""#),
        "{json}"
    );
}

fn untrusted_entry(body: Option<JsonText>) -> RecentEntry {
    RecentEntry {
        summary: EventSummary {
            event: event_ref(7),
            occurred: NOW,
            kind: KindTag::parse("companion.message").expect("kind"),
            actor: companion(),
            things: vec![],
        },
        effect: Effect::Read,
        label: mail_label("work"),
        text: None,
        body,
    }
}

#[test]
fn recent_bodies_are_a_named_mode_and_always_travel_with_their_label() {
    let query = RecentQuery {
        since: NOW,
        kinds: vec![],
        trust: TrustFilter::Any,
        limit: Count(5),
        bodies: BodyMode::Json,
    };
    assert_eq!(
        serde_json::to_string(&query).expect("json"),
        r#"{"since":1790000000,"kinds":[],"trust":"any","limit":5,"bodies":"json"}"#
    );
    round_trips(BodyMode::ALL);
    assert_eq!(BodyMode::ALL.len(), 2);

    let with = untrusted_entry(Some(JsonText::parse(r#"{"x":1}"#).expect("json")));
    let json = serde_json::to_string(&with).expect("json");
    assert!(
        json.contains(r#""body":"{\"x\":1}""#) && json.contains(r#""label":"#),
        "an untrusted body is never sent without its label: {json}"
    );
    let without = serde_json::to_string(&untrusted_entry(None)).expect("json");
    assert!(without.contains(r#""body":null"#), "{without}");
    round_trips(&[with, untrusted_entry(None)]);
}

#[test]
fn session_kinds_are_not_for_recall_and_everything_else_is() {
    let kind = |k: &str| KindTag::parse(k).expect("kind");
    for session in [
        "companion.session.opened",
        "companion.session.taint",
        "companion.session.turn",
    ] {
        assert_eq!(
            Recallable::of_kind(&kind(session)),
            Recallable::No,
            "{session}"
        );
    }
    for other in [
        "companion.episode",
        "companion.message",
        "companion.sessions",
        "thing.archived",
        "policy.ruled",
        "cua.step",
    ] {
        assert_eq!(
            Recallable::of_kind(&kind(other)),
            Recallable::Yes,
            "{other}"
        );
    }
    round_trips(&[Recallable::Yes, Recallable::No]);
}
