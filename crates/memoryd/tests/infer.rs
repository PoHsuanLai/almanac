//! The inferd-backed embedder and consolidator, over a scripted transport: what they ask for,
//! what they make of every answer, and the pure prompt and parse.

use almanac_core::*;
use almanac_fake::{mail_label, trusted_label};
use almanac_service::{
    ConsolidateError, ConsolidationInput, Consolidator, InputEvent, InputTopic, check_draft,
};
use memfiles::{Block, TopicFile, render_topic};
use memoryd::{InferdConsolidator, InferdEmbedder, class_of, parse_draft, render_prompt};
use porter_client::{Transport, TransportError};
use porter_core::capability::{LlmFeature, Modality};
use porter_core::consent::Usage;
use porter_core::need::DimsNeed;
use porter_core::{AccountId, AccountsReply, AccountsRequest, Dims, Locality, ModelId, Need, Tier};
use porter_infer::{
    ChatReply, ClientFrame, EmbedReply, EmbedVector, InferEvent, InferRefusal, InferReply,
    InferRequest, InferSession, ModelError, OpenOptions, ServedBy, SessionError, StopReason, Task,
    TokenUsage,
};
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, FakeEmbedder, RetryClass, Urgency};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
struct Opened {
    need: Need,
    class: DataClass,
    tier: Tier,
}

/// A transport whose one session plays `events`, and which remembers what was opened and sent.
#[derive(Debug, Default)]
struct Scripted {
    events: Mutex<VecDeque<InferEvent>>,
    opened: Mutex<Vec<Opened>>,
    sent: Arc<Mutex<Vec<ClientFrame>>>,
    unreachable: bool,
    /// inferd refuses the caller, with this text.
    denied: Option<String>,
}

impl Scripted {
    fn playing(events: Vec<InferEvent>) -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(events.into()),
            ..Self::default()
        })
    }

    fn down() -> Arc<Self> {
        Arc::new(Self {
            unreachable: true,
            ..Self::default()
        })
    }

    fn denying(why: &str) -> Arc<Self> {
        Arc::new(Self {
            denied: Some(why.to_owned()),
            ..Self::default()
        })
    }

    fn finishing(reply: InferReply) -> Arc<Self> {
        Self::playing(vec![InferEvent::Finished(reply)])
    }

    fn opened(&self) -> Vec<Opened> {
        self.opened.lock().expect("lock").clone()
    }

    fn sent(&self) -> Vec<ClientFrame> {
        self.sent.lock().expect("lock").clone()
    }
}

struct Session {
    events: VecDeque<InferEvent>,
    sent: Arc<Mutex<Vec<ClientFrame>>>,
}

impl InferSession for Session {
    async fn send(&mut self, frame: ClientFrame) -> Result<(), SessionError> {
        self.sent.lock().expect("lock").push(frame);
        Ok(())
    }

    async fn next(&mut self) -> Result<InferEvent, SessionError> {
        self.events.pop_front().ok_or(SessionError::Closed)
    }
}

impl Transport for Scripted {
    type Session = Session;

    async fn call(&self, _request: AccountsRequest) -> Result<AccountsReply, TransportError> {
        Err(TransportError::Unreachable)
    }

    async fn open_with(
        &self,
        need: &Need,
        class: DataClass,
        tier: Tier,
        _options: &OpenOptions,
    ) -> Result<Session, TransportError> {
        if self.unreachable {
            return Err(TransportError::Unreachable);
        }
        if let Some(why) = &self.denied {
            return Err(TransportError::Denied(why.clone()));
        }
        self.opened.lock().expect("lock").push(Opened {
            need: need.clone(),
            class,
            tier,
        });
        Ok(Session {
            events: std::mem::take(&mut *self.events.lock().expect("lock")),
            sent: self.sent.clone(),
        })
    }
}

fn served() -> ServedBy {
    ServedBy {
        account: AccountId::parse("local").expect("id"),
        model: ModelId::parse("nomic").expect("id"),
        locality: Locality::OnDevice,
    }
}

