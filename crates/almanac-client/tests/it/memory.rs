//! The client over its transports.

use almanac_client::*;
use almanac_core::*;
use almanac_fake::*;

#[tokio::test]
async fn absent_transport_is_noop() {
    let memory = Memory::over(Absent);
    let record = mail_thread_archived().expect("fixture");
    assert_eq!(memory.record(record.clone()).await, Ok(Recorded::NoMemory));
    assert_eq!(
        memory.record_batch(vec![record]).await,
        Ok(Recorded::NoMemory)
    );
    let query = RecallQuery {
        space: SpaceId::parse("work").expect("s"),
        text: "x".into(),
        limit: Count(1),
        over: RecallOver::Both,
    };
    assert_eq!(
        memory.search(query).await,
        Err(ClientError::Transport(TransportError::Absent)),
        "readers say there is no memory"
    );
}

/// A transport that answers every request with one reply.
struct Canned(MemoryReply);

impl Transport for Canned {
    async fn call(&self, _request: MemoryRequest) -> Result<MemoryReply, TransportError> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn replies_map_to_results() {
    let event = EventRef {
        space: SpaceId::parse("work").expect("s"),
        replica: ReplicaId([1; 16]),
        seq: Seq(1),
    };
    let record = mail_thread_archived().expect("fixture");
    assert_eq!(
        Memory::over(Canned(MemoryReply::Recorded(event.clone())))
            .record(record.clone())
            .await,
        Ok(Recorded::Stored(event.clone()))
    );
    assert_eq!(
        Memory::over(Canned(MemoryReply::RecordedBatch(event.clone(), Count(2))))
            .record_batch(vec![record.clone()])
            .await,
        Ok(Recorded::Stored(event))
    );
    assert_eq!(
        Memory::over(Canned(MemoryReply::Refused(Refusal::NotAllowed)))
            .record(record.clone())
            .await,
        Err(ClientError::Refused(Refusal::NotAllowed))
    );
    assert_eq!(
        Memory::over(Canned(MemoryReply::Primer("x".into())))
            .record(record)
            .await,
        Err(ClientError::Unexpected)
    );
    let status = Memory::over(Canned(MemoryReply::Ok))
        .status(SpaceId::parse("work").expect("s"))
        .await;
    assert_eq!(status, Err(ClientError::Unexpected));
    let pending = Memory::over(Canned(MemoryReply::Pending(vec![])))
        .pending(SpaceId::parse("work").expect("s"))
        .await;
    assert_eq!(pending, Ok(vec![]));
}
