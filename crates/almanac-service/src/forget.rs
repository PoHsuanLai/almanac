//! Forgetting: the plan, its digest, its machine (memory section 4.3) and the apply order.
//!
//! A plan is computed once over snapshots of the log and the facts; its digest covers the whole
//! closure, so `Forget(token)` refuses when anything new derived from it in between.

use crate::docs::{event_ref, fact_doc_id, indexed_docs, newest_episodes};
use almanac_core::{
    Count, FactId, FactState, ForgetCounts, ForgetScope, Link, PlanDigest, PlanToken, Refusal, Seq,
    SpaceId, UnixSeconds,
};
use eventlog::{BodyState, Entry, LogRead};
use memfiles::VaultPath;
use std::collections::BTreeSet;

/// A plan lapses this long after it was made.
pub const PLAN_TTL_SECONDS: i64 = 600;

/// One fact as the closure walk sees it: a snapshot node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactNode {
    /// The fact.
    pub id: FactId,
    /// What it came from.
    pub links: Vec<Link>,
    /// Where it stands.
    pub state: FactState,
}

/// The facts of a Space, as the planner reads them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FactGraph {
    /// Every fact, active and pending.
    pub nodes: Vec<FactNode>,
    /// Every procedure file.
    pub procedures: Vec<VaultPath>,
}

/// The closure of one forget: everything apply removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The Space.
    pub space: SpaceId,
    /// What was asked.
    pub scope: ForgetScope,
    /// Events whose bodies are erased (matching, and those whose subjects or sources match).
    pub events: Vec<Seq>,
    /// Active facts removed (transitively through `Link::Fact`).
    pub facts: Vec<FactId>,
    /// Pending facts removed.
    pub pending: Vec<FactId>,
    /// Procedures removed.
    pub procedures: Vec<VaultPath>,
    /// Index documents removed (`f:<fact>`, `e:<replica>:<seq>`).
    pub index_docs: Vec<String>,
}

/// Computes the closure of `scope`: events whose subjects or sources match (including `Area`
/// payloads by their `things`, and messages by the entities they name), facts linked to any of
/// them or to the thing, transitively, pending facts, procedures and index documents. Pure over
/// the snapshots; a log that cannot be read gives an empty event list (the service checks the
/// log before it plans).
pub fn plan_forget(
    space: &SpaceId,
    scope: &ForgetScope,
    log: &impl LogRead,
    graph: &FactGraph,
) -> Plan {
    let entries = log.scan(Seq(0)).unwrap_or_default();
    let newest = newest_episodes(&entries);
    let hit: Vec<&Entry> = entries
        .iter()
        .filter(|e| matches!(e.body, BodyState::Present(_)) && scope_matches(space, scope, e))
        .collect();
    let mut roots: Vec<Link> = hit
        .iter()
        .map(|e| Link::Event(event_ref(space, e)))
        .collect();
    let seeds: BTreeSet<FactId> = match scope {
        ForgetScope::Thing(thing) => {
            roots.push(Link::Thing(thing.clone()));
            BTreeSet::new()
        }
        ForgetScope::Fact(id) => graph
            .nodes
            .iter()
            .filter(|n| &n.id == id)
            .map(|n| n.id.clone())
            .collect(),
        ForgetScope::Space => graph.nodes.iter().map(|n| n.id.clone()).collect(),
        ForgetScope::Event(_)
        | ForgetScope::Range(..)
        | ForgetScope::App(_)
        | ForgetScope::Kind(_) => BTreeSet::new(),
    };
    let closure = derived_closure(graph, roots, seeds);
    let (pending, facts): (Vec<&FactNode>, Vec<&FactNode>) = graph
        .nodes
        .iter()
        .filter(|n| closure.contains(&n.id))
        .partition(|n| n.state == FactState::Pending);
    let indexed_facts = facts
        .iter()
        .filter(|n| !matches!(n.state, FactState::Superseded { .. }))
        .map(|n| fact_doc_id(&n.id));
    let event_docs = hit
        .iter()
        .flat_map(|e| indexed_docs(space, e, &newest))
        .map(|d| d.id);
    let index_docs: BTreeSet<String> = indexed_facts.chain(event_docs).collect();
    Plan {
        space: space.clone(),
        scope: scope.clone(),
        events: hit.iter().map(|e| e.header.seq).collect(),
        facts: facts.iter().map(|n| n.id.clone()).collect(),
        pending: pending.iter().map(|n| n.id.clone()).collect(),
        procedures: match scope {
            ForgetScope::Space => graph.procedures.clone(),
            _ => Vec::new(),
        },
        index_docs: index_docs.into_iter().collect(),
    }
}

