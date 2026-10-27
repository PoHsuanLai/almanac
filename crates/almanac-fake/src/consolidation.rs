//! A consolidation input to build prompts and drafts from: Ana's fact, two events (one trusted,
//! one from mail) and her topic file, in the work Space.

use crate::fixtures::{mail_label, trusted_label};
use almanac_core::*;
use almanac_service::{ConsolidationInput, InputEvent, InputTopic};
use memfiles_format::{Block, TopicFile, render_topic};

fn run_id() -> RunId {
    RunId::parse("c-0123456789abcdef0123").expect("run")
}

/// The event `seq` of the work Space's one replica.
pub fn consolidation_event(seq: u64) -> EventRef {
    EventRef {
        space: SpaceId::parse("work").expect("space"),
        replica: ReplicaId([1; 16]),
        seq: Seq(seq),
    }
}

fn ana_fact() -> Fact {
    Fact {
        id: FactId::mint(1, [1; 10]),
        text: FactText::parse("Ana is the CFO.").expect("text"),
        recorded: UnixSeconds(1_790_000_000),
        by: Actor::Unknown,
        label: trusted_label(),
        links: vec![],
        supersedes: vec![],
        valid: Validity::Unstated,
    }
}

/// The input of a consolidation run: Ana's fact, events 1 (trusted) and 2 (mail), and the
/// `people/ana` topic file.
pub fn consolidation_input() -> ConsolidationInput {
    let event = |seq| InputEvent {
        event: consolidation_event(seq),
        kind: KindTag::parse("thing.archived").expect("kind"),
        things: vec![],
        label: if seq == 1 {
            trusted_label()
        } else {
            mail_label()
        },
    };
    ConsolidationInput {
        space: SpaceId::parse("work").expect("space"),
        run: run_id(),
        now: UnixSeconds(1_790_000_100),
        facts: vec![ana_fact()],
        events: vec![event(1), event(2)],
        topics: vec![InputTopic {
            topic: TopicPath::parse("people/ana").expect("topic"),
            text: UserText::new(ana_topic_text()),
        }],
    }
}

/// `people/ana` as the service reads it back: Ana's fact with its trailer, and a bullet of the
/// person's own.
pub fn ana_topic_text() -> String {
    let file = TopicFile {
        topic: TopicPath::parse("people/ana").expect("topic"),
        title: "ana".to_owned(),
        blocks: vec![
            Block::Fact(ana_fact()),
            Block::Unstamped("Ana likes tea".to_owned()),
        ],
    };
    render_topic(&file, &chrono::Offset::fix(&chrono::Utc))
}

/// The JSON of a link to event `seq`, as a model cites it.
pub fn consolidation_event_link(seq: u64) -> String {
    serde_json::to_string(&Link::Event(consolidation_event(seq))).expect("json")
}
