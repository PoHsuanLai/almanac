//! Who may ask what: the authorisation matrix as a pure function.
//!
//! | Request | App | Router | Cuad | ShellUi |
//! |---|---|---|---|---|
//! | Record, RecordBatch | own things; actor `User{via: self}` or `App{self}` | any actor, never `Memory` bodies | `Area(Cua)` bodies with a `Cua` run actor; `Message` bodies sent by that same run | no |
//! | ExplainFile, Mark | own | yes | no | yes |
//! | Search, Facts, Related, Provenance, Primer, Inject, Recent, Entries, Spaces | no | yes (audited as `Memory.Read`) | no | yes |
//! | Propose, PlanForget | no | yes | no | yes |
//! | everything else (with `RemoveSpace`) | no | no | no | yes |
//!
//! An app also reaches only Spaces open to it (`SpaceId::owner`): another app's own Space is
//! `OutsideSpace` whatever the request.
//!
//! `Message` and `Episode` bodies are the router's to record (it stamps the sender); an app never
//! records either, and `Timeline` stays the shell's alone: the router reads recent activity
//! through `Recent`, with `BodyMode::Json` the same read plus each body, which travels in its
//! entry beside the entry's label (no extra authority: the caller already reads text and labels).
//! A message grants no read in the other Space: `Search`, `Inject` and `Recent`
//! take the Space of the invocation, whoever wrote to the task.
//!
//! The companion has no column: it reaches memory through the router, which calls as `Router`
//! with the Space of the invocation. Untrusted-labelled proposals always land in `pending/`
//! (the fact machine), whoever sends them.

use almanac_core::{
    Actor, AgentRef, AgentRole, AppName, AreaTag, Caller, EventBody, FileWhyClaim, MemoryRequest,
    Record, Refusal, SpaceId, SpaceScope,
};

/// The answer of [`allowed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowed {
    /// Go ahead.
    Yes,
    /// Refuse with this.
    No(Refusal),
}

const NO: Allowed = Allowed::No(Refusal::NotAllowed);

fn yes_if(ok: bool) -> Allowed {
    if ok { Allowed::Yes } else { NO }
}

fn is_own_actor(actor: &Actor, app: &AppName) -> bool {
    matches!(actor, Actor::User { via } if via == app)
        || matches!(actor, Actor::App { app: a } if a == app)
}

/// Whether an app may record `record`: its own things (or searches) only, as itself.
fn app_may_record(app: &AppName, record: &Record) -> bool {
    let own_body = match &record.body {
        EventBody::Thing { .. } => record
            .body
            .things()
            .iter()
            .all(|(view, _)| &view.thing.app == app),
        EventBody::Search { app: searched, .. } => searched == app,
        EventBody::File { .. }
        | EventBody::Memory { .. }
        | EventBody::Message(_)
        | EventBody::Episode(_)
        | EventBody::Area(_) => false,
        // A body kind this build does not know is not the app's own: refuse.
        _ => false,
    };
    own_body && is_own_actor(&record.actor, app)
}

fn cuad_may_record(record: &Record) -> bool {
    let run = match &record.actor {
        Actor::Companion {
            role: AgentRole::Cua { run },
            ..
        } => run,
        _ => return false,
    };
    match &record.body {
        EventBody::Area(p) => p.area == AreaTag::Cua,
        // A run's own report, as itself.
        EventBody::Message(m) => m.from.agent == AgentRef::Cua { run: run.clone() },
        EventBody::Thing { .. }
        | EventBody::File { .. }
        | EventBody::Search { .. }
        | EventBody::Memory { .. }
        | EventBody::Episode(_) => false,
        // A body kind this build does not know: refuse.
        _ => false,
    }
}

fn router_may_record(record: &Record) -> bool {
    !matches!(record.body, EventBody::Memory { .. })
}