/// Every fact derived from `roots`, transitively through `Link::Fact`, and the `seeds`
/// themselves with what derives from them (strict: any one link is enough, QUESTIONS Me2).
fn derived_closure(
    graph: &FactGraph,
    roots: Vec<Link>,
    seeds: BTreeSet<FactId>,
) -> BTreeSet<FactId> {
    let mut seen = seeds;
    let mut frontier: BTreeSet<Link> = roots
        .into_iter()
        .chain(seen.iter().cloned().map(Link::Fact))
        .collect();
    while !frontier.is_empty() {
        let next: Vec<FactId> = graph
            .nodes
            .iter()
            .filter(|n| !seen.contains(&n.id) && n.links.iter().any(|l| frontier.contains(l)))
            .map(|n| n.id.clone())
            .collect();
        seen.extend(next.iter().cloned());
        frontier = next.into_iter().map(Link::Fact).collect();
    }
    seen
}

/// Whether the (present) event is in what `scope` asks to forget.
fn scope_matches(space: &SpaceId, scope: &ForgetScope, entry: &Entry) -> bool {
    let BodyState::Present(body) = &entry.body else {
        return false;
    };
    let h = &entry.header;
    match scope {
        ForgetScope::Event(r) => &r.space == space && r.seq == h.seq && r.replica == h.replica,
        ForgetScope::Thing(thing) => body.names(thing),
        ForgetScope::Fact(_) => false,
        ForgetScope::Range(from, to) => *from <= h.occurred && h.occurred <= *to,
        ForgetScope::App(app) => body.involves_app(&h.actor, app),
        ForgetScope::Kind(pattern) => pattern.covers_event(body),
        ForgetScope::Space => true,
    }
}

impl Plan {
    /// What apply will remove, as the preview shows it.
    pub fn counts(&self) -> ForgetCounts {
        let n = |len: usize| Count(u32::try_from(len).unwrap_or(u32::MAX));
        ForgetCounts {
            events: n(self.events.len()),
            facts: n(self.facts.len()),
            pending: n(self.pending.len()),
            procedures: n(self.procedures.len()),
            index_docs: n(self.index_docs.len()),
        }
    }

    /// The digest of the closure: `blake3("QPLAN1" ‖ each list, sorted, length-prefixed)`.
    /// Equal closures have equal digests, whatever order the walk found them in.
    pub fn digest(&self) -> PlanDigest {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"QPLAN1");
        let mut put = |tag: u8, mut items: Vec<String>| {
            items.sort();
            items.dedup();
            hasher.update(&[tag]);
            hasher.update(&u32::try_from(items.len()).unwrap_or(u32::MAX).to_be_bytes());
            for item in &items {
                hasher.update(&u32::try_from(item.len()).unwrap_or(u32::MAX).to_be_bytes());
                hasher.update(item.as_bytes());
            }
        };
        put(0, vec![self.space.to_string()]);
        put(1, self.events.iter().map(|s| s.0.to_string()).collect());
        put(2, self.facts.iter().map(ToString::to_string).collect());
        put(3, self.pending.iter().map(ToString::to_string).collect());
        put(4, self.procedures.iter().map(ToString::to_string).collect());
        put(5, self.index_docs.clone());
        PlanDigest(*hasher.finalize().as_bytes())
    }
}

