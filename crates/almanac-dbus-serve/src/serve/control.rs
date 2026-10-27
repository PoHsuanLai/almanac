//! The `org.quire.Memory1.Control` object over a [`Serve`] handler, with its signals and
//! properties.

use super::{Serve, first, forward, none, text};
use almanac_core::WaitingSetting;
use almanac_dbus::{CallArg, Iface, MemoryError};
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedFd as BusFd;

/// `org.quire.Memory1.Control`.
#[derive(Debug)]
pub struct ControlObject<S>(pub(super) Arc<S>);

#[zbus::interface(name = "org.quire.Memory1.Control")]
impl<S: Serve> ControlObject<S> {
    async fn spaces(&self, #[zbus(header)] h: Header<'_>) -> Result<String, MemoryError> {
        first(forward(&*self.0, &h, Iface::Control, "Spaces", vec![], None).await?)
    }

    async fn status(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Status",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn timeline(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        query: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(query)];
        first(forward(&*self.0, &h, Iface::Control, "Timeline", args, None).await?)
    }

    async fn plan_forget(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        scope: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space), text(scope)];
        first(forward(&*self.0, &h, Iface::Control, "PlanForget", args, None).await?)
    }

    async fn forget(
        &self,
        #[zbus(header)] h: Header<'_>,
        token: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Forget",
                vec![text(token)],
                None,
            )
            .await?,
        )
    }

    async fn pending(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Pending",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn settle(
        &self,
        #[zbus(header)] h: Header<'_>,
        fact: String,
        verdict: String,
    ) -> Result<(), MemoryError> {
        let args = vec![text(fact), text(verdict)];
        none(forward(&*self.0, &h, Iface::Control, "Settle", args, None).await?)
    }

    async fn consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        let args = vec![text(space)];
        first(forward(&*self.0, &h, Iface::Control, "Consolidation", args, None).await?)
    }

    async fn run_consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<(), MemoryError> {
        let args = vec![text(space)];
        none(forward(&*self.0, &h, Iface::Control, "RunConsolidation", args, None).await?)
    }

    async fn revert(&self, #[zbus(header)] h: Header<'_>, run: String) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Revert",
                vec![text(run)],
                None,
            )
            .await?,
        )
    }

    async fn apply_consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        run: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "ApplyConsolidation",
                vec![text(run)],
                None,
            )
            .await?,
        )
    }

    async fn discard_consolidation(
        &self,
        #[zbus(header)] h: Header<'_>,
        run: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "DiscardConsolidation",
                vec![text(run)],
                None,
            )
            .await?,
        )
    }

    async fn rules(&self, #[zbus(header)] h: Header<'_>) -> Result<String, MemoryError> {
        first(forward(&*self.0, &h, Iface::Control, "Rules", vec![], None).await?)
    }

    async fn set_rule(
        &self,
        #[zbus(header)] h: Header<'_>,
        rule: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "SetRule",
                vec![text(rule)],
                None,
            )
            .await?,
        )
    }

    async fn remove_rule(
        &self,
        #[zbus(header)] h: Header<'_>,
        id: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "RemoveRule",
                vec![text(id)],
                None,
            )
            .await?,
        )
    }

    async fn pause(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        until: u64,
    ) -> Result<(), MemoryError> {
        let args = vec![text(space), CallArg::Seconds(until)];
        none(forward(&*self.0, &h, Iface::Control, "Pause", args, None).await?)
    }

    async fn resume(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Resume",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn verify(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Verify",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn rebuild(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<(), MemoryError> {
        none(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Rebuild",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn sweep(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Sweep",
                vec![text(space)],
                None,
            )
            .await?,
        )
    }

    async fn remove_space(
        &self,
        #[zbus(header)] h: Header<'_>,
        space: String,
        removal: String,
    ) -> Result<String, MemoryError> {
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "RemoveSpace",
                vec![text(space), text(removal)],
                None,
            )
            .await?,
        )
    }

    async fn export(
        &self,
        #[zbus(header)] h: Header<'_>,
        options: String,
        out: BusFd,
    ) -> Result<String, MemoryError> {
        let fd = OwnedFd::from(out);
        first(
            forward(
                &*self.0,
                &h,
                Iface::Control,
                "Export",
                vec![text(options)],
                Some(fd),
            )
            .await?,
        )
    }

    async fn confirm_settings(
        &self,
        #[zbus(header)] h: Header<'_>,
        values: HashMap<String, String>,
    ) -> Result<(), MemoryError> {
        let args = vec![CallArg::Map(values.into_iter().collect())];
        none(forward(&*self.0, &h, Iface::Control, "ConfirmSettings", args, None).await?)
    }

    async fn pending_settings(
        &self,
        #[zbus(header)] h: Header<'_>,
    ) -> Result<Vec<(String, String, String)>, MemoryError> {
        let outputs = forward(
            &*self.0,
            &h,
            Iface::Control,
            "PendingSettings",
            vec![],
            None,
        )
        .await?;
        let waiting: Vec<WaitingSetting> = serde_json::from_str(&first(outputs)?)
            .map_err(|e| MemoryError::Invalid(e.to_string()))?;
        Ok(waiting
            .into_iter()
            .map(|w| (w.key, w.waiting, w.shipped))
            .collect())
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
        self.0.confinement().to_owned()
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        almanac_core::MEMORY_WIRE_VERSION
    }
}
