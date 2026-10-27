//! The frozen skeleton of `org.quire.Memory1.Control`: its signatures, signals and properties, the source of the introspection.

use crate::serve::CONFINEMENT_OFF;
use std::collections::HashMap;
use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedFd;

/// The daemon's side.
#[derive(Debug, Default)]
pub(crate) struct ControlSkeleton;

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

    fn apply_consolidation(&self, run: String) -> fdo::Result<()> {
        let _ = run;
        Err(crate::introspect::frozen())
    }

    fn discard_consolidation(&self, run: String) -> fdo::Result<()> {
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

    fn sweep(&self, space: String) -> fdo::Result<String> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    fn remove_space(&self, space: String, removal: String) -> fdo::Result<String> {
        let _ = (space, removal);
        Err(crate::introspect::frozen())
    }

    fn export(&self, options: String, out: OwnedFd) -> fdo::Result<String> {
        let _ = (options, out);
        Err(crate::introspect::frozen())
    }

    fn confirm_settings(&self, values: HashMap<String, String>) -> fdo::Result<()> {
        let _ = values;
        Err(crate::introspect::frozen())
    }

    fn pending_settings(&self) -> fdo::Result<Vec<(String, String, String)>> {
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
    fn confinement(&self) -> String {
        CONFINEMENT_OFF.to_owned()
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        almanac_core::MEMORY_WIRE_VERSION
    }
}
