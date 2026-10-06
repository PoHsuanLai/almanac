//! The codec table, both directions: every request of the wire samples survives
//! `encode_request` then `decode_request`, every reply survives `encode_reply` then
//! `decode_reply`, and the member table has the shape memory.md section 3.10 gives.

#[path = "../../almanac-core/tests/common/mod.rs"]
mod common;

use almanac_core::*;
use almanac_dbus::*;
use common::*;

fn call_of(member: &str) -> Call {
    requests()
        .iter()
        .filter_map(|r| encode_request(r).ok())
        .find(|c| c.member == member)
        .unwrap_or_else(|| panic!("no sample request for {member}"))
}

fn reply_where(pick: impl Fn(&MemoryReply) -> bool) -> MemoryReply {
    replies().into_iter().find(pick).expect("a sample reply")
}

#[test]
fn every_request_round_trips_through_its_member() {
    for request in requests() {
        let call = encode_request(&request).expect("encode");
        assert_eq!(decode_request(&call), Ok(request.clone()), "{call:?}");
    }
}

#[test]
fn the_member_table() {
    let table: Vec<(Iface, &str, usize)> = requests()
        .iter()
        .map(|r| {
            let c = encode_request(r).expect("encode");
            (c.interface, c.member, c.args.len())
        })
        .collect();
    use Iface::{Control, Recall, Record};
    for expected in [
        (Record, "Record", 2),
        (Record, "RecordBatch", 2),
        (Record, "ExplainFile", 1),
        (Record, "Mark", 1),
        (Recall, "Search", 2),
        (Recall, "Facts", 2),
        (Recall, "Inject", 2),
        (Recall, "Recent", 2),
        (Recall, "Related", 2),
        (Recall, "Provenance", 2),
        (Recall, "Primer", 1),
        (Recall, "Propose", 2),
        (Control, "Spaces", 0),
        (Control, "Status", 1),
        (Control, "Timeline", 2),
        (Control, "PlanForget", 2),
        (Control, "Forget", 1),
        (Control, "Pending", 1),
        (Control, "Settle", 2),
        (Control, "Consolidation", 1),
        (Control, "RunConsolidation", 1),
        (Control, "Revert", 1),
        (Control, "ApplyConsolidation", 1),
        (Control, "Rules", 0),
        (Control, "SetRule", 1),
        (Control, "RemoveRule", 1),
        (Control, "Pause", 2),
        (Control, "Resume", 1),
        (Control, "Verify", 1),
        (Control, "Rebuild", 1),
        (Control, "Export", 1),
        (Control, "Sweep", 1),
    ] {
        assert!(
            table.contains(&expected),
            "{expected:?} missing from {table:?}"
        );
    }
}

#[test]
fn every_member_is_declared_in_the_introspection() {
    let xml = introspection();
    for request in requests() {
        let member = encode_request(&request).expect("encode").member;
        assert!(
            xml.contains(&format!("<method name=\"{member}\">")),
            "{member}"
        );
    }
}

#[test]
fn replies_round_trip_through_their_members() {
    let event = event_ref(3);
    let pairs: Vec<(&str, MemoryReply)> = vec![
        ("Record", MemoryReply::Recorded(event.clone())),
        ("Record", MemoryReply::Ok),
        ("RecordBatch", MemoryReply::RecordedBatch(event, Count(2))),
        ("RecordBatch", MemoryReply::Ok),
        ("ExplainFile", MemoryReply::Ok),
        ("Mark", MemoryReply::Ok),
        ("Settle", MemoryReply::Ok),
        ("Revert", MemoryReply::Ok),
        ("ApplyConsolidation", MemoryReply::Ok),
        ("SetRule", MemoryReply::Ok),
        ("RemoveRule", MemoryReply::Ok),
        ("Pause", MemoryReply::Ok),
        ("Resume", MemoryReply::Ok),
        ("Rebuild", MemoryReply::Ok),
        ("RunConsolidation", MemoryReply::Ok),
        ("Search", reply_where(|r| matches!(r, MemoryReply::Hits(_)))),
        ("Inject", reply_where(|r| matches!(r, MemoryReply::Hits(_)))),
        ("Facts", reply_where(|r| matches!(r, MemoryReply::Facts(_)))),
        (
            "Recent",
            reply_where(|r| matches!(r, MemoryReply::Recent(_))),
        ),
        (
            "Related",
            reply_where(|r| matches!(r, MemoryReply::Related(_))),
        ),
        (
            "Provenance",
            reply_where(|r| matches!(r, MemoryReply::Provenance(_))),
        ),
        (
            "Primer",
            reply_where(|r| matches!(r, MemoryReply::Primer(_))),
        ),
        (
            "Propose",
            reply_where(|r| matches!(r, MemoryReply::Proposed(..))),
        ),
        (
            "Spaces",
            reply_where(|r| matches!(r, MemoryReply::Spaces(_))),
        ),
        (
            "Status",
            reply_where(|r| matches!(r, MemoryReply::Status(_))),
        ),
        (
            "Timeline",
            reply_where(|r| matches!(r, MemoryReply::Timeline(_))),
        ),
        (
            "PlanForget",
            reply_where(|r| matches!(r, MemoryReply::Plan(_))),
        ),
        (
            "Forget",
            reply_where(|r| matches!(r, MemoryReply::Forgot(_))),
        ),
        (
            "Pending",
            reply_where(|r| matches!(r, MemoryReply::Pending(_))),
        ),
        (
            "Consolidation",
            reply_where(|r| matches!(r, MemoryReply::Consolidation(_))),
        ),
        ("Rules", reply_where(|r| matches!(r, MemoryReply::Rules(_)))),
        (
            "Verify",
            reply_where(|r| matches!(r, MemoryReply::Verified(_))),
        ),
        (
            "Export",
            reply_where(|r| matches!(r, MemoryReply::Exported(_))),
        ),
        ("Sweep", reply_where(|r| matches!(r, MemoryReply::Swept(_)))),
    ];
    for (member, reply) in pairs {
        let call = call_of(member);
        let outputs = encode_reply(&call, &reply).expect("encode");
        assert_eq!(decode_reply(&call, &outputs), Ok(reply.clone()), "{member}");
    }
}

