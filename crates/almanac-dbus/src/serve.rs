//! The daemon's half of the bus: the three interfaces of `org.quire.Memory1` as zbus objects over
//! one [`Serve`] handler. Every method builds the [`Call`] of its member from its raw arguments
//! and hands it, with the sender's unique name, to the handler; the handler answers with the
//! member's outputs (`decode_request` and `encode_reply` are the two ends of that table). Their
//! introspection is the checked-in `dbus/org.quire.Memory1.xml`, the same as the frozen
//! skeletons' (`tests/served.rs`).

use crate::codec::{Call, CallArg, Iface};
use crate::{CONTROL_INTERFACE, MEMORY_BUS, MEMORY_PATH, MemoryError};
use std::future::Future;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use zbus::Connection;
use zbus::message::Header;
use zbus::object_server::{Interface, SignalEmitter};
use zbus::zvariant::OwnedFd as BusFd;

/// What memoryd implements: one call in, the member's outputs out. `sender` is the caller's
/// unique bus name; who that is (`Caller`) is the handler's to establish.
pub trait Serve: Send + Sync + 'static {
    /// Serves `call` for `sender`; `fd` is the stream `Export` writes to.
    fn serve(
        &self,
        sender: &str,
        call: Call,
        fd: Option<OwnedFd>,
    ) -> impl Future<Output = Result<Vec<String>, MemoryError>> + Send;
}

fn sender_of(header: &Header<'_>) -> Result<String, MemoryError> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or_else(|| MemoryError::NotAllowed("no sender".into()))
}

fn text(value: String) -> CallArg {
    CallArg::Text(value)
}

async fn forward<S: Serve>(
    handler: &S,
    header: &Header<'_>,
    interface: Iface,
    member: &'static str,
    args: Vec<CallArg>,
    fd: Option<OwnedFd>,
) -> Result<Vec<String>, MemoryError> {
    let call = Call {
        interface,
        member,
        args,
    };
    handler.serve(&sender_of(header)?, call, fd).await
}

fn shape() -> MemoryError {
    MemoryError::Invalid("the handler answered with the wrong number of values".into())
}

fn first(mut outputs: Vec<String>) -> Result<String, MemoryError> {
    match outputs.len() {
        1 => outputs.pop().ok_or_else(shape),
        _ => Err(shape()),
    }
}

fn two(mut outputs: Vec<String>) -> Result<(String, String), MemoryError> {
    match (outputs.pop(), outputs.pop(), outputs.is_empty()) {
        (Some(b), Some(a), true) => Ok((a, b)),
        _ => Err(shape()),
    }
}

fn none(outputs: Vec<String>) -> Result<(), MemoryError> {
    outputs.is_empty().then_some(()).ok_or_else(shape)
}

/// `org.quire.Memory1.Record`.
#[derive(Debug)]
pub struct RecordObject<S>(Arc<S>);

#[zbus::interface(name = "org.quire.Memory1.Record")]
impl<S: Serve> RecordObject<S> {
    async fn record(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        record: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(record)];
        first(forward(&*self.0, &h, Iface::Record, "Record", args, None).await?)
    }

    #[zbus(out_args("first_ref", "count"))]
    async fn record_batch(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        records: String,
    ) -> Result<(String, u32), MemoryError> {
        let args = vec![text(space), text(records)];
        let (event, count) =
            two(forward(&*self.0, &h, Iface::Record, "RecordBatch", args, None).await?)?;
        let count = count.parse().map_err(|_| shape())?;
        Ok((event, count))
    }

    async fn explain_file(
        &self,
        #[zbus(header)] h: Header<'_>,
        why: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Record,
                "ExplainFile",
                vec![text(why)],
                None,
            )
            .await?,
        )
    }

    async fn mark(&self, #[zbus(header)] h: Header<'_>, mark: String) -> Result<(), MemoryError> {
        none(forward(&*self.0, &h, Iface::Record, "Mark", vec![text(mark)], None).await?)
    }
}

