//! Automatic recall (Q3) and recent activity: the request shapes the action router sends so the
//! companion's working-set assembler can fill its sections, and the pure budget arithmetic.

use crate::ids::KindPattern;
use crate::query::RecallOver;
use crate::text::UserText;
use crate::timeline::TrustFilter;
use crate::views::EventSummary;
use porter_core::{Count, SpaceId, Tokens, UnixSeconds};
use prov::{Effect, Label};
use serde::{Deserialize, Serialize};

/// How many characters one token is taken to be, for a budget (the same estimate recall's
/// chunker uses; the assembler measures the real tokenizer in its own tests).
pub const CHARS_PER_TOKEN: usize = 4;

/// The tokens `text` costs: its characters over [`CHARS_PER_TOKEN`], rounded up; empty is 0.
pub fn estimate_tokens(text: &str) -> Tokens {
    let tokens = text.chars().count().div_ceil(CHARS_PER_TOKEN);
    Tokens(u32::try_from(tokens).unwrap_or(u32::MAX))
}

/// Takes items in ranked order while they fit: at most `k`, at most `budget` tokens in all. An
/// item that does not fit is skipped (a later, smaller one may); the order is kept.
pub fn fit_budget<T>(
    ranked: impl IntoIterator<Item = T>,
    k: Count,
    budget: Tokens,
    cost: impl Fn(&T) -> Tokens,
) -> Vec<T> {
    let mut spent = 0u32;
    let mut taken = Vec::new();
    for item in ranked {
        if u32::try_from(taken.len()).unwrap_or(u32::MAX) >= k.0 {
            break;
        }
        match spent.checked_add(cost(&item).0) {
            Some(next) if next <= budget.0 => {
                spent = next;
                taken.push(item);
            }
            Some(_) | None => {}
        }
    }
    taken
}

/// "What is relevant to this turn": a search whose answer is cut to a token budget, so the
/// assembler can inject it without a second pass. Answered with `MemoryReply::Hits`, ranked, each
/// hit with its label; the whole answer costs at most `budget` by [`estimate_tokens`] over the
/// hits' text. Audited as one `Memory.Read` (`ReadScope::Inject`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InjectQuery {
    /// The Space (the invocation's; the router never sends another).
    pub space: SpaceId,
    /// What the turn is about: the person's words and the context's gist.
    pub text: UserText,
    /// The most tokens the hits may cost in all (about 1,500 by default; a setting).
    pub budget: Tokens,
    /// The most hits.
    pub k: Count,
    /// Which documents.
    pub over: RecallOver,
    /// Which provenance. The assembler asks `TrustedOnly`: untrusted hits reach the planner as
    /// handles through an ordinary `Search`, never injected.
    pub trust: TrustFilter,
}

/// "What happened lately": newest first, for the roster and the recent-episodes section.
/// Answered with `MemoryReply::Recent`; allowed for the router and the shell, unlike
/// `Timeline`, which is the shell's alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecentQuery {
    /// Events that happened at or after this time.
    pub since: UnixSeconds,
    /// Which kinds (empty: all), e.g. `companion.episode`, `companion.message`.
    pub kinds: Vec<KindPattern>,
    /// Which provenance.
    pub trust: TrustFilter,
    /// How many at most.
    pub limit: Count,
}

/// One recent event with what the router needs to show or inject it. It carries its label: the
/// planner applies taint from it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecentEntry {
    /// Which event, when, what kind, who, about what.
    pub summary: EventSummary,
    /// How consequential.
    pub effect: Effect,
    /// Its provenance.
    pub label: Label,
    /// Its searchable text, when it has any and its body is present (a message's words, an
    /// episode's skeleton lines).
    pub text: Option<UserText>,
}
