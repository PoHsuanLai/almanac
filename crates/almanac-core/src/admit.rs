//! Admission: the pure rule that decides whether a record is kept, kept as a header, or
//! dropped.

use crate::event::{EventBody, Record};
use crate::file::{FileView, FileWhy};
use crate::ids::DayCount;
use crate::rules::{
    Admission, DropReason, FALLBACK_DAYS, Marks, RememberMode, RememberRule, Retention, RuleScope,
    RuleSet, UNEXPLAINED_FILE_DAYS,
};
use crate::space::SpaceState;
use prov::{Actor, ActorKind, SystemPart};

/// Whether the record is part of the audit trail: the companion's, a computer-use run's, a
/// terminal's (`quire-do` cannot tell the person from an agent typing in it, so what it does is
/// audited like an agent's), the router's, every other area's payloads, and every message and
/// episode (so a pause keeps headers only). Such records are never dropped for policy reasons
/// (pause, `Never` rules, marks); at worst they keep their header.
pub fn is_audit_class(record: &Record) -> bool {
    let by_actor = matches!(
        record.actor.kind(),
        ActorKind::Companion | ActorKind::Cua | ActorKind::Cli
    ) || matches!(
        record.actor,
        Actor::System {
            part: SystemPart::Router | SystemPart::Cua
        }
    );
    by_actor
        || matches!(
            record.body,
            EventBody::Area(_) | EventBody::Message(_) | EventBody::Episode(_)
        )
}

/// Decides what to do with `record`. Pure.
///
/// A Space that cannot record (`Locked`, `Deleting`, `Gone`) drops everything: those are
/// availability limits, which the service handles by buffering before it asks. Otherwise
/// pause, marks and rules apply; the most specific scope wins (`Thing > Path > Kind > App >
/// Actor > Space`) and at equal specificity the strictest mode (`Never > HeaderOnly > Full`).
pub fn admit(record: &Record, rules: &RuleSet, state: &SpaceState, marks: &Marks) -> Admission {
    admit_with(record, rules, state, marks, UNEXPLAINED_FILE_DAYS)
}

/// [`admit`] with the person's keep for unexplained file changes (`memory.retention.
/// file_unexplained_days`) in place of the shipped 7 days.
pub fn admit_with(
    record: &Record,
    rules: &RuleSet,
    state: &SpaceState,
    marks: &Marks,
    unexplained: DayCount,
) -> Admission {
    let audit = is_audit_class(record);
    let retention = default_retention_with(record, rules, unexplained);
    let refuse = |reason: DropReason| {
        if audit {
            Admission::HeaderOnly { retention }
        } else {
            Admission::Drop(reason)
        }
    };
    match state {
        SpaceState::Locked => return Admission::Drop(DropReason::SpaceLocked),
        SpaceState::Deleting | SpaceState::Gone => {
            return Admission::Drop(DropReason::SpaceUnknown);
        }
        SpaceState::Paused { .. } => return refuse(DropReason::Paused),
        SpaceState::Open => {}
    }
    if record
        .body
        .things()
        .iter()
        .any(|(view, _)| marks.things.contains(&view.thing))
    {
        return refuse(DropReason::ThingMarked);
    }
    match winning_rule(record, rules) {
        None => Admission::Keep { retention },
        Some(rule) => match rule.mode {
            RememberMode::Full => Admission::Keep {
                retention: rule.retention,
            },
            RememberMode::HeaderOnly => Admission::HeaderOnly {
                retention: rule.retention,
            },
            RememberMode::Never => refuse(DropReason::Never(rule.id.clone())),
        },
    }
}

/// Why `record` would not be kept whole, if it would not: the reason `admit` would give a
/// non-audit record (`Drop`), whether or not this one is audit class and so would keep a header.
/// `None` means [`admit`] answers `Keep`. A durable append (`RecordDurable`) refuses on any
/// `Some`, because an acknowledgement promises the body is stored, and an audit-class record
/// under a pause or a `Never` rule keeps only its header. Pure.
pub fn withheld(
    record: &Record,
    rules: &RuleSet,
    state: &SpaceState,
    marks: &Marks,
) -> Option<DropReason> {
    match state {
        SpaceState::Locked => return Some(DropReason::SpaceLocked),
        SpaceState::Deleting | SpaceState::Gone => return Some(DropReason::SpaceUnknown),
        SpaceState::Paused { .. } => return Some(DropReason::Paused),
        SpaceState::Open => {}
    }
    if record
        .body
        .things()
        .iter()
        .any(|(view, _)| marks.things.contains(&view.thing))
    {
        return Some(DropReason::ThingMarked);
    }
    winning_rule(record, rules).and_then(|rule| match rule.mode {
        RememberMode::Full => None,
        RememberMode::HeaderOnly => Some(DropReason::HeaderOnlyRule(rule.id.clone())),
        RememberMode::Never => Some(DropReason::Never(rule.id.clone())),
    })
}

