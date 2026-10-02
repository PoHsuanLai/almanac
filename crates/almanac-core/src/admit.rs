//! Admission: the pure rule that decides whether a record is kept, kept as a header, or
//! dropped.

use crate::event::{EventBody, Record};
use crate::file::{FileView, FileWhy};
use crate::rules::{
    Admission, DropReason, FALLBACK_DAYS, Marks, RememberMode, RememberRule, Retention, RuleScope,
    RuleSet, UNEXPLAINED_FILE_DAYS,
};
use crate::space::SpaceState;
use prov::{Actor, ActorKind, SystemPart};

/// Whether the record is part of the audit trail: the companion's, the router's and every
/// other area's payloads. Such records are never dropped for policy reasons (pause, `Never`
/// rules, marks); at worst they keep their header.
pub fn is_audit_class(record: &Record) -> bool {
    let by_actor = matches!(record.actor.kind(), ActorKind::Companion | ActorKind::Cua)
        || matches!(
            record.actor,
            Actor::System {
                part: SystemPart::Router | SystemPart::Cua
            }
        );
    by_actor || matches!(record.body, EventBody::Area(_))
}

/// Decides what to do with `record`. Pure.
///
/// A Space that cannot record (`Locked`, `Deleting`, `Gone`) drops everything: those are
/// availability limits, which the service handles by buffering before it asks. Otherwise
/// pause, marks and rules apply; the most specific scope wins (`Thing > Path > Kind > App >
/// Actor > Space`) and at equal specificity the strictest mode (`Never > HeaderOnly > Full`).
pub fn admit(record: &Record, rules: &RuleSet, state: &SpaceState, marks: &Marks) -> Admission {
    let audit = is_audit_class(record);
    let retention = default_retention(record, rules);
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

/// How long the record's body is kept when no rule says: unexplained file changes 7 days,
/// otherwise the narrowest matching default, otherwise 30 days.
pub fn default_retention(record: &Record, rules: &RuleSet) -> Retention {
    if matches!(
        record.body,
        EventBody::File {
            why: FileWhy::Unexplained,
            ..
        }
    ) {
        return Retention::Days(UNEXPLAINED_FILE_DAYS);
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
        _ => {}
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