fn usage() -> TokenUsage {
    TokenUsage {
        input: porter_core::Tokens(3),
        output: porter_core::Tokens(0),
        cached: porter_core::Tokens(0),
    }
}

fn embed_reply(vectors: Vec<Vec<f32>>) -> InferReply {
    InferReply::Embed(EmbedReply {
        vectors: vectors.into_iter().map(EmbedVector).collect(),
        usage: usage(),
        served: served(),
    })
}

fn card() -> EmbedderCard {
    FakeEmbedder::new().card().clone()
}

fn texts(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("text {i}")).collect()
}

fn vector_of(text: &str) -> Vec<f32> {
    FakeEmbedder::vector(text).0
}

#[tokio::test]
async fn the_embedder_asks_for_the_cards_model_and_returns_one_vector_per_text() {
    let want = vec![vector_of("alpha"), vector_of("beta")];
    let transport = Scripted::playing(vec![
        InferEvent::Routed(served()),
        InferEvent::Waiting(porter_infer::Readiness::Loading),
        InferEvent::Finished(embed_reply(want.clone())),
    ]);
    let embedder = InferdEmbedder::new(transport.clone(), card());
    let got = embedder
        .embed(&texts(2), EmbedRole::Document, Urgency::Background)
        .await
        .expect("vectors");
    assert_eq!(got.iter().map(|v| v.0.clone()).collect::<Vec<_>>(), want);

    let opened = transport.opened();
    assert_eq!(opened.len(), 1);
    let Need::Embeddings(need) = &opened[0].need else {
        panic!("{:?}", opened[0].need)
    };
    assert_eq!(need.dims, DimsNeed::Exactly(Dims(card().dims)));
    assert!(need.modalities.contains(&Modality::Text));
    assert_eq!(
        (opened[0].class, opened[0].tier),
        (DataClass::Mail, Tier::Fast)
    );
    let sent = transport.sent();
    let [ClientFrame::Request(InferRequest::Embed(request))] = sent.as_slice() else {
        panic!("{:?}", transport.sent())
    };
    assert_eq!(request.inputs, texts(2));
    assert_eq!(request.role, porter_infer::EmbedRole::Document);
    assert_eq!(request.usage, Usage::Background);
    assert_eq!(request.class, DataClass::Mail);
}

#[tokio::test]
async fn a_query_is_interactive_and_the_class_is_the_embedders() {
    let transport = Scripted::finishing(embed_reply(vec![vector_of("q")]));
    let embedder = InferdEmbedder::for_class(transport.clone(), card(), DataClass::Notes);
    embedder
        .embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
        .await
        .expect("vectors");
    let sent = transport.sent();
    let [ClientFrame::Request(InferRequest::Embed(request))] = sent.as_slice() else {
        panic!("one embed request")
    };
    assert_eq!(request.role, porter_infer::EmbedRole::Query);
    assert_eq!(request.usage, Usage::Interactive);
    assert_eq!(request.class, DataClass::Notes);
}

#[tokio::test]
async fn nothing_to_embed_opens_nothing() {
    let transport = Scripted::playing(vec![]);
    let embedder = InferdEmbedder::new(transport.clone(), card());
    let none = embedder
        .embed(&[], EmbedRole::Document, Urgency::Background)
        .await;
    assert_eq!(none, Ok(vec![]));
    assert!(transport.opened().is_empty());
}

fn failed(class: RetryClass) -> impl Fn(&EmbedError) -> bool {
    move |e| matches!(e, EmbedError::Failed { class: c, .. } if *c == class)
}

async fn embed_error(reply: InferReply, count: usize) -> EmbedError {
    let embedder = InferdEmbedder::new(Scripted::finishing(reply), card());
    embedder
        .embed(&texts(count), EmbedRole::Document, Urgency::Background)
        .await
        .expect_err("an error")
}

