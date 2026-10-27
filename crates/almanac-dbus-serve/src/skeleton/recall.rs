//! The frozen skeleton of `org.quire.Memory1.Recall`: its signatures, the source of the introspection.

use zbus::fdo;

/// The daemon's side.
#[derive(Debug, Default)]
pub(crate) struct RecallSkeleton;

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

    fn inject(&self, space: String, query: String) -> fdo::Result<String> {
        let _ = (space, query);
        Err(crate::introspect::frozen())
    }

    fn recent(&self, space: String, query: String) -> fdo::Result<String> {
        let _ = (space, query);
        Err(crate::introspect::frozen())
    }

    fn entries(&self, space: String, query: String) -> fdo::Result<String> {
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
