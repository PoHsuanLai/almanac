//! `org.quire.Memory1.Control`: the person's controls (the shell UI only), the signals, and
//! the wire `Version`.

use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedFd;

/// The caller's side.
#[zbus::proxy(
    interface = "org.quire.Memory1.Control",
    default_service = "org.quire.Memory1",
    default_path = "/org/quire/Memory1"
)]
pub trait Control {
    /// The Spaces (`Vec<SpaceSummary>` JSON).
    fn spaces(&self) -> zbus::Result<String>;
    /// One status (`SpaceStatus` JSON).
    fn status(&self, space: &str) -> zbus::Result<String>;
    /// A timeline page (`TimelineQuery` JSON; answers `TimelinePage` JSON).
    fn timeline(&self, space: &str, query: &str) -> zbus::Result<String>;
    /// What a forget would remove (`ForgetScope` JSON; answers `ForgetPlanView` JSON).
    fn plan_forget(&self, space: &str, scope: &str) -> zbus::Result<String>;
    /// Applies a plan (`PlanToken` JSON; answers `ForgetReport` JSON).
    fn forget(&self, token: &str) -> zbus::Result<String>;
    /// Pending facts (`Vec<FactView>` JSON).
    fn pending(&self, space: &str) -> zbus::Result<String>;
    /// Keeps or discards a pending fact (`FactId`, `Settlement` JSON).
    fn settle(&self, fact: &str, verdict: &str) -> zbus::Result<()>;
    /// The last consolidation diff (`DraftView` JSON).
    fn consolidation(&self, space: &str) -> zbus::Result<String>;
    /// Runs consolidation now.
    fn run_consolidation(&self, space: &str) -> zbus::Result<()>;
    /// Reverts a run (`RunId` JSON).
    fn revert(&self, run: &str) -> zbus::Result<()>;
    /// The rules (`RuleSet` JSON).
    fn rules(&self) -> zbus::Result<String>;
    /// Adds or replaces a rule (`RememberRule` JSON).
    fn set_rule(&self, rule: &str) -> zbus::Result<()>;
    /// Removes a rule (`RuleId` JSON).
    fn remove_rule(&self, id: &str) -> zbus::Result<()>;
    /// Pauses memory for a Space until the time (seconds since the epoch).
    fn pause(&self, space: &str, until: u64) -> zbus::Result<()>;
    /// Resumes it.
    fn resume(&self, space: &str) -> zbus::Result<()>;
    /// Verifies the hash chain (answers `ChainReport` JSON).
    fn verify(&self, space: &str) -> zbus::Result<String>;
    /// Rebuilds the index.
    fn rebuild(&self, space: &str) -> zbus::Result<()>;
    /// Writes a tar export to `out` (`ExportOptions` JSON; answers `ExportManifest` JSON).
    fn export(&self, options: &str, out: zbus::zvariant::Fd<'_>) -> zbus::Result<String>;
    /// A record was stored.
    #[zbus(signal)]
    fn recorded(&self, space: &str, event_ref: &str, kind: &str) -> zbus::Result<()>;
    /// A forget was applied (`ForgetReport` JSON).
    #[zbus(signal)]
    fn forgotten(&self, space: &str, report: &str) -> zbus::Result<()>;
    /// The pending count changed.
    #[zbus(signal)]
    fn pending_changed(&self, space: &str, count: u32) -> zbus::Result<()>;
    /// A consolidation diff is ready for review.
    #[zbus(signal)]
    fn consolidation_ready(&self, space: &str, run: &str) -> zbus::Result<()>;
    /// A Space's status changed (`SpaceStatus` JSON).
    #[zbus(signal)]
    fn status_changed(&self, space: &str, status: &str) -> zbus::Result<()>;
    /// The wire version.
    #[zbus(property)]
    fn version(&self) -> zbus::Result<u32>;
}

/// The daemon's side.
#[derive(Debug, Default)]
pub struct ControlSkeleton;

#[zbus::interface(name = "org.quire.Memory1.Control")]
impl ControlSkeleton {
    fn spaces(&self) -> fdo::Result<String> {
        Err(crate::introspect::frozen())
    }

    fn status(&self, space: String) -> fdo::Result<String> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn timeline(&self, space: String, query: String) -> fdo::Result<String> {
        let _ = (space, query);
        Err(crate::introspect::frozen())
    }

    fn plan_forget(&self, space: String, scope: String) -> fdo::Result<String> {
        let _ = (space, scope);
        Err(crate::introspect::frozen())
    }

    fn forget(&self, token: String) -> fdo::Result<String> {
        let _ = token;
        Err(crate::introspect::frozen())
    }

    fn pending(&self, space: String) -> fdo::Result<String> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn settle(&self, fact: String, verdict: String) -> fdo::Result<()> {
        let _ = (fact, verdict);
        Err(crate::introspect::frozen())
    }

    fn consolidation(&self, space: String) -> fdo::Result<String> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn run_consolidation(&self, space: String) -> fdo::Result<()> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn revert(&self, run: String) -> fdo::Result<()> {
        let _ = run;
        Err(crate::introspect::frozen())
    }

    fn rules(&self) -> fdo::Result<String> {
        Err(crate::introspect::frozen())
    }

    fn set_rule(&self, rule: String) -> fdo::Result<()> {
        let _ = rule;
        Err(crate::introspect::frozen())
    }

    fn remove_rule(&self, id: String) -> fdo::Result<()> {
        let _ = id;
        Err(crate::introspect::frozen())
    }

    fn pause(&self, space: String, until: u64) -> fdo::Result<()> {
        let _ = (space, until);
        Err(crate::introspect::frozen())
    }

    fn resume(&self, space: String) -> fdo::Result<()> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn verify(&self, space: String) -> fdo::Result<String> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn rebuild(&self, space: String) -> fdo::Result<()> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn export(&self, options: String, out: OwnedFd) -> fdo::Result<String> {
        let _ = (options, out);
        Err(crate::introspect::frozen())
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