#[tokio::test]
async fn the_embedder_maps_every_refusal_and_failure() {
    let refusals = [
        (InferRefusal::Unavailable, EmbedError::Unavailable),
        (
            InferRefusal::RequiresCloud(DataClass::Mail),
            EmbedError::Refused(InferRefusal::RequiresCloud(DataClass::Mail).to_string()),
        ),
        (
            InferRefusal::NeedsGrant,
            EmbedError::Refused(InferRefusal::NeedsGrant.to_string()),
        ),
        (
            InferRefusal::Denied,
            EmbedError::Refused(InferRefusal::Denied.to_string()),
        ),
        (
            InferRefusal::OverBudget,
            EmbedError::Refused(InferRefusal::OverBudget.to_string()),
        ),
        (
            InferRefusal::Unsupported,
            EmbedError::Refused(InferRefusal::Unsupported.to_string()),
        ),
    ];
    for (refusal, expected) in refusals {
        assert_eq!(embed_error(InferReply::Refused(refusal), 1).await, expected);
    }
    let failures = [
        (ModelError::Unreachable, EmbedError::Unavailable),
        (ModelError::NotReady, EmbedError::Unavailable),
        (ModelError::RateLimited(5), EmbedError::Busy),
        (ModelError::ContextOverflow, EmbedError::TooLong),
        (
            ModelError::Unauthorized,
            EmbedError::Refused(ModelError::Unauthorized.to_string()),
        ),
        (
            ModelError::Refused,
            EmbedError::Refused(ModelError::Refused.to_string()),
        ),
    ];
    for (failure, expected) in failures {
        assert_eq!(embed_error(InferReply::Failed(failure), 1).await, expected);
    }
    for transient in [ModelError::Unreadable, ModelError::Unparseable] {
        let error = embed_error(InferReply::Failed(transient), 1).await;
        assert!(failed(RetryClass::Retry)(&error), "{error:?}");
    }
    let cancelled = embed_error(InferReply::Cancelled, 1).await;
    assert!(failed(RetryClass::Retry)(&cancelled));
    // Retry classes follow the error: refusals are fatal, unavailable and busy are not.
    assert_eq!(EmbedError::Unavailable.retry_class(), RetryClass::Retry);
    assert_eq!(EmbedError::Busy.retry_class(), RetryClass::Retry);
    assert_eq!(
        EmbedError::Refused("x".into()).retry_class(),
        RetryClass::Fatal
    );
}

#[tokio::test]
async fn a_reply_that_does_not_fit_the_index_is_fatal() {
    let wrong_count = embed_error(embed_reply(vec![vector_of("a")]), 2).await;
    assert!(failed(RetryClass::Fatal)(&wrong_count), "{wrong_count:?}");
    let wrong_dims = embed_error(embed_reply(vec![vec![0.5; 7]]), 1).await;
    assert!(failed(RetryClass::Fatal)(&wrong_dims), "{wrong_dims:?}");
    let wrong_kind = embed_error(
        InferReply::Chat(ChatReply {
            text: "hello".into(),
            tool_calls: vec![],
            stop: StopReason::EndTurn,
            thought: None,
            usage: usage(),
            served: served(),
        }),
        1,
    )
    .await;
    assert!(failed(RetryClass::Fatal)(&wrong_kind));
}

#[tokio::test]
async fn an_absent_daemon_or_a_closed_session_is_unavailable() {
    let down = InferdEmbedder::new(Scripted::down(), card());
    assert_eq!(
        down.embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
            .await,
        Err(EmbedError::Unavailable)
    );
    // The session ends before the turn does.
    let closed = InferdEmbedder::new(Scripted::playing(vec![]), card());
    assert_eq!(
        closed
            .embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
            .await,
        Err(EmbedError::Unavailable)
    );
}

#[tokio::test]
async fn a_caller_inferd_refuses_is_a_fatal_failure_with_the_daemons_text() {
    let denied = InferdEmbedder::new(
        Scripted::denying("inferd: the caller is not in its caller table"),
        card(),
    );
    let got = denied
        .embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
        .await;
    assert_eq!(
        got,
        Err(EmbedError::Failed {
            class: RetryClass::Fatal,
            why: "inferd: the caller is not in its caller table".to_owned()
        })
    );
}

// --- the consolidator -------------------------------------------------------------------------

