//! The authorisation matrix.

mod common;

use almanac_core::*;
use almanac_service::{Allowed, allowed};
use common::*;

const MAIL: &str = "org.quire.Mail";

fn callers() -> [(&'static str, Caller); 4] {
    [
        ("app", caller_app(MAIL)),
        ("router", Caller::Router),
        ("cuad", Caller::Cuad),
        ("shell", Caller::ShellUi),
    ]
}

fn yes(a: Allowed) -> bool {
    a == Allowed::Yes
}

#[test]
fn recording_follows_the_caller_class() {
    // (name, request, [app, router, cuad, shell])
    let cases: Vec<(&str, MemoryRequest, [bool; 4])> = vec![
        (
            "own thing as the user",
            MemoryRequest::Record(record(thing_body(MAIL, "k"), user(MAIL))),
            [true, true, false, false],
        ),
        (
            "own thing as the app itself",
            MemoryRequest::Record(record(thing_body(MAIL, "k"), Actor::App { app: app(MAIL) })),
            [true, true, false, false],
        ),
        (
            "another app's thing",
            MemoryRequest::Record(record(thing_body("org.quire.Files", "k"), user(MAIL))),
            [false, true, false, false],
        ),
        (
            "acting as another app",
            MemoryRequest::Record(record(thing_body(MAIL, "k"), user("org.quire.Files"))),
            [false, true, false, false],
        ),
        (
            "a companion actor",
            MemoryRequest::Record(record(thing_body(MAIL, "k"), planner())),
            [false, true, false, false],
        ),
        (
            "an unknown actor",
            MemoryRequest::Record(record(thing_body(MAIL, "k"), Actor::Unknown)),
            [false, true, false, false],
        ),
        (
            "own search",
            MemoryRequest::Record(record(
                EventBody::Search {
                    app: app(MAIL),
                    text: "x".into(),
                    scope: ThingKind::parse("mail.thread").expect("k"),
                    results: Count(1),
                },
                user(MAIL),
            )),
            [true, true, false, false],
        ),
        (
            "a cua step by the run",
            MemoryRequest::Record(record(area_body(AreaTag::Cua), cua_actor())),
            [false, true, true, false],
        ),
        (
            "a cua step by the planner",
            MemoryRequest::Record(record(area_body(AreaTag::Cua), planner())),
            [false, true, false, false],
        ),
        (
            "a docket audit by cuad",
            MemoryRequest::Record(record(area_body(AreaTag::Docket), cua_actor())),
            [false, true, false, false],
        ),
        (
            "memory's own audit",
            MemoryRequest::Record(record(
                EventBody::Memory {
                    op: MemoryOp::Resumed,
                },
                Actor::System {
                    part: SystemPart::Memory,
                },
            )),
            [false, false, false, false],
        ),
        (
            "a batch of own things",
            MemoryRequest::RecordBatch(vec![
                record(thing_body(MAIL, "a"), user(MAIL)),
                record(thing_body(MAIL, "b"), user(MAIL)),
            ]),
            [true, true, false, false],
        ),
        (
            "a batch with one foreign thing",
            MemoryRequest::RecordBatch(vec![
                record(thing_body(MAIL, "a"), user(MAIL)),
                record(thing_body("org.quire.Files", "b"), user(MAIL)),
            ]),
            [false, true, false, false],
        ),
    ];
    for (name, request, want) in cases {
        for ((who, caller), want) in callers().into_iter().zip(want) {
            assert_eq!(yes(allowed(&caller, &request)), want, "{name} by {who}");
        }
    }
    assert_eq!(
        allowed(&Caller::Router, &MemoryRequest::RecordBatch(vec![])),
        Allowed::No(Refusal::Invalid("empty batch".into()))
    );
}

#[test]
fn app_cannot_claim_companion_actor() {
    let mail = caller_app(MAIL);
    let forged = MemoryRequest::Record(record(thing_body(MAIL, "k"), planner()));
    assert_eq!(allowed(&mail, &forged), Allowed::No(Refusal::NotAllowed));
    let mcp = MemoryRequest::Record(record(
        thing_body(MAIL, "k"),
        Actor::Mcp {
            client: ClientName::parse("x").expect("c"),
        },
    ));
    assert_eq!(allowed(&mail, &mcp), Allowed::No(Refusal::NotAllowed));
}

#[test]
fn explaining_files_and_marking_things() {
    let claim = |cause_app: &str, by: Actor| {
        MemoryRequest::ExplainFile(FileWhyClaim {
            space: space("work"),
            path: SpacePath::parse("/a").expect("p"),
            content: ContentDigest([1; 32]),
            cause: thing(cause_app, "k"),
            verb: Verb::Downloaded,
            by,
        })
    };
    let mark = |app_name: &str| {
        MemoryRequest::Mark(MarkRequest {
            space: space("work"),
            thing: thing(app_name, "k"),
            mark: MarkKind::DoNotRemember,
        })
    };
    let mail = caller_app(MAIL);
    assert!(yes(allowed(&mail, &claim(MAIL, user(MAIL)))));
    assert!(!yes(allowed(&mail, &claim("org.quire.Files", user(MAIL)))));
    assert!(!yes(allowed(&mail, &claim(MAIL, planner()))));
    assert!(yes(allowed(
        &Caller::Router,
        &claim("org.quire.Files", planner())
    )));
    assert!(yes(allowed(&Caller::ShellUi, &claim(MAIL, user(MAIL)))));
    assert!(!yes(allowed(&Caller::Cuad, &claim(MAIL, user(MAIL)))));
    assert!(yes(allowed(&mail, &mark(MAIL))));
    assert!(!yes(allowed(&mail, &mark("org.quire.Files"))));
    assert!(!yes(allowed(&Caller::Cuad, &mark(MAIL))));
    assert!(yes(allowed(&Caller::ShellUi, &mark("org.quire.Files"))));
}

#[test]
fn reading_proposing_and_planning_belong_to_the_router_and_the_shell() {
    let w = space("work");
    let reads: Vec<MemoryRequest> = vec![
        MemoryRequest::Search(RecallQuery {
            space: w.clone(),
            text: "x".into(),
            limit: Count(1),
            over: RecallOver::Both,
        }),
        MemoryRequest::Facts(FactQuery {
            space: w.clone(),
            topic: None,
            about: None,
            state: FactFilter::All,
            limit: Count(1),
        }),
        MemoryRequest::Related(w.clone(), thing(MAIL, "k")),
        MemoryRequest::Provenance(w.clone(), SpacePath::parse("/a").expect("p")),
        MemoryRequest::Primer(w.clone()),
        MemoryRequest::Propose(
            w.clone(),
            FactDraft {
                topic: TopicPath::parse("a").expect("t"),
                text: FactText::parse("x").expect("t"),
                links: vec![],
                supersedes: vec![],
            },
        ),
        MemoryRequest::PlanForget(w.clone(), ForgetScope::Space),
        MemoryRequest::Inject(InjectQuery {
            space: w.clone(),
            text: "x".into(),
            budget: Tokens(1500),
            k: Count(8),
            over: RecallOver::Both,
            trust: TrustFilter::TrustedOnly,
        }),
        MemoryRequest::Recent(
            w.clone(),
            RecentQuery {
                since: NOW,
                kinds: vec![],
                trust: TrustFilter::Any,
                limit: Count(10),
                bodies: BodyMode::Without,
            },
        ),
        MemoryRequest::Recent(
            w.clone(),
            RecentQuery {
                since: NOW,
                kinds: vec![],
                trust: TrustFilter::Any,
                limit: Count(11),
                bodies: BodyMode::Json,
            },
        ),
    ];
    for request in &reads {
        for (who, caller) in callers() {
            assert_eq!(
                yes(allowed(&caller, request)),
                who == "router" || who == "shell",
                "{request:?} by {who}"
            );
        }
    }
}

/// docket lists the Spaces to name the one a task works in; it is the router's call as well.
#[test]
fn listing_the_spaces_is_the_routers_and_the_shells() {
    for (who, caller) in callers() {
        assert_eq!(
            yes(allowed(&caller, &MemoryRequest::Spaces)),
            who == "router" || who == "shell",
            "Spaces by {who}"
        );
    }
}

#[test]
fn control_is_the_shells_alone() {
    let w = space("work");
    let control: Vec<MemoryRequest> = vec![
        MemoryRequest::Status(w.clone()),
        MemoryRequest::Timeline(
            w.clone(),
            TimelineQuery {
                before: None,
                limit: Count(1),
                filter: TimelineFilter {
                    actors: ActorFilter::Everyone,
                    apps: vec![],
                    kinds: vec![],
                    trust: TrustFilter::Any,
                    range: None,
                },
            },
        ),
        MemoryRequest::Forget(PlanToken::parse("p-1").expect("t")),
        MemoryRequest::Pending(w.clone()),
        MemoryRequest::Settle(FactId::mint(1, [0; 10]), Settlement::Discard),
        MemoryRequest::Consolidation(w.clone()),
        MemoryRequest::RunConsolidation(w.clone()),
        MemoryRequest::Revert(RunId::parse("run-1").expect("r")),
        MemoryRequest::ApplyConsolidation(RunId::parse("run-1").expect("r")),
        MemoryRequest::Rules,
        MemoryRequest::RemoveRule(RuleId::parse("r-1").expect("r")),
        MemoryRequest::Pause(w.clone(), NOW),
        MemoryRequest::Resume(w.clone()),
        MemoryRequest::Verify(w.clone()),
        MemoryRequest::Rebuild(w.clone()),
        MemoryRequest::Sweep(w),
        MemoryRequest::Export(ExportOptions {
            spaces: vec![],
            verification_key: VerificationKey::Omit,
        }),
    ];
    for request in &control {
        for (who, caller) in callers() {
            assert_eq!(
                yes(allowed(&caller, request)),
                who == "shell",
                "{request:?} by {who}"
            );
        }
    }
}

fn message_from(agent: AgentRef) -> EventBody {
    EventBody::Message(Box::new(Message {
        id: MessageId::parse("m-2").expect("id"),
        thread: ThreadId::parse("m-1").expect("id"),
        in_reply_to: None,
        from: Address::new(agent, space("work")),
        to: Address::new(AgentRef::Companion, space("home")),
        kind: MessageKind::Report {
            status: ReportStatus::Done,
        },
        parts: vec![Part::Text(MessageText::new("done"))],
        label: label(Integrity::Untrusted),
        sent: NOW,
    }))
}

fn episode_body() -> EventBody {
    EventBody::Episode(Box::new(Episode {
        id: EpisodeId::parse("r-1").expect("id"),
        agent: AgentRef::Cua {
            run: RunId::parse("r-1").expect("r"),
        },
        kind: EpisodeKind::Task,
        parent: None,
        space: space("work"),
        started: NOW,
        ended: NOW,
        outcome: EpisodeOutcome::Done,
        skeleton: Skeleton {
            label: label(Integrity::Trusted),
            asked: vec![],
            steps: vec![],
            touched: vec![],
            results: vec![],
        },
        narrative: None,
    }))
}

#[test]
fn messages_and_episodes_are_recorded_by_the_router_and_a_run_reports_for_itself() {
    let run = AgentRef::Cua {
        run: RunId::parse("r-1").expect("r"),
    };
    let other_run = AgentRef::Cua {
        run: RunId::parse("r-2").expect("r"),
    };
    // (name, request, [app, router, cuad, shell])
    let cases: Vec<(&str, MemoryRequest, [bool; 4])> = vec![
        (
            "a run's report, by the run",
            MemoryRequest::Record(record(message_from(run.clone()), cua_actor())),
            [false, true, true, false],
        ),
        (
            "a run's report claiming to be another run",
            MemoryRequest::Record(record(message_from(other_run), cua_actor())),
            [false, true, false, false],
        ),
        (
            "a report from the companion, as the run",
            MemoryRequest::Record(record(message_from(AgentRef::Companion), cua_actor())),
            [false, true, false, false],
        ),
        (
            "a run's report by another actor",
            MemoryRequest::Record(record(message_from(run), planner())),
            [false, true, false, false],
        ),
        (
            "the user's turn to a subagent",
            MemoryRequest::Record(record(
                message_from(AgentRef::User),
                user("org.quire.Shell"),
            )),
            [false, true, false, false],
        ),
        (
            "an episode, by the router",
            MemoryRequest::Record(record(episode_body(), planner())),
            [false, true, false, false],
        ),
        (
            "an episode, by cuad",
            MemoryRequest::Record(record(episode_body(), cua_actor())),
            [false, true, false, false],
        ),
        (
            "an episode, by an app",
            MemoryRequest::Record(record(episode_body(), user(MAIL))),
            [false, true, false, false],
        ),
    ];
    for (name, request, want) in cases {
        for ((who, caller), want) in callers().into_iter().zip(want) {
            assert_eq!(yes(allowed(&caller, &request)), want, "{name} by {who}");
        }
    }
}
