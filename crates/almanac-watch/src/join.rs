//! The file-why join (memory section 4.6): observed changes meet the reasons apps supply.
//!
//! | Input | Within the window, same path | Outcome |
//! |---|---|---|
//! | Observed + why | content agrees (or the watcher has none) | `Explained`, actor = the why's |
//! | Observed alone | window elapsed | `Unexplained`; actor `App` if the process resolved, else `Unknown` |
//! | Why alone | window elapsed | recorded as the app said; content is the app's digest |
//! | Rename (cookie pair) | | `Renamed { from }` and an alias row |

use crate::observed::{Alias, FileEvent, JoinWindow, Joined, Observed, Stamp, WhyAt};
use almanac_core::{Actor, FileChange, FileView, FileWhy, Verb};

/// What the join is waiting on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pending {
    /// Observations not yet settled.
    pub observed: Vec<Observed>,
    /// Explanations not yet settled.
    pub whys: Vec<WhyAt>,
}

/// The result of one join step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinStep {
    /// Events to record, oldest first.
    pub settled: Vec<Joined>,
    /// What still waits for a partner.
    pub waiting: Pending,
}

/// The change an app's verb implies when no filesystem observation came with it.
pub fn change_for(verb: Verb) -> FileChange {
    match verb {
        Verb::Created | Verb::Downloaded | Verb::Imported | Verb::Copied | Verb::Exported => {
            FileChange::Created
        }
        Verb::Edited | Verb::Saved => FileChange::Modified,
        Verb::Deleted => FileChange::Deleted,
        _ => FileChange::Closed,
    }
}

fn apart(a: Stamp, b: Stamp) -> u64 {
    a.0.abs_diff(b.0)
}

fn elapsed(at: Stamp, now: Stamp, window: JoinWindow) -> bool {
    now.0.saturating_sub(at.0) > i64::from(window.0)
}

fn view(observed: &Observed, content: almanac_core::ContentDigest) -> FileView {
    FileView {
        path: observed.path.clone(),
        inode: observed.inode,
        content,
    }
}

fn alias_of(observed: &Observed) -> Option<Alias> {
    match &observed.change {
        FileChange::Renamed { from } => Some(Alias {
            from: from.clone(),
            to: observed.path.clone(),
        }),
        _ => None,
    }
}

/// The index of the why that explains `observed`: same path, within the window, content equal
/// (or unknown to the watcher); the nearest in time wins.
fn partner(observed: &Observed, whys: &[WhyAt], window: JoinWindow) -> Option<usize> {
    whys.iter()
        .enumerate()
        .filter(|(_, w)| w.claim.path == observed.path)
        .filter(|(_, w)| apart(w.at, observed.at) <= u64::from(window.0))
        .filter(|(_, w)| observed.content.is_none_or(|c| c == w.claim.content))
        .min_by_key(|(_, w)| apart(w.at, observed.at))
        .map(|(i, _)| i)
}

/// Joins what is pending as of `now`: pairs observations with explanations, settles the ones
/// whose window has elapsed, and leaves the rest waiting. Pure.
pub fn join(pending: Pending, now: Stamp, window: JoinWindow) -> JoinStep {
    let Pending {
        mut observed,
        mut whys,
    } = pending;
    observed.sort_by_key(|o| o.at);
    let mut settled: Vec<(Stamp, Joined)> = Vec::new();
    let mut waiting_observed = Vec::new();
    for o in observed {
        if let Some(i) = partner(&o, &whys, window) {
            let why = whys.remove(i);
            let content = o.content.unwrap_or(why.claim.content);
            let event = FileEvent {
                change: o.change.clone(),
                file: view(&o, content),
                why: FileWhy::Explained {
                    cause: why.claim.cause,
                    verb: why.claim.verb,
                    by: why.claim.by.clone(),
                },
                actor: why.claim.by,
            };
            settled.push((
                o.at,
                Joined {
                    alias: alias_of(&o),
                    event,
                },
            ));
        } else if elapsed(o.at, now, window) {
            let actor = o
                .by_app
                .clone()
                .map_or(Actor::Unknown, |app| Actor::App { app });
            let content = o.content.unwrap_or(almanac_core::ContentDigest([0; 32]));
            let event = FileEvent {
                change: o.change.clone(),
                file: view(&o, content),
                why: FileWhy::Unexplained,
                actor,
            };
            settled.push((
                o.at,
                Joined {
                    alias: alias_of(&o),
                    event,
                },
            ));
        } else {
            waiting_observed.push(o);
        }
    }
    let mut waiting_whys = Vec::new();
    for w in whys {
        if elapsed(w.at, now, window) {
            let event = FileEvent {
                change: change_for(w.claim.verb),
                file: FileView {
                    path: w.claim.path.clone(),
                    inode: 0,
                    content: w.claim.content,
                },
                why: FileWhy::Explained {
                    cause: w.claim.cause,
                    verb: w.claim.verb,
                    by: w.claim.by.clone(),
                },
                actor: w.claim.by,
            };
            settled.push((w.at, Joined { event, alias: None }));
        } else {
            waiting_whys.push(w);
        }
    }
    settled.sort_by_key(|(at, _)| *at);
    JoinStep {
        settled: settled.into_iter().map(|(_, j)| j).collect(),
        waiting: Pending {
            observed: waiting_observed,
            whys: waiting_whys,
        },
    }
}
