//! The durable append: `RecordDurable` answers only once the event is in the log's storage, and
//! says exactly why when it is not.
//!
//! `Record` is fire and forget: a locked Space buffers the record, a paused Space or a rule drops
//! it, and the answer is `Ok` or the event either way. A writer that must know the event is
//! durable before it acts (docket writes a session's taint entry before it reveals untrusted
//! text) asks for `RecordDurable`: the Space is never buffered, a drop is a refusal, and the
//! reply is returned after the log's commit, which `SqliteLog` makes with `synchronous=FULL`.

use crate::backend::Backend;
use crate::record::land_in_receiver;
use crate::service::MemoryService;
use almanac_core::{Ack, Caller, MemoryReply, Record, Refusal};

impl<B: Backend> MemoryService<B> {
    /// One record into its Space, acknowledged only when committed.
    pub(crate) async fn record_durable(
        &self,
        caller: &Caller,
        mut record: Record,
    ) -> Result<MemoryReply, Refusal> {
        land_in_receiver(&mut record);
        let id = record.space.clone();
        let mut lease = self.checkout(caller, &id).await.map_err(unavailable)?;
        let cx = self.cx(caller);
        let open = lease.open().ok_or(Refusal::Busy)?;
        let event = open.record_whole(&cx, record).await?;
        Ok(MemoryReply::Durable(Ack { event }))
    }
}

/// A Space that would not open for a reason other than a lock is unavailable to a writer that
/// needs the answer, not "invalid".
fn unavailable(refusal: Refusal) -> Refusal {
    match refusal {
        Refusal::Invalid(_) => Refusal::Unavailable,
        other => other,
    }
}