fn may_record(caller: &Caller, record: &Record) -> bool {
    match caller {
        Caller::App(id) => app_may_record(&id.name, record),
        Caller::Router => router_may_record(record),
        Caller::Cuad => cuad_may_record(record),
        Caller::ShellUi => false,
    }
}

fn app_may_explain(app: &AppName, claim: &FileWhyClaim) -> bool {
    &claim.cause.app == app && is_own_actor(&claim.by, app)
}

/// May `caller` send `request`? Pure; the transport derived `caller`. A request that reaches
/// into another app's own Space is `OutsideSpace` before anything else is asked.
pub fn allowed(caller: &Caller, request: &MemoryRequest) -> Allowed {
    match space_is_open(caller, request) {
        Allowed::Yes => by_class(caller, request),
        refused => refused,
    }
}

fn by_class(caller: &Caller, request: &MemoryRequest) -> Allowed {
    use MemoryRequest as R;
    let router_or_shell = matches!(caller, Caller::Router | Caller::ShellUi);
    match request {
        R::Record(record) | R::RecordDurable(record) => yes_if(may_record(caller, record)),
        R::RecordBatch(records) if records.is_empty() => {
            Allowed::No(Refusal::Invalid("empty batch".to_owned()))
        }
        R::RecordBatch(records) => yes_if(records.iter().all(|r| may_record(caller, r))),
        R::ExplainFile(claim) => match caller {
            Caller::App(id) => yes_if(app_may_explain(&id.name, claim)),
            Caller::Router | Caller::ShellUi => Allowed::Yes,
            Caller::Cuad => NO,
        },
        R::Mark(mark) => match caller {
            Caller::App(id) => yes_if(mark.thing.app == id.name),
            Caller::Router | Caller::ShellUi => Allowed::Yes,
            Caller::Cuad => NO,
        },
        R::Search(_)
        | R::Facts(_)
        | R::Related(..)
        | R::Provenance(..)
        | R::Primer(_)
        | R::Inject(_)
        | R::Recent(..)
        | R::Entries(..)
        | R::Propose(..)
        | R::PlanForget(..)
        | R::Spaces => yes_if(router_or_shell),
        R::Status(_)
        | R::Timeline(..)
        | R::Forget(_)
        | R::Pending(_)
        | R::Settle(..)
        | R::Consolidation(_)
        | R::RunConsolidation(_)
        | R::Revert(_)
        | R::ApplyConsolidation(_)
        | R::DiscardConsolidation(_)
        | R::Rules
        | R::SetRule(_)
        | R::RemoveRule(_)
        | R::Pause(..)
        | R::Resume(_)
        | R::Verify(_)
        | R::Rebuild(_)
        | R::Sweep(_)
        | R::Export(_)
        | R::RemoveSpace(..) => yes_if(matches!(caller, Caller::ShellUi)),
        // A request kind this build does not know is not allowed to anyone.
        _ => NO,
    }
}

/// The Spaces `request` names, in order: `RecordBatch` names one per record.
fn spaces_of(request: &MemoryRequest) -> Vec<&SpaceId> {
    match request {
        MemoryRequest::RecordBatch(records) => records.iter().map(|r| &r.space).collect(),
        other => other.space().into_iter().collect(),
    }
}

/// An app reaches only Spaces open to it: its own App Spaces, the desktop and the desktop-wide
/// Spaces (porter's `SpaceScope::open_to`). The name is the caller's, never read from the id.
/// Router, cuad and the shell are not apps and act across Spaces.
fn space_is_open(caller: &Caller, request: &MemoryRequest) -> Allowed {
    match caller {
        Caller::App(id) => {
            let open = spaces_of(request)
                .into_iter()
                .all(|space| SpaceScope::Only(space.clone()).open_to(&id.name));
            match open {
                true => Allowed::Yes,
                false => Allowed::No(Refusal::OutsideSpace),
            }
        }
        Caller::Router | Caller::Cuad | Caller::ShellUi => Allowed::Yes,
    }
}
