//! `Search` and `Inject`: the ranking of a query over the index, the merge of fact and event
//! rankings, and the resolution of each ranked document to the hit a person sees.

use crate::docs::{DocRef, event_docs, present};
use crate::reader::SpaceReader;
use almanac_core::{
    Fact, InjectQuery, Link, MemoryItem, RecallHit, RecallOver, RecallQuery, RecallWhy, Refusal,
    TrustFilter, UserText, estimate_tokens, fit_budget,
};
use almanac_store::{
    Allow, EmbedRole, Embedder, EmbedderCard, Fused, HitWhy, Ranked, SearchIndex, SearchQuery,
    SpaceCheck, TopK, TrustTier, Urgency, Vector, chunk, fuse_rrf,
};

fn why_of(why: HitWhy) -> RecallWhy {
    match why {
        HitWhy::Lexical { rank } => RecallWhy::Lexical { rank },
        HitWhy::Semantic { rank } => RecallWhy::Semantic { rank },
        HitWhy::Both { lexical, semantic } => RecallWhy::Both { lexical, semantic },
    }
}

fn failed(e: impl std::fmt::Display) -> Refusal {
    Refusal::Invalid(e.to_string())
}

/// The query embedded, or `None` when the embedder is away or in another vector space than the
/// index's `card` (search then degrades to lexical, as the index does).
pub async fn embed_query(card: &EmbedderCard, e: &impl Embedder, text: &str) -> Option<Vector> {
    if e.card().space_vs(card) == SpaceCheck::Different {
        return None;
    }
    let first = chunk(text, e.card().max_tokens).into_iter().next()?;
    e.embed(&[first.text], EmbedRole::Query, Urgency::Interactive)
        .await
        .ok()?
        .pop()
}

/// The documents a search may return: those of the kinds `over` covers and of the trust asked
/// for. The facets live in the lexical half of the index.
fn allow<R: SpaceReader>(r: &R, over: RecallOver, trust: TrustFilter) -> Result<Allow, Refusal> {
    let trust = match trust {
        TrustFilter::Any => None,
        TrustFilter::TrustedOnly => Some(TrustTier::Trusted),
        TrustFilter::UntrustedOnly => Some(TrustTier::Untrusted),
    };
    r.index()
        .allow_kinds(over.facet_kinds(), trust)
        .map_err(failed)
}

/// Fused lexical and semantic results over `over`, best first, from a query vector that was
/// embedded beforehand.
fn ranked_with<R: SpaceReader>(
    r: &R,
    text: &str,
    vector: Option<&Vector>,
    over: RecallOver,
    trust: TrustFilter,
    k: u32,
) -> Result<Vec<Fused>, Refusal> {
    let allow = allow(r, over, trust)?;
    let query = SearchQuery {
        text: text.to_owned(),
        k: TopK(k),
        allow: allow.clone(),
        urgency: Urgency::Interactive,
    };
    let index = r.index();
    let lexical = index.lexical(&query).map_err(failed)?;
    let semantic = match vector {
        Some(v) => index.nearest(v, TopK(k), &allow).map_err(failed)?,
        None => Vec::new(),
    };
    let mut fused = fuse_rrf(&[lexical, semantic], index.rrf_k());
    fused.truncate(usize::try_from(k).unwrap_or(usize::MAX));
    Ok(fused)
}

/// The hit of one fused result, if its source is still there.
fn resolve<R: SpaceReader>(r: &R, fused: &Fused, facts: &[Fact]) -> Option<RecallHit> {
    match DocRef::parse(&fused.id.0)? {
        DocRef::Fact(id) => {
            let fact = facts.iter().find(|f| f.id == id)?;
            Some(RecallHit {
                doc: MemoryItem::Fact(id),
                text: UserText::new(fact.text.as_str()),
                at: fact.recorded,
                label: fact.label.clone(),
                links: fact.links.clone(),
                why: why_of(fused.why),
            })
        }
        DocRef::Event { seq, .. } => {
            let entry = r.entry_at(seq)?;
            let body = present(&entry)?;
            let event = r.event_ref(&entry);
            let doc = event_docs(&event, &entry.header.label, body)
                .into_iter()
                .find(|d| d.id == fused.id.0)?;
            Some(RecallHit {
                doc: MemoryItem::Event(event.clone()),
                text: UserText::new(doc.text),
                at: entry.header.occurred,
                label: doc.label,
                links: vec![Link::Event(event)],
                why: why_of(fused.why),
            })
        }
    }
}

fn resolve_all<R: SpaceReader>(r: &R, fused: &[Fused]) -> Result<Vec<RecallHit>, Refusal> {
    let facts = if fused.iter().any(|f| f.id.0.starts_with("f:")) {
        r.stored_facts()?
    } else {
        Vec::new()
    };
    Ok(fused.iter().filter_map(|f| resolve(r, f, &facts)).collect())
}

/// Two ranked lists as one: reciprocal-rank fusion by position, each hit keeping the `why` of
/// the list it came from.
fn merged<R: SpaceReader>(r: &R, facts: Vec<Fused>, events: Vec<Fused>) -> Vec<Fused> {
    let listed = |list: &[Fused]| -> Vec<Ranked> {
        list.iter()
            .zip(1u32..)
            .map(|(f, rank)| Ranked {
                id: f.id.clone(),
                rank,
            })
            .collect()
    };
    let fused = fuse_rrf(&[listed(&facts), listed(&events)], r.index().rrf_k());
    fused
        .into_iter()
        .filter_map(|f| {
            let own = facts.iter().chain(events.iter()).find(|o| o.id == f.id)?;
            Some(Fused { why: own.why, ..f })
        })
        .collect()
}

/// The hits for `q`, best first, from `vector` (the query as [`embed_query`] returned it).
pub fn search<R: SpaceReader>(
    r: &R,
    vector: Option<&Vector>,
    q: RecallQuery,
) -> Result<Vec<RecallHit>, Refusal> {
    let fused = ranked_with(
        r,
        q.text.as_str(),
        vector,
        q.over,
        TrustFilter::Any,
        q.limit.0,
    )?;
    resolve_all(r, &fused)
}

/// A search cut to a token budget. Over `Both`, facts and event documents are ranked apart and
/// merged by reciprocal rank, so neither drowns the other; then `fit_budget` takes hits in order
/// while they fit.
pub fn inject<R: SpaceReader>(
    r: &R,
    vector: Option<&Vector>,
    q: InjectQuery,
) -> Result<Vec<RecallHit>, Refusal> {
    let pool = q.k.0.saturating_mul(4).max(16);
    let text = q.text.as_str();
    let fused = match q.over {
        RecallOver::Both => {
            let facts = ranked_with(r, text, vector, RecallOver::Facts, q.trust, pool)?;
            let events = ranked_with(r, text, vector, RecallOver::Events, q.trust, pool)?;
            merged(r, facts, events)
        }
        over => ranked_with(r, text, vector, over, q.trust, pool)?,
    };
    Ok(fit_budget(resolve_all(r, &fused)?, q.k, q.budget, |h| {
        estimate_tokens(h.text.as_str())
    }))
}
