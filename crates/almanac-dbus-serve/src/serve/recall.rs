//! The `org.quire.Memory1.Recall` object over a [`Serve`] handler.

use super::{Serve, first, forward, text, two};
use almanac_dbus::{Iface, MemoryError};
use std::sync::Arc;
use zbus::message::Header;

/// `org.quire.Memory1.Recall`.
#[derive(Debug)]
pub struct RecallObject<S>(pub(super) Arc<S>);

#[zbus::interface(name = "org.quire.Memory1.Recall")]
impl<S: Serve> RecallObject<S> {
    async fn search(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Search", args, None).await?)
    }

    async fn facts(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Facts", args, None).await?)
    }

    async fn inject(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Inject", args, None).await?)
    }

    async fn recent(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Recent", args, None).await?)
    }

    async fn entries(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Recall, "Entries", args, None).await?)
    }

    async fn related(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        thing: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(thing)];
        first(forward(&*self.0, &h, Iface::Recall, "Related", args, None).await?)
    }

    async fn provenance(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        path: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(path)];
        first(forward(&*self.0, &h, Iface::Recall, "Provenance", args, None).await?)
    }

    async fn primer(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Recall,
                "Primer",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    #[zbus(out_args("fact_id", "state"))]
    async fn propose(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        draft: String,
    ) -> Result<(String, String), MemoryError> {
        let args = vec![text(space), text(draft)];
        two(forward(&*self.0, &h, Iface::Recall, "Propose", args, None).await?)
    }
}
