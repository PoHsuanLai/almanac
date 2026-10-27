//! The frozen skeleton of `org.quire.Memory1.Record`: its signatures, the source of the introspection.

use zbus::fdo;

/// The daemon's side.
#[derive(Debug, Default)]
pub(crate) struct RecordSkeleton;

#[zbus::interface(name = "org.quire.Memory1.Record")]
impl RecordSkeleton {
    fn record(&self, space: String, record: String) -> fdo::Result<String> {
        let _ = (space, record);
        Err(crate::introspect::frozen())
    }

    fn record_durable(&self, space: String, record: String) -> fdo::Result<String> {
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
