//! The caller's half of the bus: one [`Call`] through the proxy of its member. The client's
//! transport is `encode_request`, this, `decode_reply`; a bus error that carries one of memoryd's
//! error names comes back as the `MemoryError` of that name (`MemoryError::refusal`).

use crate::codec::{Args, Call, Iface};
use crate::{ControlProxy, MemoryError, RecallProxy, RecordProxy};
use std::os::fd::OwnedFd;
use zbus::Connection;

fn fd_missing() -> MemoryError {
    MemoryError::Invalid("Export needs the stream it writes to".into())
}

/// Calls `call` on `connection` and returns the member's outputs as `s` values, in order (a
/// number is its decimal). `fd` is the stream `Export` writes the tar to; other members take
/// none.
pub async fn invoke(
    connection: &Connection,
    call: &Call,
    fd: Option<OwnedFd>,
) -> Result<Vec<String>, MemoryError> {
    let mut a = Args::of(call);
    match (call.interface, call.member) {
        (Iface::Record, member) => {
            let p = RecordProxy::new(connection).await?;
            match member {
                "Record" => Ok(vec![p.record(a.text()?, a.text()?).await?]),
                "RecordBatch" => {
                    let (first, count) = p.record_batch(a.text()?, a.text()?).await?;
                    Ok(vec![first, count.to_string()])
                }
                "ExplainFile" => p
                    .explain_file(a.text()?)
                    .await
                    .map(|()| vec![])
                    .map_err(Into::into),
                "Mark" => p.mark(a.text()?).await.map(|()| vec![]).map_err(Into::into),
                other => Err(MemoryError::Invalid(format!("no member {other}"))),
            }
        }
        (Iface::Recall, member) => {
            let p = RecallProxy::new(connection).await?;
            match member {
                "Search" => Ok(vec![p.search(a.text()?, a.text()?).await?]),
                "Facts" => Ok(vec![p.facts(a.text()?, a.text()?).await?]),
                "Inject" => Ok(vec![p.inject(a.text()?, a.text()?).await?]),
                "Recent" => Ok(vec![p.recent(a.text()?, a.text()?).await?]),
                "Related" => Ok(vec![p.related(a.text()?, a.text()?).await?]),
                "Provenance" => Ok(vec![p.provenance(a.text()?, a.text()?).await?]),
                "Primer" => Ok(vec![p.primer(a.text()?).await?]),
                "Propose" => {
                    let (id, state) = p.propose(a.text()?, a.text()?).await?;
                    Ok(vec![id, state])
                }
                other => Err(MemoryError::Invalid(format!("no member {other}"))),
            }
        }
        (Iface::Control, member) => {
            let p = ControlProxy::new(connection).await?;
            let done = |r: zbus::Result<()>| r.map(|()| Vec::new()).map_err(MemoryError::from);
            match member {
                "Spaces" => Ok(vec![p.spaces().await?]),
                "Status" => Ok(vec![p.status(a.text()?).await?]),
                "Timeline" => Ok(vec![p.timeline(a.text()?, a.text()?).await?]),
                "PlanForget" => Ok(vec![p.plan_forget(a.text()?, a.text()?).await?]),
                "Forget" => Ok(vec![p.forget(a.text()?).await?]),
                "Pending" => Ok(vec![p.pending(a.text()?).await?]),
                "Settle" => done(p.settle(a.text()?, a.text()?).await),
                "Consolidation" => Ok(vec![p.consolidation(a.text()?).await?]),
                "RunConsolidation" => done(p.run_consolidation(a.text()?).await),
                "Revert" => done(p.revert(a.text()?).await),
                "ApplyConsolidation" => done(p.apply_consolidation(a.text()?).await),
                "Rules" => Ok(vec![p.rules().await?]),
                "SetRule" => done(p.set_rule(a.text()?).await),
                "RemoveRule" => done(p.remove_rule(a.text()?).await),
                "Pause" => done(p.pause(a.text()?, a.seconds()?).await),
                "Resume" => done(p.resume(a.text()?).await),
                "Verify" => Ok(vec![p.verify(a.text()?).await?]),
                "Rebuild" => done(p.rebuild(a.text()?).await),
                "Sweep" => Ok(vec![p.sweep(a.text()?).await?]),
                "Export" => {
                    let out = fd.ok_or_else(fd_missing)?;
                    Ok(vec![
                        p.export(a.text()?, zbus::zvariant::Fd::from(&out)).await?,
                    ])
                }
                other => Err(MemoryError::Invalid(format!("no member {other}"))),
            }
        }
    }
}