fn run_id() -> RunId {
    RunId::parse("c-0123456789abcdef0123").expect("run")
}

fn event(seq: u64) -> EventRef {
    EventRef {
        space: SpaceId::parse("work").expect("space"),
        replica: ReplicaId([1; 16]),
        seq: Seq(seq),
    }
}

fn fact(n: u8, text: &str, label: Label) -> Fact {
    Fact {
        id: FactId::mint(u64::from(n), [n; 10]),
        text: FactText::parse(text).expect("text"),
        recorded: UnixSeconds(1_790_000_000),
        by: Actor::Unknown,
        label,
        links: vec![],
        supersedes: vec![],
        valid: Validity::Unstated,
    }
}

fn input() -> ConsolidationInput {
    ConsolidationInput {
        space: SpaceId::parse("work").expect("space"),
        run: run_id(),
        now: UnixSeconds(1_790_000_100),
        facts: vec![fact(1, "Ana is the CFO.", trusted_label())],
        events: vec![
            InputEvent {
                event: event(1),
                kind: KindTag::parse("thing.archived").expect("kind"),
                things: vec![],
                label: trusted_label(),
            },
            InputEvent {
                event: event(2),
                kind: KindTag::parse("thing.archived").expect("kind"),
                things: vec![],
                label: mail_label(),
            },
        ],
        topics: vec![InputTopic {
            topic: TopicPath::parse("people/ana").expect("topic"),
            text: UserText::new(topic_text()),
        }],
    }
}

/// `people/ana` as the service reads it back: Ana's fact with its trailer, and a bullet of the
/// person's own.
fn topic_text() -> String {
    let file = TopicFile {
        topic: TopicPath::parse("people/ana").expect("topic"),
        title: "ana".to_owned(),
        blocks: vec![
            Block::Fact(fact(1, "Ana is the CFO.", trusted_label())),
            Block::Unstamped("Ana likes tea".to_owned()),
        ],
    };
    render_topic(&file, &jiff::tz::TimeZone::UTC)
}

fn event_link(seq: u64) -> String {
    serde_json::to_string(&Link::Event(event(seq))).expect("json")
}

fn chat(text: &str) -> InferReply {
    InferReply::Chat(ChatReply {
        text: text.to_owned(),
        tool_calls: vec![],
        stop: StopReason::EndTurn,
        thought: None,
        usage: usage(),
        served: served(),
    })
}

#[test]
fn the_prompt_lists_the_facts_and_the_events_and_says_what_to_answer() {
    let prompt = render_prompt(&input());
    assert!(prompt.contains("ONE JSON object"));
    assert!(prompt.contains("\"kind\":\"promote\""));
    assert!(prompt.contains(&format!("\"id\":\"{}\"", fact_id())));
    assert!(prompt.contains("Ana is the CFO."));
    let listed: Vec<serde_json::Value> = prompt
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let wanted = serde_json::to_value(event(1)).expect("json");
    assert!(
        listed.iter().any(|l| l.get("event") == Some(&wanted)),
        "an event line carries its reference exactly as a link needs it: {listed:?}"
    );
    assert!(prompt.contains("thing.archived"));
}

#[test]
fn the_prompt_shows_the_topic_files_and_how_to_tidy_one() {
    let prompt = render_prompt(&input());
    assert!(prompt.contains("\"kind\":\"tidy\""));
    let shown: Vec<serde_json::Value> = prompt
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .filter(|l: &serde_json::Value| l.get("topic").is_some() && l.get("text").is_some())
        .collect();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0]["topic"], "people/ana");
    assert_eq!(shown[0]["text"].as_str(), Some(topic_text().as_str()));
}

fn tidy_answer(topic: &str, after: &str) -> String {
    serde_json::json!({"hunks": [{"kind": "tidy", "topic": topic, "after": after}]}).to_string()
}

