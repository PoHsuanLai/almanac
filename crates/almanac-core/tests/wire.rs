//! The wire and the stored forms: every variant survives serde, tags and JSON are pinned.

mod common;

use almanac_core::*;
use common::*;

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
        MemoryRequest::Sweep(_) => 30,
    }
}

#[test]
fn wire_round_trip_every_request() {
    let all = requests();
    round_trips(&all);
    let covered: std::collections::BTreeSet<usize> = all.iter().map(request_index).collect();
    assert_eq!(
        covered,
        (0..=30).collect(),
        "a request variant has no sample"
    );
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
        MemoryReply::Swept(_) => 21,
    }
}

#[test]
fn wire_round_trip_every_reply() {
    let all = replies();
    round_trips(&all);
    let covered: std::collections::BTreeSet<usize> = all.iter().map(reply_index).collect();
    assert_eq!(covered, (0..=21).collect(), "a reply variant has no sample");
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
