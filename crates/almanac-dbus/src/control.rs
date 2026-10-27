//! `org.quire.Memory1.Control`: the person's controls (the shell UI only), the signals, and
//! the wire `Version`.

use std::collections::HashMap;

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
    /// Applies a proposed run (`RunId` JSON).
    fn apply_consolidation(&self, run: &str) -> zbus::Result<()>;
    /// Discards a proposed run (`RunId` JSON); it stays on disk, marked discarded.
    fn discard_consolidation(&self, run: &str) -> zbus::Result<()>;
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
    /// Runs the retention sweep now (answers `SweepReport` JSON).
    fn sweep(&self, space: &str) -> zbus::Result<String>;
    /// Settles a removed desktop-wide Space's memories and history (`Removal` JSON, the memories and the history fate; answers `Relocation` JSON).
    fn remove_space(&self, space: &str, removal: &str) -> zbus::Result<String>;
    /// Writes a tar export to `out` (`ExportOptions` JSON; answers `ExportManifest` JSON).
    fn export(&self, options: &str, out: zbus::zvariant::Fd<'_>) -> zbus::Result<String>;
    /// The person confirmed these loosened memory settings in the Settings app (setting key to
    /// value). The Settings app's alone: until then a value looser than the shipped default waits.
    fn confirm_settings(&self, values: HashMap<String, String>) -> zbus::Result<()>;
    /// The loose memory settings the file holds that wait for confirmation: each one's key, the
    /// value waiting and the shipped value in force, in key order. The Settings app's and the
    /// shell's.
    fn pending_settings(&self) -> zbus::Result<Vec<(String, String, String)>>;
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
    /// Whether the daemon keeps itself to its own folders: `confined`, `partial`,
    /// `not-available` or (in a test build only) `off`.
    #[zbus(property)]
    fn confinement(&self) -> zbus::Result<String>;
    /// The wire version.
    #[zbus(property)]
    fn version(&self) -> zbus::Result<u32>;
}