/// The steps of applying a plan, in order. The order is idempotent: a crash re-runs from the
/// plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyStep {
    /// Remove index documents.
    Index,
    /// Remove facts, pending facts and procedures from the files.
    Memfiles,
    /// Erase event bodies and their `things` rows.
    EventBodies,
    /// Log `Memory.Forgot`.
    AuditEntry,
    /// `wal_checkpoint(TRUNCATE)`, so erased bytes leave the write-ahead log.
    WalTruncate,
}

impl ApplyStep {
    /// Every step, in the order they run.
    pub const ORDER: [ApplyStep; 5] = [
        ApplyStep::Index,
        ApplyStep::Memfiles,
        ApplyStep::EventBodies,
        ApplyStep::AuditEntry,
        ApplyStep::WalTruncate,
    ];
}

/// Where a plan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    /// Made, waiting for the person.
    Planned {
        /// The closure's digest.
        digest: PlanDigest,
        /// When it lapses.
        expires: UnixSeconds,
    },
    /// Being applied.
    Applying,
    /// Done.
    Applied,
    /// The closure changed; make a new plan.
    Stale,
    /// It lapsed.
    Expired,
}

/// What happened to a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanEvent {
    /// `Forget(token)`: the closure's digest now, and the time.
    Forget {
        /// The digest recomputed now.
        current: PlanDigest,
        /// Now.
        now: UnixSeconds,
    },
    /// Time passed.
    Tick {
        /// Now.
        now: UnixSeconds,
    },
    /// Apply finished.
    Done,
}

/// What the service must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanEffect {
    /// Run [`ApplyStep::ORDER`].
    Apply,
    /// Answer the request with this refusal.
    Refuse(Refusal),
}

impl PlanState {
    /// A fresh plan with digest `digest`, made at `now`.
    pub fn planned(digest: PlanDigest, now: UnixSeconds) -> PlanState {
        PlanState::Planned {
            digest,
            expires: UnixSeconds(now.0 + PLAN_TTL_SECONDS),
        }
    }
}

/// The next state and effects.
pub fn plan_step(state: PlanState, event: PlanEvent) -> (PlanState, Vec<PlanEffect>) {
    match (state, event) {
        (PlanState::Planned { expires, .. }, PlanEvent::Forget { now, .. }) if now > expires => (
            PlanState::Expired,
            vec![PlanEffect::Refuse(Refusal::PlanExpired)],
        ),
        (PlanState::Planned { digest, .. }, PlanEvent::Forget { current, .. })
            if current != digest =>
        {
            (
                PlanState::Stale,
                vec![PlanEffect::Refuse(Refusal::PlanStale)],
            )
        }
        (PlanState::Planned { .. }, PlanEvent::Forget { .. }) => {
            (PlanState::Applying, vec![PlanEffect::Apply])
        }
        (PlanState::Planned { expires, .. }, PlanEvent::Tick { now }) if now > expires => {
            (PlanState::Expired, vec![])
        }
        (PlanState::Applying, PlanEvent::Done) => (PlanState::Applied, vec![]),
        (PlanState::Expired, PlanEvent::Forget { .. }) => (
            PlanState::Expired,
            vec![PlanEffect::Refuse(Refusal::PlanExpired)],
        ),
        (PlanState::Stale, PlanEvent::Forget { .. }) => (
            PlanState::Stale,
            vec![PlanEffect::Refuse(Refusal::PlanStale)],
        ),
        (unchanged, _) => (unchanged, vec![]),
    }
}

/// The token a plan is addressed by: `p-` and the digest's first 16 hex digits (an id in
/// porter's grammar).
pub fn token_for(digest: &PlanDigest) -> Option<PlanToken> {
    let hex = digest.to_string();
    PlanToken::parse(&format!("p-{}", &hex[..16])).ok()
}
