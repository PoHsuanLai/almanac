//! What a reply to a request changed: one event, a status or a count to look up, nothing for a
//! refusal.

use crate::support::space;
use almanac_core::*;
use almanac_tidy::{Change, changes};

#[test]
fn what_a_reply_changed() {
    let record = almanac_fake::mail_thread_archived().expect("fixture");
    let event = EventRef {
        space: space("work"),
        replica: ReplicaId([1; 16]),
        seq: Seq(1),
    };
    let request = MemoryRequest::Record(record);
    assert_eq!(
        changes(&request, &MemoryReply::Recorded(event.clone())),
        vec![Change::Recorded {
            event,
            kind: "thing.archived".to_owned()
        }]
    );
    assert!(
        changes(&request, &MemoryReply::Ok).is_empty(),
        "nothing stored, nothing to say"
    );
    assert!(changes(&request, &MemoryReply::Refused(Refusal::Busy)).is_empty());
    let pause = MemoryRequest::Pause(space("work"), UnixSeconds(1));
    assert_eq!(
        changes(&pause, &MemoryReply::Ok),
        vec![Change::StatusChanged(space("work"))]
    );
    let propose = MemoryRequest::Propose(
        space("work"),
        FactDraft {
            topic: TopicPath::parse("a").expect("topic"),
            text: FactText::parse("x").expect("text"),
            links: vec![],
            supersedes: vec![],
            on_behalf: None,
        },
    );
    let fact = FactId::mint(1, [1; 10]);
    assert_eq!(
        changes(
            &propose,
            &MemoryReply::Proposed(fact.clone(), FactState::Pending)
        ),
        vec![Change::PendingChanged(space("work"))]
    );
    assert!(changes(&propose, &MemoryReply::Proposed(fact, FactState::Active)).is_empty());
}
