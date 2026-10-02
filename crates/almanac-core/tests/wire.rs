//! The wire and the stored forms: every variant survives serde, tags and JSON are pinned.

mod common;

use almanac_core::*;
use common::*;

fn status() -> SpaceStatus {
    SpaceStatus {
        state: SpaceState::Open,
        index: IndexView::Building {
            done: Count(3),
            total: Count(10),
        },
        chain: ChainHealth::Checked(ChainReport::Intact {
            head: Head {
                seq: Seq(4),
                link: Link32([1; 32]),
            },
            erased: Count(1),
        }),
        usage: Bytes(1024),
        events: Count(4),
        facts: Count(2),
        pending: Count(1),
        last_run: None,
    }
}

fn timeline_entry() -> TimelineEntry {
    TimelineEntry {
        event: event_ref(3),
        occurred: NOW,
        actor: user(),
        kind: KindTag::parse("thing.archived").expect("kind"),
        effect: Effect::UndoableWrite,
        label: user_label(),
        things: vec![view("org.quire.Mail", "mail.thread", "7f3a", "Q4 budget")],
        body: EntryBody::Erased {
            by: EraseCause::Forgotten,
        },
        derived_facts: Count(1),
        undo: UndoRef::Token("t-1".into()),
    }
}

fn fact_view() -> FactView {
    FactView {
        fact: fact(),
        topic: TopicPath::parse("prefs/meetings").expect("topic"),
        state: FactState::Superseded {
            by: FactId::mint(5, [3; 10]),
        },
        sources: vec![
            SourceView::Present(view("org.quire.Mail", "mail.thread", "7f3a", "Q4")),
            SourceView::Event(timeline_entry()),
            SourceView::Purged,
        ],
        used: UseCount(2),
        last_used: Some(NOW),
    }
}

fn requests() -> Vec<MemoryRequest> {
    let w = space("work");
    let topic = TopicPath::parse("prefs/meetings").expect("topic");
    let scope = |s| MemoryRequest::PlanForget(w.clone(), s);
    vec![
        MemoryRequest::Record(record(archived_body(), user())),
        MemoryRequest::RecordBatch(vec![
            record(archived_body(), user()),
            record(file_body(FileWhy::Unexplained), Actor::Unknown),
        ]),
        MemoryRequest::ExplainFile(FileWhyClaim {
            space: w.clone(),
            path: SpacePath::parse("/a/b").expect("p"),
            content: ContentDigest([2; 32]),
            cause: thing("org.quire.Mail", "mail.message", "m1"),
            verb: Verb::Downloaded,
            by: user(),
        }),
        MemoryRequest::Mark(MarkRequest {
            space: w.clone(),
            thing: thing("org.quire.Mail", "mail.thread", "k"),
            mark: MarkKind::DoNotRemember,
        }),
        MemoryRequest::Search(RecallQuery {
            space: w.clone(),
            text: "lisbon".into(),
            limit: Count(10),
            over: RecallOver::Both,
        }),
        MemoryRequest::Facts(FactQuery {
            space: w.clone(),
            topic: Some(topic.clone()),
            about: None,
            state: FactFilter::Active,
            limit: Count(5),
        }),
        MemoryRequest::Related(w.clone(), thing("org.quire.Mail", "mail.thread", "k")),
        MemoryRequest::Provenance(w.clone(), SpacePath::parse("/a/b").expect("p")),
        MemoryRequest::Primer(w.clone()),
        MemoryRequest::Propose(
            w.clone(),
            FactDraft {
                topic,
                text: FactText::parse("x").expect("t"),
                links: vec![],
                supersedes: vec![],
            },
        ),
        MemoryRequest::Spaces,
        MemoryRequest::Status(w.clone()),
        MemoryRequest::Timeline(
            w.clone(),
            TimelineQuery {
                before: Some(Cursor(Seq(9))),
                limit: Count(50),
                filter: TimelineFilter {
                    actors: ActorFilter::You,
                    apps: vec![app("org.quire.Mail")],
                    kinds: vec![KindPattern::any()],
                    trust: TrustFilter::Any,
                    range: Some((UnixSeconds(1), UnixSeconds(2))),
                },
            },
        ),
        scope(ForgetScope::Event(event_ref(1))),
        scope(ForgetScope::Thing(thing(
            "org.quire.Mail",
            "mail.thread",
            "k",
        ))),
        scope(ForgetScope::Fact(fact().id)),
        scope(ForgetScope::Range(UnixSeconds(1), UnixSeconds(2))),
        scope(ForgetScope::App(app("org.quire.Mail"))),
        scope(ForgetScope::Kind(KindPattern::any())),
        scope(ForgetScope::Space),
        MemoryRequest::Forget(PlanToken::parse("p-1").expect("token")),
        MemoryRequest::Pending(w.clone()),
        MemoryRequest::Settle(fact().id, Settlement::Discard),
        MemoryRequest::Settle(
            fact().id,
            Settlement::Keep(ConfirmReceipt {
                id: ConfirmId::parse("c-1").expect("id"),
                input: InputProof::ShellCaller,
                at: NOW,
            }),
        ),
        MemoryRequest::Consolidation(w.clone()),
        MemoryRequest::RunConsolidation(w.clone()),
        MemoryRequest::Revert(RunId::parse("run-1").expect("run")),
        MemoryRequest::Rules,
        MemoryRequest::SetRule(rule(RuleScope::Space(w.clone()), RememberMode::HeaderOnly)),
        MemoryRequest::RemoveRule(RuleId::parse("r-1").expect("id")),
        MemoryRequest::Pause(w.clone(), NOW),
        MemoryRequest::Resume(w.clone()),
        MemoryRequest::Verify(w.clone()),
        MemoryRequest::Rebuild(w.clone()),
        MemoryRequest::Export(ExportOptions {
            spaces: vec![],
            verification_key: VerificationKey::Omit,
        }),
        MemoryRequest::Inject(InjectQuery {
            space: w.clone(),
            text: "lisbon receipts".into(),
            budget: Tokens(1500),
            k: Count(8),
            over: RecallOver::Both,
            trust: TrustFilter::TrustedOnly,
        }),
        MemoryRequest::Recent(
            w,
            RecentQuery {
                since: NOW,
                kinds: vec![KindPattern::parse("companion.*").expect("pattern")],
                trust: TrustFilter::Any,
                limit: Count(20),
                bodies: BodyMode::Without,
            },
        ),
    ]
}

