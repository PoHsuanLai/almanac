//! The Secret Service's lock signal. A keyring that locks (screen lock, timeout) or unlocks
//! changes the `Locked` property of its collection, and the service announces it with
//! `org.freedesktop.DBus.Properties.PropertiesChanged`. memoryd listens for that and checks its
//! Spaces' keys then, instead of asking on a timer. The check is idempotent, so a signal from
//! anything else on the bus (the match is by path, not by sender) costs one needless check.

use std::pin::Pin;
use zbus::message::Type;
use zbus::{MatchRule, Message, MessageStream};

/// Where the Secret Service keeps its collections (and items).
const SECRETS_PATH: &str = "/org/freedesktop/secrets";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const LOCKED: &str = "Locked";

/// The signals that may carry a lock change: `PropertiesChanged` under the Secret Service's path.
fn rule() -> MatchRule<'static> {
    MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(PROPERTIES)
        .expect("a fixed interface name")
        .member("PropertiesChanged")
        .expect("a fixed member name")
        .path_namespace(SECRETS_PATH)
        .expect("a fixed path")
        .build()
}

/// Does this message say a collection's `Locked` changed? (Pure over the message body.)
pub fn is_lock_change(message: &Message) -> bool {
    let body = message.body();
    let parsed: Result<
        (
            String,
            std::collections::HashMap<String, zbus::zvariant::Value<'_>>,
            Vec<String>,
        ),
        _,
    > = body.deserialize();
    parsed.is_ok_and(|(interface, changed, invalidated)| {
        interface == COLLECTION
            && (changed.contains_key(LOCKED) || invalidated.iter().any(|name| name == LOCKED))
    })
}

/// The keyring's lock changes on `connection`: one item per lock or unlock. The match rule is
/// installed when this returns, so a change after it is not missed.
#[derive(Debug)]
pub struct LockChanges {
    stream: MessageStream,
}

impl LockChanges {
    /// Subscribes on `connection`.
    pub async fn on(connection: &zbus::Connection) -> zbus::Result<Self> {
        let stream = MessageStream::for_match_rule(rule(), connection, None).await?;
        Ok(Self { stream })
    }

    /// Waits for the next lock change; `None` when the connection is gone.
    pub async fn next(&mut self) -> Option<()> {
        loop {
            let item = std::future::poll_fn(|cx| {
                zbus::export::futures_core::Stream::poll_next(Pin::new(&mut self.stream), cx)
            })
            .await?;
            if item.is_ok_and(|message| is_lock_change(&message)) {
                return Some(());
            }
        }
    }
}