/// How long the record's body is kept when no rule says: unexplained file changes 7 days,
/// otherwise the narrowest matching default, otherwise 30 days.
pub fn default_retention(record: &Record, rules: &RuleSet) -> Retention {
    default_retention_with(record, rules, UNEXPLAINED_FILE_DAYS)
}

/// [`default_retention`] with the person's keep for unexplained file changes.
pub fn default_retention_with(
    record: &Record,
    rules: &RuleSet,
    unexplained: DayCount,
) -> Retention {
    if matches!(
        record.body,
        EventBody::File {
            why: FileWhy::Unexplained,
            ..
        }
    ) {
        return Retention::Days(unexplained);
    }
    let kind = record.body.kind();
    rules
        .defaults
        .iter()
        .filter(|d| d.kind.covers(kind.as_str()))
        .max_by_key(|d| d.kind.specificity())
        .map_or(Retention::Days(FALLBACK_DAYS), |d| d.retention)
}

fn winning_rule<'a>(record: &Record, rules: &'a RuleSet) -> Option<&'a RememberRule> {
    let applicable: Vec<&RememberRule> = rules
        .rules
        .iter()
        .filter(|r| applies(&r.scope, record))
        .collect();
    let top = applicable.iter().map(|r| r.scope.specificity()).max()?;
    applicable
        .into_iter()
        .filter(|r| r.scope.specificity() == top)
        .fold(None, |best: Option<&RememberRule>, r| match best {
            Some(b) if b.mode >= r.mode => Some(b),
            _ => Some(r),
        })
}

fn applies(scope: &RuleScope, record: &Record) -> bool {
    match scope {
        RuleScope::Space(space) => &record.space == space,
        RuleScope::Actor(class) => record.actor.kind() == *class,
        RuleScope::App(app) => app_of(record).iter().any(|a| a == app),
        RuleScope::Kind(pattern) => {
            pattern.covers(record.body.kind().as_str())
                || record
                    .body
                    .things()
                    .iter()
                    .any(|(view, _)| pattern.covers(view.thing.kind.as_str()))
        }
        RuleScope::Path(glob) => match &record.body {
            EventBody::File {
                file: FileView { path, .. },
                ..
            } => glob_matches(glob.as_str(), path.as_str()),
            _ => false,
        },
        RuleScope::Thing(thing) => record.body.names(thing),
    }
}

/// The apps an event involves: its things' owners, the app searched in, the acting app.
fn app_of(record: &Record) -> Vec<porter_core::AppName> {
    let mut apps: Vec<porter_core::AppName> = record
        .body
        .things()
        .iter()
        .map(|(v, _)| v.thing.app.clone())
        .collect();
    if let EventBody::Search { app, .. } = &record.body {
        apps.push(app.clone());
    }
    match &record.actor {
        Actor::User { via: app } | Actor::App { app } | Actor::ThirdParty { app, .. } => {
            apps.push(app.clone());
        }
        // No app acts: a terminal (`Cli`) is not an app, so an App-scoped rule never matches it
        // by actor (it still matches the things and the search the event is about).
        Actor::Companion { .. }
        | Actor::Mcp { .. }
        | Actor::Cli
        | Actor::System { .. }
        | Actor::Unknown => {}
    }
    apps
}

/// `*` matches within one path element, `**` across elements; everything else is literal.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some((b'*', rest)) => match rest.split_first() {
                Some((b'*', deep)) => (0..=t.len()).any(|i| go(deep, &t[i..])),
                _ => {
                    for i in 0..=t.len() {
                        if go(rest, &t[i..]) {
                            return true;
                        }
                        if t.get(i) == Some(&b'/') {
                            break;
                        }
                    }
                    false
                }
            },
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}