/// An exhaustive match: adding a variant breaks this until the sample table covers it.
fn request_index(r: &MemoryRequest) -> usize {
    match r {
        MemoryRequest::Record(_) => 0,
        MemoryRequest::RecordBatch(_) => 1,
        MemoryRequest::ExplainFile(_) => 2,
        MemoryRequest::Mark(_) => 3,
        MemoryRequest::Search(_) => 4,
        MemoryRequest::Facts(_) => 5,
        MemoryRequest::Related(..) => 6,
        MemoryRequest::Provenance(..) => 7,
        MemoryRequest::Primer(_) => 8,
        MemoryRequest::Propose(..) => 9,
        MemoryRequest::Spaces => 10,
        MemoryRequest::Status(_) => 11,
        MemoryRequest::Timeline(..) => 12,
        MemoryRequest::PlanForget(..) => 13,
        MemoryRequest::Forget(_) => 14,
        MemoryRequest::Pending(_) => 15,
        MemoryRequest::Settle(..) => 16,
        MemoryRequest::Consolidation(_) => 17,
        MemoryRequest::RunConsolidation(_) => 18,
        MemoryRequest::Revert(_) => 19,
        MemoryRequest::Rules => 20,
        MemoryRequest::SetRule(_) => 21,
        MemoryRequest::RemoveRule(_) => 22,
        MemoryRequest::Pause(..) => 23,
        MemoryRequest::Resume(_) => 24,
        MemoryRequest::Verify(_) => 25,
        MemoryRequest::Rebuild(_) => 26,
        MemoryRequest::Export(_) => 27,
        MemoryRequest::Inject(_) => 28,
        MemoryRequest::Recent(..) => 29,
    }
}

#[test]
fn wire_round_trip_every_request() {
    let all = requests();
    round_trips(&all);
    let covered: std::collections::BTreeSet<usize> = all.iter().map(request_index).collect();
    assert_eq!(
        covered,
        (0..=29).collect(),
        "a request variant has no sample"
    );
}

