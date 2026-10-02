//! "Do not remember" and retention: the rules the person sets and the defaults under them.

use crate::ids::{DayCount, KindPattern, PathGlob, RuleId};
use crate::thing::ThingRef;
use porter_core::{AppName, SpaceId};
use prov::ActorKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The actor classes a rule can name.
pub type ActorClass = ActorKind;

/// How long something is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Retention {
    /// This many days from when it happened.
    Days(DayCount),
    /// Until the thing it is about is deleted.
    WhileSourceExists,
    /// Until the person forgets it.
    UntilForgotten,
}

/// What a rule does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RememberMode {
    /// Keep the event whole.
    Full,
    /// Keep the chained header, never the body.
    HeaderOnly,
    /// Do not record it at all.
    Never,
}

/// What a rule covers, most specific first: `Thing > Path > Kind > App > Actor > Space`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum RuleScope {
    /// One whole Space.
    Space(SpaceId),
    /// One app's events.
    App(AppName),
    /// Events of these kinds (event kind tags or thing kinds).
    Kind(KindPattern),
    /// File events under a path.
    Path(PathGlob),
    /// Events about one thing.
    Thing(ThingRef),
    /// Events by one class of actor.
    Actor(ActorClass),
}

impl RuleScope {
    /// How specific the scope is; the higher wins.
    pub fn specificity(&self) -> u8 {
        match self {
            RuleScope::Thing(_) => 5,
            RuleScope::Path(_) => 4,
            RuleScope::Kind(_) => 3,
            RuleScope::App(_) => 2,
            RuleScope::Actor(_) => 1,
            RuleScope::Space(_) => 0,
        }
    }
}

/// One rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RememberRule {
    /// Its id.
    pub id: RuleId,
    /// What it covers.
    pub scope: RuleScope,
    /// What it does.
    pub mode: RememberMode,
    /// How long what it keeps is kept.
    pub retention: Retention,
}

/// The default body retention of one kind pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KindRetention {
    /// The kinds.
    pub kind: KindPattern,
    /// How long bodies of these kinds are kept.
    pub retention: Retention,
}

/// Every rule and the defaults beneath them: the contents of `memory.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RuleSet {
    /// The person's rules.
    pub rules: Vec<RememberRule>,
    /// Default body retention by kind; the narrowest matching pattern wins.
    pub defaults: Vec<KindRetention>,
}

/// Unexplained file changes are kept this long (header and body).
pub const UNEXPLAINED_FILE_DAYS: DayCount = DayCount(7);
/// Headers of audit events outlive their bodies by this long.
pub const HEADER_DAYS: DayCount = DayCount(365);
/// A proposed fact nobody settled is removed after this long.
pub const PENDING_TTL_DAYS: DayCount = DayCount(14);
/// Used when no default matches.
pub const FALLBACK_DAYS: DayCount = DayCount(30);

impl RuleSet {
    /// The shipped defaults (design/22 `memory.retention.*`), with no rules.
    pub fn standard() -> RuleSet {
        let days = |kind: &str, n: u32| KindRetention {
            kind: KindPattern::parse(kind).unwrap_or_else(|_| KindPattern::any()),
            retention: Retention::Days(DayCount(n)),
        };
        let while_source = |kind: &str| KindRetention {
            kind: KindPattern::parse(kind).unwrap_or_else(|_| KindPattern::any()),
            retention: Retention::WhileSourceExists,
        };
        RuleSet {
            rules: Vec::new(),
            defaults: vec![
                days("search.*", 30),
                while_source("thing.*"),
                while_source("file.*"),
                days("session.*", 30),
                days("cua.*", 30),
                days("policy.*", 90),
                days("consent.*", 90),
                days("memory.*", 90),
            ],
        }
    }
}

/// What admission decided about one record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Admission {
    /// Record it whole.
    Keep {
        /// For how long.
        retention: Retention,
    },
    /// Record the header; discard the body.
    HeaderOnly {
        /// For how long.
        retention: Retention,
    },
    /// Do not record it.
    Drop(DropReason),
}

/// Why a record was not kept.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum DropReason {
    /// Memory is paused.
    Paused,
    /// A `Never` rule.
    Never(RuleId),
    /// The thing is marked "do not remember".
    ThingMarked,
    /// The Space is locked and its buffer is full.
    SpaceLocked,
    /// The caller may not record this.
    CallerNotAllowed,
    /// There is no such Space (or it is being deleted).
    SpaceUnknown,
}

/// Things marked "do not remember" (by the person or their app).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Marks {
    /// The marked things.
    pub things: BTreeSet<ThingRef>,
}
