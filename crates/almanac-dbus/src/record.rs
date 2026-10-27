//! `org.quire.Memory1.Record`: writers. Bodies are the serde JSON of the `almanac-core` types
//! in `s` arguments.

/// The caller's side.
#[zbus::proxy(
    interface = "org.quire.Memory1.Record",
    default_service = "org.quire.Memory1",
    default_path = "/org/quire/Memory1"
)]
pub trait Record {
    /// Records one event (`Record` JSON); answers the `EventRef` JSON.
    fn record(&self, space: &str, record: &str) -> zbus::Result<String>;
    /// Records one event (`Record` JSON) and answers the `Ack` JSON only once it is durable; a
    /// locked, full, paused or unavailable Space is the bus error of that name.
    fn record_durable(&self, space: &str, record: &str) -> zbus::Result<String>;
    /// Records several in order (`Vec<Record>` JSON); answers the first `EventRef` and the count.
    fn record_batch(&self, space: &str, records: &str) -> zbus::Result<(String, u32)>;
    /// Says why a file changed (`FileWhyClaim` JSON).
    fn explain_file(&self, why: &str) -> zbus::Result<()>;
    /// Marks or unmarks a thing (`MarkRequest` JSON).
    fn mark(&self, mark: &str) -> zbus::Result<()>;
}