#[test]
fn the_non_json_outputs() {
    let primer = call_of("Primer");
    assert_eq!(
        encode_reply(&primer, &MemoryReply::Primer("# primer".into())).expect("encode"),
        vec!["# primer".to_owned()],
        "the primer is the markdown itself"
    );
    let batch = call_of("RecordBatch");
    assert_eq!(
        encode_reply(&batch, &MemoryReply::RecordedBatch(event_ref(1), Count(7))).expect("encode")
            [1],
        "7",
        "the count is a decimal"
    );
    assert_eq!(
        encode_reply(&batch, &MemoryReply::Ok).expect("encode"),
        vec![String::new(), "0".to_owned()],
        "a batch admission dropped answers an empty first output"
    );
}

#[test]
fn run_consolidation_returns_nothing_whatever_the_service_drafted() {
    let call = call_of("RunConsolidation");
    let draft = reply_where(|r| matches!(r, MemoryReply::Consolidation(_)));
    assert_eq!(
        encode_reply(&call, &draft).expect("encode"),
        Vec::<String>::new()
    );
}

#[test]
fn a_refusal_is_the_bus_error_of_its_name() {
    let call = call_of("Search");
    for refusal in [
        Refusal::NotAllowed,
        Refusal::SpaceLocked,
        Refusal::SpaceUnknown,
        Refusal::OutsideSpace,
        Refusal::PlanStale,
        Refusal::PlanExpired,
        Refusal::NoSuchFact,
        Refusal::NotPending,
        Refusal::Busy,
        Refusal::Invalid("why".into()),
    ] {
        let error = encode_reply(&call, &MemoryReply::Refused(refusal.clone()))
            .expect_err("a refusal is an error");
        assert_eq!(error.refusal(), Some(refusal));
    }
}

#[test]
fn a_reply_the_member_does_not_answer_with_is_invalid() {
    let call = call_of("Search");
    let error = encode_reply(&call, &MemoryReply::Rules(RuleSet::standard())).expect_err("kind");
    assert!(matches!(error.refusal(), Some(Refusal::Invalid(_))));
}

#[test]
fn malformed_outputs_are_shape_or_json_errors() {
    let search = call_of("Search");
    assert_eq!(decode_reply(&search, &[]), Err(CodecError::Shape));
    assert_eq!(
        decode_reply(&search, &["[]".into(), "[]".into()]),
        Err(CodecError::Shape)
    );
    assert!(matches!(
        decode_reply(&search, &["{".into()]),
        Err(CodecError::Json(_))
    ));
    assert_eq!(
        decode_reply(&call_of("Mark"), &["x".into()]),
        Err(CodecError::Shape),
        "a void member returns nothing"
    );
    assert_eq!(
        decode_reply(&call_of("RecordBatch"), &["".into()]),
        Err(CodecError::Shape)
    );
    assert!(matches!(
        decode_reply(
            &call_of("RecordBatch"),
            &["{\"space\":1}".into(), "x".into()]
        ),
        Err(CodecError::Json(_))
    ));
}

#[test]
fn malformed_calls_are_refused() {
    let mut search = call_of("Search");
    search.args.pop();
    assert_eq!(decode_request(&search), Err(CodecError::Shape));
    let mut extra = call_of("Primer");
    extra.args.push(CallArg::Text("x".into()));
    assert_eq!(decode_request(&extra), Err(CodecError::Shape));
    let mut wrong_kind = call_of("Pause");
    wrong_kind.args[1] = CallArg::Text("1".into());
    assert_eq!(decode_request(&wrong_kind), Err(CodecError::Shape));
    let unknown = Call {
        interface: Iface::Control,
        member: "Obliterate",
        args: vec![],
    };
    assert!(matches!(decode_request(&unknown), Err(CodecError::Json(_))));
    let mut bad_space = call_of("Status");
    bad_space.args[0] = CallArg::Text("Not A Space!".into());
    assert!(matches!(
        decode_request(&bad_space),
        Err(CodecError::Json(_))
    ));
}

#[test]
fn a_space_argument_that_disagrees_with_the_body_is_refused() {
    for member in ["Record", "Search", "Facts", "Inject", "RecordBatch"] {
        let mut call = call_of(member);
        call.args[0] = CallArg::Text("home".into());
        assert!(
            matches!(decode_request(&call), Err(CodecError::Json(_))),
            "{member}"
        );
    }
}

#[test]
fn an_empty_batch_and_a_time_before_the_epoch() {
    let empty = MemoryRequest::RecordBatch(vec![]);
    let call = encode_request(&empty).expect("encode");
    assert_eq!(decode_request(&call), Ok(empty));
    let early = MemoryRequest::Pause(space("work"), UnixSeconds(-1));
    assert!(matches!(encode_request(&early), Err(CodecError::Json(_))));
}

#[test]
fn a_space_travels_as_its_plain_id_and_a_token_as_json() {
    let status = encode_request(&MemoryRequest::Status(space("work"))).expect("encode");
    assert_eq!(status.args, vec![CallArg::Text("work".into())]);
    let forget = encode_request(&MemoryRequest::Forget(
        PlanToken::parse("p-1").expect("token"),
    ))
    .expect("encode");
    assert_eq!(forget.args, vec![CallArg::Text("\"p-1\"".into())]);
}
