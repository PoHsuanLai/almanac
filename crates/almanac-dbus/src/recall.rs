//! `org.quire.Memory1.Recall`: readers and proposals. Callers are the action router and the
//! shell; every call is audited as `Memory.Read`.

use zbus::fdo;

/// The caller's side.
#[zbus::proxy(
    interface = "org.quire.Memory1.Recall",
    default_service = "org.quire.Memory1",
    default_path = "/org/quire/Memory1"
)]
pub trait Recall {
    /// Search (`RecallQuery` JSON); answers `Vec<RecallHit>` JSON.
    fn search(&self, space: &str, query: &str) -> zbus::Result<String>;
    /// Facts (`FactQuery` JSON); answers `Vec<FactView>` JSON.
    fn facts(&self, space: &str, query: &str) -> zbus::Result<String>;
    /// Events related to a thing (`ThingRef` JSON); answers `Vec<EventSummary>` JSON.
    fn related(&self, space: &str, thing: &str) -> zbus::Result<String>;
    /// Where a file came from (`SpacePath` JSON); answers `FileProvenance` JSON.
    fn provenance(&self, space: &str, path: &str) -> zbus::Result<String>;
    /// The primer, markdown.
    fn primer(&self, space: &str) -> zbus::Result<String>;
    /// Proposes a fact (`FactDraft` JSON); answers the `FactId` and the `FactState` JSON.
    fn propose(&self, space: &str, draft: &str) -> zbus::Result<(String, String)>;
}

/// The daemon's side.
#[derive(Debug, Default)]
pub struct RecallSkeleton;

#[zbus::interface(name = "org.quire.Memory1.Recall")]
impl RecallSkeleton {
    fn search(&self, space: String, query: String) -> fdo::Result<String> {
        let _ = (space, query);
        Err(crate::introspect::frozen())
    }

    fn facts(&self, space: String, query: String) -> fdo::Result<String> {
        let _ = (space, query);
        Err(crate::introspect::frozen())
    }

    fn related(&self, space: String, thing: String) -> fdo::Result<String> {
        let _ = (space, thing);
        Err(crate::introspect::frozen())
    }

    fn provenance(&self, space: String, path: String) -> fdo::Result<String> {
        let _ = (space, path);
        Err(crate::introspect::frozen())
    }

    fn primer(&self, space: String) -> fdo::Result<String> {
        let _ = space;
        Err(crate::introspect::frozen())
    }

    #[zbus(out_args("fact_id", "state"))]
    fn propose(&self, space: String, draft: String) -> fdo::Result<(String, String)> {
        let _ = (space, draft);
        Err(crate::introspect::frozen())
    }
}
