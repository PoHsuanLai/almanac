//! Every member of the bus reaches its handler: each sample request goes out through the real
//! client path (`encode_request`, `invoke`) over a private bus and the served objects, and the
//! handler sees the very `Call` that was sent. A member that `invoke` does not know answers
//! `Invalid("no member")` and never arrives, which is how `Entries` and `RecordDurable` once
//! worked in process and nowhere else.

#[path = "../../almanac-core/tests/common/mod.rs"]
mod wire;

mod common;

use almanac_core::Refusal;
use almanac_dbus::{Call, MemoryError, Serve, encode_request, introspection, invoke, serve_on};
use common::bus::PrivateBus;
use common::connect;
use std::collections::BTreeSet;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};

/// Records each call and refuses it as busy, so the caller learns the call arrived.
#[derive(Default)]
struct Witness(Mutex<Vec<Call>>);

impl Serve for Witness {
    async fn serve(
        &self,
        _sender: &str,
        call: Call,
        _fd: Option<OwnedFd>,
    ) -> Result<Vec<String>, MemoryError> {
        self.0.lock().expect("calls").push(call);
        Err(MemoryError::Busy(String::new()))
    }
}

fn members_of(xml: &str) -> BTreeSet<String> {
    xml.split("<method name=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn every_member_of_the_interface_is_invoked_over_the_bus() {
    let scratch = tempfile::tempdir().expect("scratch");
    let bus = PrivateBus::start(scratch.path());
    let witness = Arc::new(Witness::default());
    let server = connect(&bus.address).await;
    serve_on(&server, witness.clone()).await.expect("serve");
    let client = connect(&bus.address).await;

    let mut reached = BTreeSet::new();
    for request in wire::requests() {
        let call = encode_request(&request).expect("encode");
        let stream = std::fs::File::create(scratch.path().join("export.tar")).expect("stream");
        let result = invoke(&client, &call, Some(OwnedFd::from(stream))).await;
        let refusal = result.expect_err("the witness refuses").refusal();
        assert_eq!(refusal, Some(Refusal::Busy), "{call:?} did not arrive");
        reached.insert(call.member.to_owned());
    }

    let sent: Vec<Call> = witness.0.lock().expect("calls").clone();
    let arrived: BTreeSet<String> = sent.iter().map(|c| c.member.to_owned()).collect();
    assert_eq!(
        arrived, reached,
        "each call arrived as the member it was sent to"
    );
    assert_eq!(
        reached,
        members_of(&introspection()),
        "the samples cover every declared member, and invoke knows each"
    );
}
