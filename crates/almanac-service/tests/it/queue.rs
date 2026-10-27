//! The request queue: what a request holds. (That requests for one Space wait for each other is
//! `queue::line`'s unit tests, and memoryd's `tests/it/queue.rs` on a real service.)

use crate::support::space;
use almanac_core::*;
use almanac_fake::{mail_thread_archived, trusted_label};
use almanac_service::Caller;
use almanac_service::{Claim, claim_for, claim_of};
use std::collections::BTreeSet;

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
            label: trusted_label(),
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
            MemoryRequest::DiscardConsolidation(RunId::parse("run-1").expect("r")),
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
        (
            MemoryRequest::ConfirmSettings(Default::default()),
            Claim::Everything,
        ),
        (MemoryRequest::PendingSettings, Claim::Nothing),
    ];
    for (request, claim) in table {
        assert_eq!(claim_of(&request), claim, "{request:?}");
    }
}

#[test]
fn a_request_the_service_will_refuse_holds_nothing() {
    let settle = || MemoryRequest::Settle(FactId::mint(1, [0; 10]), Settlement::Discard);
    let forget = || MemoryRequest::Forget(PlanToken::parse("p-1").expect("t"));
    let table = [
        (Caller::ShellUi, settle(), Claim::Everything),
        (Caller::Router, settle(), Claim::Nothing),
        (Caller::Cuad, forget(), Claim::Nothing),
        (Caller::ShellUi, forget(), Claim::Everything),
        (Caller::Router, search_in("work"), spaces(&["work"])),
    ];
    for (caller, request, claim) in table {
        assert_eq!(
            claim_for(&caller, &request),
            claim,
            "{caller:?} {request:?}"
        );
    }
}
