//! `org.quire.Memory1.Record`: writers. Bodies are the serde JSON of the `almanac-core` types
//! in `s` arguments.

use zbus::fdo;

/// The caller's side.
#[zbus::proxy(
    interface = "org.quire.Memory1.Record",
    default_service = "org.quire.Memory1",
    default_path = "/org/quire/Memory1"
)]
pub trait Record {
    /// Records one event (`Record` JSON); answers the `EventRef` JSON.
    fn record(&self, space: &str, record: &str) -> zbus::Result<String>;
    /// Records several in order (`Vec<Record>` JSON); answers the first `EventRef` and the count.
    fn record_batch(&self, space: &str, records: &str) -> zbus::Result<(String, u32)>;
    /// Says why a file changed (`FileWhyClaim` JSON).
    fn explain_file(&self, why: &str) -> zbus::Result<()>;
    /// Marks or unmarks a thing (`MarkRequest` JSON).
    fn mark(&self, mark: &str) -> zbus::Result<()>;
}

/// The daemon's side.
#[derive(Debug, Default)]
pub struct RecordSkeleton;

#[zbus::interface(name = "org.quire.Memory1.Record")]
impl RecordSkeleton {
    fn record(&self, space: String, record: String) -> fdo::Result<String> {
        let _ = (space, record);
        Err(crate::introspect::frozen())
    }

    #[zbus(out_args("first_ref", "count"))]
    fn record_batch(&self, space: String, records: String) -> fdo::Result<(String, u32)> {
        let _ = (space, records);
        Err(crate::introspect::frozen())
    }

    fn explain_file(&self, why: String) -> fdo::Result<()> {
        let _ = why;
        Err(crate::introspect::frozen())
    }

    fn mark(&self, mark: String) -> fdo::Result<()> {
        let _ = mark;
        Err(crate::introspect::frozen())
    }
}