#[test]
fn a_tidy_of_a_shown_file_becomes_a_hunk_with_the_text_the_model_saw() {
    let reworded = topic_text().replace("Ana is the CFO.", "Ana is our CFO.");
    let input = input();
    let draft = parse_draft(&tidy_answer("people/ana", &reworded), &input).expect("draft");
    let [Hunk::Tidy(tidy)] = draft.hunks.as_slice() else {
        panic!("{:?}", draft.hunks)
    };
    assert_eq!(tidy.topic, TopicPath::parse("people/ana").expect("topic"));
    assert_eq!(tidy.before.as_str(), topic_text());
    assert_eq!(tidy.after.as_str(), reworded);
    let checked = check_draft(&input, draft);
    assert!(checked.dropped.is_empty(), "{:?}", checked.dropped);
}

#[test]
fn a_tidy_that_is_not_one_is_dropped() {
    let input = input();
    let text = topic_text();
    let cases = [
        ("an unchanged file", "people/ana", text.clone()),
        (
            "a topic it was not shown",
            "people/bo",
            text.replace("CFO", "COO"),
        ),
        (
            "a topic that is not a path",
            "../x",
            text.replace("CFO", "COO"),
        ),
        (
            "a file that lost its fact",
            "people/ana",
            text.lines()
                .filter(|l| !l.contains("fact:") && !l.contains("CFO"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        ("not a topic file", "people/ana", "just words".to_owned()),
    ];
    for (name, topic, after) in cases {
        let draft = parse_draft(&tidy_answer(topic, &after), &input).expect("draft");
        assert!(draft.hunks.is_empty(), "{name}: {:?}", draft.hunks);
    }
}

fn fact_id() -> FactId {
    FactId::mint(1, [1; 10])
}

#[test]
fn a_promotion_cites_events_and_takes_the_join_of_their_labels() {
    let answer = format!(
        r#"{{"hunks":[{{"kind":"promote","text":"Ana sent the Lisbon budget.","from":[{},{}]}}]}}"#,
        event_link(1),
        event_link(2)
    );
    let input = input();
    let draft = parse_draft(&answer, &input).expect("draft");
    let [Hunk::Promote { fact, to }] = draft.hunks.as_slice() else {
        panic!("{:?}", draft.hunks)
    };
    assert_eq!(fact.text.as_str(), "Ana sent the Lisbon budget.");
    assert_eq!(fact.recorded, input.now);
    assert_eq!(
        fact.by,
        Actor::System {
            part: SystemPart::Memory
        }
    );
    assert_eq!(
        fact.label.integrity,
        Integrity::Untrusted,
        "the stranger's mail taints it"
    );
    assert_eq!(*to, Lands::Pending, "so it waits for the person");
    assert_eq!(
        fact.links,
        vec![Link::Event(event(1)), Link::Event(event(2))]
    );
    let checked = check_draft(&input, draft);
    assert!(checked.dropped.is_empty(), "{:?}", checked.dropped);
}

#[test]
fn a_trusted_promotion_lands_active_and_ids_are_deterministic() {
    let answer = format!(
        r#"```json
{{"hunks":[{{"kind":"promote","text":"Ana archived the thread.","from":[{}]}}]}}
```"#,
        event_link(1)
    );
    let a = parse_draft(&answer, &input()).expect("draft");
    let b = parse_draft(&answer, &input()).expect("draft");
    assert_eq!(a, b, "the same run and text give the same fact");
    let [Hunk::Promote { to, fact }] = a.hunks.as_slice() else {
        panic!("{:?}", a.hunks)
    };
    assert_eq!(*to, Lands::Active);
    let other = answer.replace("archived the thread", "archived another one");
    let c = parse_draft(&other, &input()).expect("draft");
    let [
        Hunk::Promote {
            fact: other_fact, ..
        },
    ] = c.hunks.as_slice()
    else {
        panic!("{:?}", c.hunks)
    };
    assert_ne!(fact.id, other_fact.id);
}

#[test]
fn a_supersede_and_a_flag_are_built_and_pass_the_check() {
    let answer = format!(
        r#"{{"hunks":[
          {{"kind":"supersede","old":"{old}","text":"Ana is the CEO now.","from":[{from}]}},
          {{"kind":"flag","facts":["{old}"],"note":"the title changed"}}
        ]}}"#,
        old = fact_id(),
        from = event_link(1)
    );
    let input = input();
    let draft = parse_draft(&answer, &input).expect("draft");
    assert_eq!(draft.hunks.len(), 2);
    assert!(
        matches!(&draft.hunks[0], Hunk::Supersede { old, new } if *old == fact_id() && new.supersedes == vec![fact_id()])
    );
    assert!(matches!(&draft.hunks[1], Hunk::Flag { facts, .. } if facts == &vec![fact_id()]));
    assert!(check_draft(&input, draft).dropped.is_empty());
}

#[test]
fn hunks_that_cannot_be_built_are_dropped_one_by_one() {
    let bad_cite = serde_json::to_string(&Link::Event(event(99))).expect("json");
    let answer = format!(
        r#"{{"hunks":[
          {{"kind":"promote","text":"cites nothing","from":[]}},
          {{"kind":"promote","text":"cites a stranger","from":[{bad_cite}]}},
          {{"kind":"promote","text":"","from":[{good}]}},
          {{"kind":"obliterate","everything":true}},
          {{"kind":"flag","facts":[],"note":"nothing flagged"}},
          {{"kind":"promote","text":"kept","from":[{good}]}}
        ]}}"#,
        good = event_link(1)
    );
    let draft = parse_draft(&answer, &input()).expect("draft");
    assert_eq!(draft.hunks.len(), 1, "{:?}", draft.hunks);
    assert!(matches!(&draft.hunks[0], Hunk::Promote { fact, .. } if fact.text.as_str() == "kept"));
}

#[test]
fn an_answer_that_is_not_the_object_is_unparseable() {
    for answer in [
        "",
        "I would promote nothing.",
        "{\"hunks\": 3}",
        "{\"nothing\":[]}",
        "}{",
    ] {
        assert_eq!(
            parse_draft(answer, &input()),
            Err(ConsolidateError::Unparseable),
            "{answer:?}"
        );
    }
    assert_eq!(
        parse_draft("{\"hunks\":[]}", &input()),
        Ok(almanac_service::Draft { hunks: vec![] })
    );
}

#[test]
fn at_most_sixty_four_hunks_are_read() {
    let flag = format!(r#"{{"kind":"flag","facts":["{}"],"note":"x"}}"#, fact_id());
    let answer = format!("{{\"hunks\":[{}]}}", vec![flag; 80].join(","));
    assert_eq!(
        parse_draft(&answer, &input()).expect("draft").hunks.len(),
        64
    );
}

#[test]
fn the_class_of_a_request_is_the_most_sensitive_of_its_labels() {
    let with = |classes: &[DataClass]| {
        let mut label = trusted_label();
        label.classes = classes.iter().copied().collect();
        label
    };
    let table: [(Vec<Label>, DataClass); 7] = [
        (vec![], DataClass::AppOwn),
        (vec![with(&[])], DataClass::AppOwn),
        (vec![with(&[DataClass::Public])], DataClass::Public),
        (
            vec![with(&[DataClass::Public]), with(&[DataClass::Mail])],
            DataClass::Mail,
        ),
        (
            vec![with(&[DataClass::Notes, DataClass::Files])],
            DataClass::Notes,
        ),
        (
            vec![with(&[DataClass::Mail]), with(&[DataClass::Voice])],
            DataClass::Voice,
        ),
        (
            vec![with(&[DataClass::Mail, DataClass::Prompt])],
            DataClass::Prompt,
        ),
    ];
    for (labels, expected) in table {
        assert_eq!(class_of(&labels), expected, "{labels:?}");
    }
}

#[tokio::test]
async fn the_consolidator_sends_the_prompt_as_a_background_extract_task() {
    let answer = format!(
        r#"{{"hunks":[{{"kind":"promote","text":"Ana archived it.","from":[{}]}}]}}"#,
        event_link(1)
    );
    let transport = Scripted::finishing(chat(&answer));
    let consolidator = InferdConsolidator::new(transport.clone());
    let draft = consolidator.draft(input()).await.expect("draft");
    assert_eq!(draft.hunks.len(), 1);

    let opened = transport.opened();
    let Need::Llm(need) = &opened[0].need else {
        panic!("{:?}", opened[0].need)
    };
    assert!(need.features.contains(&LlmFeature::Chat));
    assert_eq!(
        (opened[0].class, opened[0].tier),
        (DataClass::Mail, Tier::Balanced),
        "the stranger's mail label carries its class"
    );
    let sent = transport.sent();
    let [ClientFrame::Request(InferRequest::Task(task))] = sent.as_slice() else {
        panic!("one task request")
    };
    assert_eq!(task.task, Task::Extract);
    assert_eq!(task.usage, Usage::Background);
    assert_eq!(task.class, DataClass::Mail);
    assert_eq!(task.input, render_prompt(&input()));
}

async fn consolidation_error(reply: InferReply) -> ConsolidateError {
    InferdConsolidator::new(Scripted::finishing(reply))
        .draft(input())
        .await
        .expect_err("an error")
}

#[tokio::test]
async fn the_consolidator_maps_every_refusal_and_failure() {
    use ConsolidateError::{Busy, Unavailable, Unparseable};
    for (refusal, expected) in [
        (InferRefusal::Unavailable, Unavailable),
        (InferRefusal::RequiresCloud(DataClass::Mail), Unavailable),
        (InferRefusal::NeedsGrant, Unavailable),
        (InferRefusal::Denied, Unavailable),
        (InferRefusal::Unsupported, Unavailable),
        (InferRefusal::OverBudget, Busy),
    ] {
        assert_eq!(
            consolidation_error(InferReply::Refused(refusal)).await,
            expected
        );
    }
    for (failure, expected) in [
        (ModelError::RateLimited(3), Busy),
        (ModelError::Unreadable, Unparseable),
        (ModelError::Unparseable, Unparseable),
        (ModelError::ContextOverflow, Unparseable),
        (ModelError::Unreachable, Unavailable),
        (ModelError::Unauthorized, Unavailable),
        (ModelError::Refused, Unavailable),
        (ModelError::NotReady, Unavailable),
    ] {
        assert_eq!(
            consolidation_error(InferReply::Failed(failure)).await,
            expected
        );
    }
    assert_eq!(consolidation_error(InferReply::Cancelled).await, Busy);
    assert_eq!(
        consolidation_error(chat("no json here")).await,
        Unparseable,
        "an answer that is not the object"
    );
    let down = InferdConsolidator::new(Scripted::down());
    assert_eq!(down.draft(input()).await, Err(Unavailable));
}

#[test]
fn a_class_tag_is_the_slug_and_every_class_round_trips() {
    use almanac_service::{class_from_tag, class_tag};
    // The exhaustive match makes a new class a compile error here, so it gets a row.
    let slug = |class: DataClass| match class {
        DataClass::AppOwn => "app_own",
        DataClass::Mail => "mail",
        DataClass::Calendar => "calendar",
        DataClass::Contacts => "contacts",
        DataClass::Notes => "notes",
        DataClass::Files => "files",
        DataClass::Photos => "photos",
        DataClass::Clipboard => "clipboard",
        DataClass::Screen => "screen",
        DataClass::Voice => "voice",
        DataClass::Prompt => "prompt",
        DataClass::Public => "public",
    };
    let all = [
        DataClass::AppOwn,
        DataClass::Mail,
        DataClass::Calendar,
        DataClass::Contacts,
        DataClass::Notes,
        DataClass::Files,
        DataClass::Photos,
        DataClass::Clipboard,
        DataClass::Screen,
        DataClass::Voice,
        DataClass::Prompt,
        DataClass::Public,
    ];
    for class in all {
        let tag = class_tag(class);
        assert_eq!(tag.0, slug(class));
        assert_eq!(class_from_tag(&tag), Some(class));
    }
    assert_eq!(class_from_tag(&recall::ClassTag::default()), None);
    assert_eq!(class_from_tag(&recall::ClassTag("later".into())), None);
}
