//! Requests for one Space queue instead of colliding: `Busy` never reaches a client.

mod common;

use almanac_core::*;
use almanac_fake::mail_thread_archived;
use common::{SharedKeys, dirs_in, service, space};
use memoryd::{Claim, Serialised, claim_of};
use std::collections::BTreeSet;
use std::sync::Arc;

fn record_in(space_id: &str, key: &str) -> Record {
    let mut record = mail_thread_archived().expect("fixture");
    record.space = space(space_id);
    if let EventBody::Thing { thing, .. } = &mut record.body {
        thing.thing.key = ThingKey::parse(key).expect("key");
    }
    record
}

fn search_in(space_id: &str) -> MemoryRequest {
    MemoryRequest::Search(RecallQuery {
        space: space(space_id),
        text: "budget".into(),
        limit: Count(5),
        over: RecallOver::Both,
    })
}

#[tokio::test]
async fn two_requests_at_once_for_one_space_collide_without_the_queue_and_queue_with_it() {
    let scratch = tempfile::tempdir().expect("scratch");
    let keys = SharedKeys::default();

    // The bare service: the second request finds the Space checked out by the first (the
    // embedder gives the executor back mid-request, as a call to inferd does).
    let bare = service(&dirs_in(&scratch.path().join("bare")), &keys);
    let (a, b) = tokio::join!(
        bare.handle(
            &Caller::Router,
            MemoryRequest::Record(record_in("work", "a1"))
        ),
        bare.handle(
            &Caller::Router,
            MemoryRequest::Record(record_in("work", "a2"))
        ),
    );
    assert!(
        [&a, &b]
            .iter()
            .any(|r| **r == MemoryReply::Refused(Refusal::Busy)),
        "without a queue one of them is Busy: {a:?} {b:?}"
    );

    // The same two requests through the queue both succeed, in order.
    let queued = Serialised::new(service(&dirs_in(&scratch.path().join("queued")), &keys));
    let (a, b) = tokio::join!(
        queued.handle(
            &Caller::Router,
            MemoryRequest::Record(record_in("work", "a1"))
        ),
        queued.handle(
            &Caller::Router,
            MemoryRequest::Record(record_in("work", "a2"))
        ),
    );
    let (MemoryReply::Recorded(first), MemoryReply::Recorded(second)) = (&a, &b) else {
        panic!("both are stored: {a:?} {b:?}")
    };
    assert_eq!(
        (first.seq, second.seq),
        (Seq(1), Seq(2)),
        "in the order they arrived"
    );
}

#[tokio::test]
async fn a_crowd_of_mixed_requests_never_sees_busy() {
    let scratch = tempfile::tempdir().expect("scratch");
    let queued = Arc::new(Serialised::new(service(
        &dirs_in(scratch.path()),
        &SharedKeys::default(),
    )));
    let mut tasks = Vec::new();
    for n in 0..24 {
        let queue = queued.clone();
        tasks.push(tokio::spawn(async move {
            let (caller, request) = match n % 6 {
                0 => (
                    Caller::Router,
                    MemoryRequest::Record(record_in("work", &format!("w{n}"))),
                ),
                1 => (
                    Caller::Router,
                    MemoryRequest::Record(record_in("home", &format!("h{n}"))),
                ),
                2 => (Caller::Router, search_in("work")),
                3 => (Caller::ShellUi, MemoryRequest::Status(space("home"))),
                // A request that names no Space holds every queue.
                4 => (Caller::ShellUi, MemoryRequest::Spaces),
                _ => (
                    Caller::ShellUi,
                    MemoryRequest::Settle(FactId::mint(1, [7; 10]), Settlement::Discard),
                ),
            };
            queue.handle(&caller, request).await
        }));
    }
    for task in tasks {
        let reply = task.await.expect("task");
        assert_ne!(reply, MemoryReply::Refused(Refusal::Busy), "{reply:?}");
    }
}

fn spaces(ids: &[&str]) -> Claim {
    Claim::Spaces(ids.iter().map(|s| space(s)).collect::<BTreeSet<_>>())
}

#[test]
fn what_a_request_holds() {
    let message_to_home = {
        let mut record = record_in("work", "m");
        record.body = EventBody::Message(Box::new(Message {
            id: MessageId::parse("m-2").expect("id"),
            thread: ThreadId::parse("m-1").expect("id"),
            in_reply_to: None,
            from: Address::new(AgentRef::Companion, space("work")),
            to: Address::new(AgentRef::User, space("home")),
            kind: MessageKind::Note,
            parts: vec![Part::Text(MessageText::new("hello"))],
            label: almanac_fake::trusted_label(),
            sent: UnixSeconds(1),
        }));
        record
    };
    let table = [
        (MemoryRequest::Spaces, Claim::Nothing),
        (MemoryRequest::Rules, Claim::Nothing),
        (
            MemoryRequest::Record(record_in("work", "a")),
            spaces(&["work"]),
        ),
        (MemoryRequest::Record(message_to_home), spaces(&["home"])),
        (
            MemoryRequest::RecordBatch(vec![record_in("work", "a"), record_in("home", "b")]),
            spaces(&["home", "work"]),
        ),
        (search_in("work"), spaces(&["work"])),
        (MemoryRequest::Status(space("home")), spaces(&["home"])),
        (
            MemoryRequest::RunConsolidation(space("work")),
            spaces(&["work"]),
        ),
        (MemoryRequest::Sweep(space("work")), spaces(&["work"])),
        (
            MemoryRequest::Forget(PlanToken::parse("p-1").expect("t")),
            Claim::Everything,
        ),
        (
            MemoryRequest::Settle(FactId::mint(1, [0; 10]), Settlement::Discard),
            Claim::Everything,
        ),
        (
            MemoryRequest::Revert(RunId::parse("run-1").expect("r")),
            Claim::Everything,
        ),
        (
            MemoryRequest::ApplyConsolidation(RunId::parse("run-1").expect("r")),
            Claim::Everything,
        ),
        (
            MemoryRequest::Export(ExportOptions {
                spaces: vec![],
                verification_key: VerificationKey::Omit,
            }),
            Claim::Everything,
        ),
        (
            MemoryRequest::RemoveRule(RuleId::parse("r-1").expect("r")),
            Claim::Everything,
        ),
    ];
    for (request, claim) in table {
        assert_eq!(claim_of(&request), claim, "{request:?}");
    }
}
