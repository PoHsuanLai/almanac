//! Requests for one Space queue instead of colliding: `Busy` never reaches a client.

use crate::support::{SharedKeys, dirs_in, service, space};
use almanac_core::*;
use almanac_fake::mail_thread_archived;
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
async fn two_requests_at_once_for_one_space_queue_instead_of_meeting_busy() {
    let scratch = tempfile::tempdir().expect("scratch");
    let keys = SharedKeys::default();

    // The embedder gives the executor back mid-request, as a call to inferd does, so the second
    // request meets the first while its Space is out: it waits instead of being answered Busy.
    let queued = service(&dirs_in(&scratch.path().join("queued")), &keys);
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
    let queued = Arc::new(service(&dirs_in(scratch.path()), &SharedKeys::default()));
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
