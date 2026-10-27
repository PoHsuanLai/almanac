//! The signals of `org.quire.Memory1.Control` and how they are emitted.

use almanac_dbus::{CONTROL_INTERFACE, MEMORY_PATH};
use zbus::Connection;

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

async fn send<B>(
    connection: &Connection,
    to: Option<zbus::names::BusName<'_>>,
    member: &'static str,
    body: &B,
) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    connection
        .emit_signal(to, MEMORY_PATH, CONTROL_INTERFACE, member, body)
        .await
}

/// Emits `signal` from `MEMORY_PATH` to the one connection `to` (a unique name): the signals
/// carry Space ids and what happened in them, so they go to the peers entitled to them, never to
/// the whole bus.
pub async fn emit_to(connection: &Connection, to: &str, signal: &Signal) -> zbus::Result<()> {
    let to = zbus::names::BusName::try_from(to)?;
    emit_signal(connection, Some(to), signal).await
}

/// Emits `signal` from `MEMORY_PATH` to every connection that listens. Only for a host that has
/// no peer table (a test); memoryd uses [`emit_to`].
pub async fn emit(connection: &Connection, signal: &Signal) -> zbus::Result<()> {
    emit_signal(connection, None, signal).await
}

async fn emit_signal(
    connection: &Connection,
    to: Option<zbus::names::BusName<'_>>,
    signal: &Signal,
) -> zbus::Result<()> {
    match signal {
        Signal::Recorded {
            space,
            event_ref,
            kind,
        } => {
            let body = (space.as_str(), event_ref.as_str(), kind.as_str());
            send(connection, to, "Recorded", &body).await
        }
        Signal::Forgotten { space, report } => {
            send(
                connection,
                to,
                "Forgotten",
                &(space.as_str(), report.as_str()),
            )
            .await
        }
        Signal::PendingChanged { space, count } => {
            send(connection, to, "PendingChanged", &(space.as_str(), *count)).await
        }
        Signal::ConsolidationReady { space, run } => {
            send(
                connection,
                to,
                "ConsolidationReady",
                &(space.as_str(), run.as_str()),
            )
            .await
        }
        Signal::StatusChanged { space, status } => {
            send(
                connection,
                to,
                "StatusChanged",
                &(space.as_str(), status.as_str()),
            )
            .await
        }
    }
}