fn replies() -> Vec<MemoryReply> {
    let hit = RecallHit {
        doc: MemoryItem::Fact(fact().id),
        text: "x".into(),
        at: NOW,
        label: user_label(),
        links: vec![Link::Run(RunId::parse("run-1").expect("run"))],
        why: RecallWhy::Both {
            lexical: 1,
            semantic: 2,
        },
    };
    let counts = ForgetCounts {
        events: Count(1),
        facts: Count(2),
        pending: Count(0),
        procedures: Count(0),
        index_docs: Count(3),
    };
    vec![
        MemoryReply::Recorded(event_ref(1)),
        MemoryReply::RecordedBatch(event_ref(1), Count(2)),
        MemoryReply::Ok,
        MemoryReply::Hits(vec![hit]),
        MemoryReply::Recent(vec![RecentEntry {
            summary: EventSummary {
                event: event_ref(5),
                occurred: NOW,
                kind: KindTag::parse("companion.episode").expect("k"),
                actor: companion(),
                things: vec![],
            },
            effect: Effect::Read,
            label: user_label(),
            text: Some("asked: archive the Lisbon receipts".into()),
            body: Some(JsonText::parse(r#"{"asked":"archive"}"#).expect("json")),
        }]),
        MemoryReply::Facts(vec![fact_view()]),
        MemoryReply::Related(vec![EventSummary {
            event: event_ref(2),
            occurred: NOW,
            kind: KindTag::parse("file.created").expect("k"),
            actor: Actor::Unknown,
            things: vec![],
        }]),
        MemoryReply::Provenance(FileProvenance {
            path: SpacePath::parse("/a").expect("p"),
            history: vec![FileHistoryEntry {
                event: event_ref(2),
                occurred: NOW,
                change: FileChange::Renamed {
                    from: SpacePath::parse("/b").expect("p"),
                },
                why: FileWhy::Unexplained,
            }],
        }),
        MemoryReply::Primer("# primer".into()),
        MemoryReply::Proposed(fact().id, FactState::Pending),
        MemoryReply::Spaces(vec![SpaceSummary {
            id: space("work"),
            state: SpaceState::Locked,
            vault: VaultKind::Sealed,
            created: NOW,
        }]),
        MemoryReply::Status(status()),
        MemoryReply::Timeline(TimelinePage {
            entries: vec![timeline_entry()],
            next: Some(Cursor(Seq(2))),
        }),
        MemoryReply::Plan(ForgetPlanView {
            token: PlanToken::parse("p-1").expect("t"),
            expires: NOW,
            events: Count(1),
            facts: vec![fact_view()],
            procedures: Count(0),
            index_docs: Count(2),
            pending: Count(0),
        }),
        MemoryReply::Forgot(ForgetReport {
            plan: PlanDigest([5; 32]),
            counts,
        }),
        MemoryReply::Pending(vec![]),
        MemoryReply::Consolidation(DraftView {
            run: RunId::parse("run-1").expect("r"),
            state: RunState::Failed(ConsolidateFailure::Unparseable),
            hunks: vec![
                Hunk::Tidy(TidyHunk {
                    topic: TopicPath::parse("a").expect("t"),
                    before: "x".into(),
                    after: "y".into(),
                }),
                Hunk::Promote {
                    fact: fact(),
                    to: Lands::Pending,
                },
                Hunk::Supersede {
                    old: fact().id,
                    new: fact(),
                },
                Hunk::Flag {
                    facts: vec![fact().id],
                    note: "stale".into(),
                },
                Hunk::ExternalEdit {
                    topic: TopicPath::parse("a").expect("t"),
                    before: "x".into(),
                    after: "y".into(),
                },
                Hunk::Stamp {
                    topic: TopicPath::parse("a").expect("t"),
                    text: "z".into(),
                },
            ],
        }),
        MemoryReply::Rules(RuleSet::standard()),
        MemoryReply::Verified(ChainReport::Broken {
            at: Seq(3),
            why: Break::Gap,
        }),
        MemoryReply::Exported(ExportManifest {
            format: EXPORT_FORMAT.into(),
            created: NOW,
            spaces: vec![ExportedSpace {
                space: space("work"),
                head: None,
                counts: ExportCounts {
                    events: Count(1),
                    facts: Count(1),
                    pending: Count(0),
                    procedures: Count(0),
                },
            }],
            counts: ExportCounts {
                events: Count(1),
                facts: Count(1),
                pending: Count(0),
                procedures: Count(0),
            },
        }),
        MemoryReply::Refused(Refusal::Invalid("nope".into())),
    ]
}

fn reply_index(r: &MemoryReply) -> usize {
    match r {
        MemoryReply::Recorded(_) => 0,
        MemoryReply::RecordedBatch(..) => 1,
        MemoryReply::Ok => 2,
        MemoryReply::Hits(_) => 3,
        MemoryReply::Facts(_) => 4,
        MemoryReply::Related(_) => 5,
        MemoryReply::Provenance(_) => 6,
        MemoryReply::Primer(_) => 7,
        MemoryReply::Proposed(..) => 8,
        MemoryReply::Spaces(_) => 9,
        MemoryReply::Status(_) => 10,
        MemoryReply::Timeline(_) => 11,
        MemoryReply::Plan(_) => 12,
        MemoryReply::Forgot(_) => 13,
        MemoryReply::Pending(_) => 14,
        MemoryReply::Consolidation(_) => 15,
        MemoryReply::Rules(_) => 16,
        MemoryReply::Verified(_) => 17,
        MemoryReply::Exported(_) => 18,
        MemoryReply::Refused(_) => 19,
        MemoryReply::Recent(_) => 20,
    }
}

#[test]
fn wire_round_trip_every_reply() {
    let all = replies();
    round_trips(&all);
    let covered: std::collections::BTreeSet<usize> = all.iter().map(reply_index).collect();
    assert_eq!(covered, (0..=20).collect(), "a reply variant has no sample");
}

#[test]
fn requests_name_their_space() {
    for request in requests() {
        let named = request.space().is_some();
        let by_index = !matches!(
            request_index(&request),
            1 | 10 | 14 | 16 | 19 | 20 | 21 | 22 | 27
        );
        assert_eq!(named, by_index, "{request:?}");
    }
}
