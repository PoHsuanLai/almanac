//! The `org.quire.Memory1.Record` object over a [`Serve`] handler.

use super::{Serve, first, forward, none, shape, text, two};
use almanac_dbus::{Iface, MemoryError};
use std::sync::Arc;
use zbus::message::Header;

/// `org.quire.Memory1.Record`.
#[derive(Debug)]
pub struct RecordObject<S>(pub(super) Arc<S>);

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

    async fn record_durable(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        record: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(record)];
        first(forward(&*self.0, &h, Iface::Record, "RecordDurable", args, None).await?)
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