/// `org.quire.Memory1.Recall`.
#[derive(Debug)]
pub struct RecallObject<S>(Arc<S>);

#[zbus::interface(name = "org.quire.Memory1.Recall")]
impl<S: Serve> RecallObject<S> {
    async fn search(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Search", args, None).await?)
    }

    async fn facts(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Facts", args, None).await?)
    }

    async fn inject(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Inject", args, None).await?)
    }

    async fn recent(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Recent", args, None).await?)
    }

    async fn related(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        thing: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(thing)];
        first(forward(&*self.0, &h, Iface::Recall, "Related", args, None).await?)
    }

    async fn provenance(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        path: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(path)];
        first(forward(&*self.0, &h, Iface::Recall, "Provenance", args, None).await?)
    }

    async fn primer(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Recall,
                "Primer",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    #[zbus(out_args("fact_id", "state"))]
    async fn propose(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        draft: String,
    ) -> Result<(String, String), MemoryError> {
        let args = vec![text(space), text(draft)];
        two(forward(&*self.0, &h, Iface::Recall, "Propose", args, None).await?)
    }
}

/// `org.quire.Memory1.Control`.
#[derive(Debug)]
pub struct ControlObject<S>(Arc<S>);

#[zbus::interface(name = "org.quire.Memory1.Control")]
impl<S: Serve> ControlObject<S> {
    async fn spaces(&self, #[zbus(header)] h: Header<'_>) -> Result<String, MemoryError> {
        first(forward(&*self.0, &h, Iface::Control, "Spaces", vec![], None).await?)
    }

    async fn status(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Status",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn timeline(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Control, "Timeline", args, None).await?)
    }

