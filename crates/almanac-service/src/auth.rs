//! Who may ask what: the authorisation matrix as a pure function.
//!
//! | Request | App | Router | Cuad | ShellUi |
//! |---|---|---|---|---|
//! | Record, RecordBatch | own things; actor `User{via: self}` or `App{self}` | any actor, never `Memory` bodies | `Area(Cua)` bodies with a `Cua` run actor | no |
//! | ExplainFile, Mark | own | yes | no | yes |
//! | Search, Facts, Related, Provenance, Primer | no | yes (audited as `Memory.Read`) | no | yes |
//! | Propose, PlanForget | no | yes | no | yes |
//! | everything else | no | no | no | yes |
//!
//! The companion has no column: it reaches memory through the router, which calls as `Router`
//! with the Space of the invocation. Untrusted-labelled proposals always land in `pending/`
//! (the fact machine), whoever sends them.

use almanac_core::{
    Actor, AgentRole, AppName, AreaTag, Caller, EventBody, FileWhyClaim, MemoryRequest, Record,
    Refusal,
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
        EventBody::File { .. } | EventBody::Memory { .. } | EventBody::Area(_) => false,
    };
    own_body && is_own_actor(&record.actor, app)
}

fn cuad_may_record(record: &Record) -> bool {
    matches!(&record.body, EventBody::Area(p) if p.area == AreaTag::Cua)
        && matches!(
            &record.actor,
            Actor::Companion {
                role: AgentRole::Cua { .. },
                ..
            }
        )
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

/// May `caller` send `request`? Pure; the transport derived `caller`.
pub fn allowed(caller: &Caller, request: &MemoryRequest) -> Allowed {
    use MemoryRequest as R;
    let router_or_shell = matches!(caller, Caller::Router | Caller::ShellUi);
    match request {
        R::Record(record) => yes_if(may_record(caller, record)),
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
        | R::Propose(..)
        | R::PlanForget(..) => yes_if(router_or_shell),
        R::Spaces
        | R::Status(_)
        | R::Timeline(..)
        | R::Forget(_)
        | R::Pending(_)
        | R::Settle(..)
        | R::Consolidation(_)
        | R::RunConsolidation(_)
        | R::Revert(_)
        | R::Rules
        | R::SetRule(_)
        | R::RemoveRule(_)
        | R::Pause(..)
        | R::Resume(_)
        | R::Verify(_)
        | R::Rebuild(_)
        | R::Export(_) => yes_if(matches!(caller, Caller::ShellUi)),
    }
}
