//! The one rule for "which apps an event involves" and "which kinds a pattern covers".
//!
//! Admission (`never remember app X`) and forgetting (`forget app X`) both read it, so the two
//! always cover the same events.

use crate::event::EventBody;
use crate::ids::KindPattern;
use porter_core::AppName;
use prov::Actor;

impl EventBody {
    /// Whether the event involves `app`: the owner of one of its things, the app searched in,
    /// or the acting app. A terminal, a companion or an external agent is no app.
    pub fn involves_app(&self, actor: &Actor, app: &AppName) -> bool {
        let acting = match actor {
            Actor::User { via: a } | Actor::App { app: a } | Actor::ThirdParty { app: a, .. } => {
                a == app
            }
            Actor::Companion { .. }
            | Actor::Mcp { .. }
            | Actor::Acp { .. }
            | Actor::Cli
            | Actor::System { .. }
            | Actor::Unknown => false,
        };
        let searched = matches!(self, EventBody::Search { app: a, .. } if a == app);
        acting || searched || self.thing_refs().iter().any(|(thing, _)| &thing.app == app)
    }
}

impl KindPattern {
    /// Whether the pattern covers the event's kind or the kind of any thing it names.
    pub fn covers_event(&self, body: &EventBody) -> bool {
        self.covers(body.kind().as_str())
            || body
                .things()
                .iter()
                .any(|(view, _)| self.covers(view.thing.kind.as_str()))
    }
}