    async fn plan_forget(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        scope: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(scope)];
        first(forward(&*self.0, &h, Iface::Control, "PlanForget", args, None).await?)
    }

    async fn forget(
        &self,
        #[zbus(header)] h: Header<'_>,
        token: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Forget",
                vec![text(token)],
                None,
            )
            .await?,
        )
    }

    async fn pending(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Pending",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn settle(
        &self,
        #[zbus(header)] h: Header<'_>,
        fact: String,
        verdict: String,
    ) -> Result<(), MemoryError> {
        let args = vec![text(fact), text(verdict)];
        none(forward(&*self.0, &h, Iface::Control, "Settle", args, None).await?)
    }

    async fn consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space)];
        first(forward(&*self.0, &h, Iface::Control, "Consolidation", args, None).await?)
    }

    async fn run_consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<(), MemoryError> {
        let args = vec![text(space)];
        none(forward(&*self.0, &h, Iface::Control, "RunConsolidation", args, None).await?)
    }

    async fn revert(&self, #[zbus(header)] h: Header<'_>, run: String) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Revert",
                vec![text(run)],
                None,
            )
            .await?,
        )
    }

    async fn apply_consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        run: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "ApplyConsolidation",
                vec![text(run)],
                None,
            )
            .await?,
        )
    }

    async fn discard_consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        run: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "DiscardConsolidation",
                vec![text(run)],
                None,
            )
            .await?,
        )
    }

    async fn rules(&self, #[zbus(header)] h: Header<'_>) -> Result<String, MemoryError> {
        first(forward(&*self.0, &h, Iface::Control, "Rules", vec![], None).await?)
    }

    async fn set_rule(
        &self,
        #[zbus(header)] h: Header<'_>,
        rule: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "SetRule",
                vec![text(rule)],
                None,
            )
            .await?,
        )
    }

    async fn remove_rule(
        &self,
        #[zbus(header)] h: Header<'_>,
        id: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "RemoveRule",
                vec![text(id)],
                None,
            )
            .await?,
        )
    }

    async fn pause(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        until: u64,
    ) -> Result<(), MemoryError> {
        let args = vec![text(space), CallArg::Seconds(until)];
        none(forward(&*self.0, &h, Iface::Control, "Pause", args, None).await?)
    }

    async fn resume(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Resume",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn verify(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Verify",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn rebuild(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Rebuild",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn sweep(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Sweep",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn export(
        &self,
        #[zbus(header)] h: Header<'_>,
        options: String,
        out: BusFd,
    ) -> Result<String, MemoryError> {
        let fd = OwnedFd::from(out);
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Export",
                vec![text(options)],
                Some(fd),
            )
            .await?,
        )
    }

    #[zbus(signal)]
    async fn recorded(
        emitter: &SignalEmitter<'_>,
        space: &str,
        event_ref: &str,
        kind: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn forgotten(emitter: &SignalEmitter<'_>, space: &str, report: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn pending_changed(
        emitter: &SignalEmitter<'_>,
        space: &str,
        count: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn consolidation_ready(
        emitter: &SignalEmitter<'_>,
        space: &str,
        run: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_changed(
        emitter: &SignalEmitter<'_>,
        space: &str,
        status: &str,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn version(&self) -> u32 {
        almanac_core::MEMORY_WIRE_VERSION
    }
}

/// The signals of `org.quire.Memory1.Control`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// A record was stored.
    Recorded {
        /// Its Space.
        space: String,
        /// The `EventRef` JSON.
        event_ref: String,
        /// The kind tag.
        kind: String,
    },
    /// A forget was applied.
    Forgotten {
        /// Its Space.
        space: String,
        /// The `ForgetReport` JSON.
        report: String,
    },
    /// The pending count changed.
    PendingChanged {
        /// The Space.
        space: String,
        /// How many are pending.
        count: u32,
    },
    /// A consolidation diff is ready.
    ConsolidationReady {
        /// The Space.
        space: String,
        /// The run id.
        run: String,
    },
    /// A Space's status changed.
    StatusChanged {
        /// The Space.
        space: String,
        /// The `SpaceStatus` JSON.
        status: String,
    },
}

/// Puts the three interfaces at `MEMORY_PATH` over `handler`, then claims `MEMORY_BUS`: nothing
/// is reachable under the name before every interface is.
pub async fn serve_on<S: Serve>(connection: &Connection, handler: Arc<S>) -> zbus::Result<()> {
    let server = connection.object_server();
    server
        .at(MEMORY_PATH, RecordObject(handler.clone()))
        .await?;
    server
        .at(MEMORY_PATH, RecallObject(handler.clone()))
        .await?;
    server.at(MEMORY_PATH, ControlObject(handler)).await?;
    connection.request_name(MEMORY_BUS).await
}

async fn send<B>(connection: &Connection, member: &'static str, body: &B) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    connection
        .emit_signal(
            None::<zbus::names::BusName<'_>>,
            MEMORY_PATH,
            CONTROL_INTERFACE,
            member,
            body,
        )
        .await
}

/// Emits `signal` from `MEMORY_PATH`.
pub async fn emit(connection: &Connection, signal: &Signal) -> zbus::Result<()> {
    match signal {
        Signal::Recorded {
            space,
            event_ref,
            kind,
        } => {
            let body = (space.as_str(), event_ref.as_str(), kind.as_str());
            send(connection, "Recorded", &body).await
        }
        Signal::Forgotten { space, report } => {
            send(connection, "Forgotten", &(space.as_str(), report.as_str())).await
        }
        Signal::PendingChanged { space, count } => {
            send(connection, "PendingChanged", &(space.as_str(), *count)).await
        }
        Signal::ConsolidationReady { space, run } => {
            send(
                connection,
                "ConsolidationReady",
                &(space.as_str(), run.as_str()),
            )
            .await
        }
        Signal::StatusChanged { space, status } => {
            send(
                connection,
                "StatusChanged",
                &(space.as_str(), status.as_str()),
            )
            .await
        }
    }
}

/// The introspection of the served objects: the same document as the skeletons' (`introspection`).
pub fn served_introspection<S: Serve>(handler: &Arc<S>) -> String {
    let objects = (
        RecordObject(handler.clone()),
        RecallObject(handler.clone()),
        ControlObject(handler.clone()),
    );
    crate::introspect::document([&objects.0 as &dyn Interface, &objects.1, &objects.2])
}
