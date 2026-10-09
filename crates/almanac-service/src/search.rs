//! Readers: `Search`, `Inject`, `Recent`, `Related`, `Provenance`, `Facts`, `Primer`, and the
//! `Memory.Read` audit of the router's reads.

use crate::backend::Backend;
use crate::docs::{DocRef, event_docs};
use crate::open::{Cx, Open, Stored, present};
use almanac_core::{
    Actor, BodyMode, Caller, Count, EventBody, EventSummary, FactFilter, FactId, FactQuery,
    FactView, FileHistoryEntry, FileProvenance, InjectQuery, JsonText, Link, MemoryItem, MemoryOp,
    ReadScope, RecallHit, RecallOver, RecallQuery, RecallWhy, RecentEntry, RecentQuery, Refusal,
    SystemPart, ThingRef, TimelineQuery, TrustFilter, UserText, estimate_tokens, fit_budget,
};
use eventlog::{Entry, LogRead};
use recall::{
    Allow, EmbedRole, Embedder, EmbedderCard, Fused, HitWhy, Ranked, SearchQuery, SpaceCheck, TopK,
    TrustTier, Urgency, Vector, VectorIndex, chunk, fuse_rrf,
};

fn why_of(why: HitWhy) -> RecallWhy {
    match why {
        HitWhy::Lexical { rank } => RecallWhy::Lexical { rank },
        HitWhy::Semantic { rank } => RecallWhy::Semantic { rank },
        HitWhy::Both { lexical, semantic } => RecallWhy::Both { lexical, semantic },
    }
}

fn count_of(n: usize) -> Count {
    Count(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The actor that stands for a caller in the audit.
pub(crate) fn caller_actor(caller: &Caller) -> Actor {
    match caller {
        Caller::App(id) => Actor::App {
            app: id.name.clone(),
        },
        Caller::Router => Actor::System {
            part: SystemPart::Router,
        },
        Caller::Cuad => Actor::System {
            part: SystemPart::Cua,
        },
        Caller::ShellUi => almanac_core::AppName::parse("org.quire.Shell")
            .map_or(Actor::Unknown, |via| Actor::User { via }),
    }
}

impl<B: Backend> Open<B> {
    /// The documents a search may return: those of the kinds `over` covers and of the trust
    /// asked for. The facets live in the lexical half of the index.
    fn allow(&self, over: RecallOver, trust: TrustFilter) -> Result<Allow, Refusal> {
        let trust = match trust {
            TrustFilter::Any => None,
            TrustFilter::TrustedOnly => Some(TrustTier::Trusted),
            TrustFilter::UntrustedOnly => Some(TrustTier::Untrusted),
        };
        self.rt
            .index
            .allow_kinds(over.facet_kinds(), trust)
            .map_err(crate::open::failed)
    }

    /// Fused lexical and semantic results over `over`, best first, from a query vector that was
    /// embedded beforehand (the index's own `search` holds a shared borrow of its SQLite
    /// connections across the embedding, which would make the future `!Send`).
    fn ranked_with(
        &self,
        text: &str,
        vector: Option<&Vector>,
        over: RecallOver,
        trust: TrustFilter,
        k: u32,
    ) -> Result<Vec<Fused>, Refusal> {
        let allow = self.allow(over, trust)?;
        let query = SearchQuery {
            text: text.to_owned(),
            k: TopK(k),
            allow: allow.clone(),
            urgency: Urgency::Interactive,
        };
        let failed = crate::open::failed;
        let lexical = self.rt.index.lexical(&query).map_err(failed)?;
        let semantic = match vector {
            Some(v) => self
                .rt
                .index
                .parts()
                .1
                .nearest(v, TopK(k), &allow)
                .map_err(failed)?,
            None => Vec::new(),
        };
        let mut fused = fuse_rrf(&[lexical, semantic], self.rt.index.rrf_k());
        fused.truncate(usize::try_from(k).unwrap_or(usize::MAX));
        Ok(fused)
    }

    /// The query embedded, or `None` when the embedder is away or in another vector space
    /// (search then degrades to lexical, as the index does).
    async fn embed_query(self_card: EmbedderCard, e: &impl Embedder, text: &str) -> Option<Vector> {
        if e.card().space_vs(&self_card) == SpaceCheck::Different {
            return None;
        }
        let first = chunk(text, e.card().max_tokens).into_iter().next()?;
        e.embed(&[first.text], EmbedRole::Query, Urgency::Interactive)
            .await
            .ok()?
            .pop()
    }

    /// The hit of one fused result, if its source is still there.
    fn resolve(&self, fused: &Fused, facts: &[Stored]) -> Option<RecallHit> {
        match DocRef::parse(&fused.id.0)? {
            DocRef::Fact(id) => {
                let s = facts.iter().find(|s| s.fact.id == id)?;
                Some(RecallHit {
                    doc: MemoryItem::Fact(id),
                    text: UserText::new(s.fact.text.as_str()),
                    at: s.fact.recorded,
                    label: s.fact.label.clone(),
                    links: s.fact.links.clone(),
                    why: why_of(fused.why),
                })
            }
            DocRef::Event { seq, .. } => {
                let entry = self.entry_at(seq)?;
                let body = present(&entry)?;
                let event = self.event_ref(&entry);
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

    fn resolve_all(&self, fused: &[Fused]) -> Result<Vec<RecallHit>, Refusal> {
        let facts = if fused.iter().any(|f| f.id.0.starts_with("f:")) {
            self.stored()?
        } else {
            Vec::new()
        };
        Ok(fused
            .iter()
            .filter_map(|f| self.resolve(f, &facts))
            .collect())
    }

    pub(crate) async fn search(
        &mut self,
        cx: &Cx<'_, B>,
        q: RecallQuery,
    ) -> Result<Vec<RecallHit>, Refusal> {
        let vector = self.query_vector(cx, q.text.as_str()).await;
        let fused = self.ranked_with(
            q.text.as_str(),
            vector.as_ref(),
            q.over,
            TrustFilter::Any,
            q.limit.0,
        )?;
        let hits = self.resolve_all(&fused)?;
        self.audit_read(cx, ReadScope::Search, &hits)?;
        Ok(hits)
    }

    async fn query_vector(&mut self, cx: &Cx<'_, B>, text: &str) -> Option<Vector> {
        let card = self.rt.index.parts().1.card().clone();
        Self::embed_query(card, cx.backend.embedder(), text).await
    }

    /// A search cut to a token budget. Over `Both`, facts and event documents are ranked apart
    /// and merged by reciprocal rank, so neither drowns the other; then `fit_budget` takes hits
    /// in order while they fit.
    pub(crate) async fn inject(
        &mut self,
        cx: &Cx<'_, B>,
        q: InjectQuery,
    ) -> Result<Vec<RecallHit>, Refusal> {
        let pool = q.k.0.saturating_mul(4).max(16);
        let text = q.text.as_str();
        let vector = self.query_vector(cx, text).await;
        let v = vector.as_ref();
        let fused = match q.over {
            RecallOver::Both => {
                let facts = self.ranked_with(text, v, RecallOver::Facts, q.trust, pool)?;
                let events = self.ranked_with(text, v, RecallOver::Events, q.trust, pool)?;
                self.merged(facts, events)
            }
            over => self.ranked_with(text, v, over, q.trust, pool)?,
        };
        let hits = fit_budget(self.resolve_all(&fused)?, q.k, q.budget, |h| {
            estimate_tokens(h.text.as_str())
        });
        self.audit_read(cx, ReadScope::Inject, &hits)?;
        Ok(hits)
    }

    /// Two ranked lists as one: reciprocal-rank fusion by position, each hit keeping the `why`
    /// of the list it came from.
    fn merged(&self, facts: Vec<Fused>, events: Vec<Fused>) -> Vec<Fused> {
        let listed = |list: &[Fused]| -> Vec<Ranked> {
            list.iter()
                .zip(1u32..)
                .map(|(f, rank)| Ranked {
                    id: f.id.clone(),
                    rank,
                })
                .collect()
        };
        let fused = fuse_rrf(&[listed(&facts), listed(&events)], self.rt.index.rrf_k());
        fused
            .into_iter()
            .filter_map(|f| {
                let own = facts.iter().chain(events.iter()).find(|o| o.id == f.id)?;
                Some(Fused { why: own.why, ..f })
            })
            .collect()
    }

    /// `Memory.Read` for the router's reads (every read it makes is audited).
    fn audit_read(
        &mut self,
        cx: &Cx<'_, B>,
        scope: ReadScope,
        hits: &[RecallHit],
    ) -> Result<(), Refusal> {
        let facts = hits
            .iter()
            .filter_map(|h| match &h.doc {
                MemoryItem::Fact(id) => Some(id.clone()),
                MemoryItem::Event(_) => None,
            })
            .collect();
        let events = hits
            .iter()
            .filter(|h| matches!(h.doc, MemoryItem::Event(_)))
            .count();
        self.audit_by(cx, scope, facts, events)
    }

    pub(crate) fn audit_by(
        &mut self,
        cx: &Cx<'_, B>,
        scope: ReadScope,
        facts: Vec<FactId>,
        events: usize,
    ) -> Result<(), Refusal> {
        match cx.caller {
            Caller::Router => self
                .audit(
                    cx.now(),
                    MemoryOp::Read {
                        by: caller_actor(cx.caller),
                        scope,
                        facts,
                        events: count_of(events),
                    },
                )
                .map(drop),
            Caller::App(_) | Caller::Cuad | Caller::ShellUi => Ok(()),
        }
    }

    /// Recent activity, newest first, with labels (and bodies when asked).
    pub(crate) fn recent(
        &mut self,
        cx: &Cx<'_, B>,
        q: RecentQuery,
    ) -> Result<Vec<RecentEntry>, Refusal> {
        let mut filter = crate::open::any_filter();
        filter.kinds = q.kinds.clone();
        filter.trust = q.trust;
        filter.range = Some((q.since, almanac_core::UnixSeconds(i64::MAX)));
        let page = TimelineQuery {
            before: None,
            limit: q.limit,
            filter,
        };
        let entries = self.rt.log.page(&page).map_err(crate::open::log_refusal)?;
        let out: Vec<RecentEntry> = entries
            .iter()
            .map(|e| self.recent_entry(e, q.bodies))
            .collect();
        self.audit_by(cx, ReadScope::Recent, Vec::new(), out.len())?;
        Ok(out)
    }

    pub(crate) fn recent_entry(&self, entry: &Entry, bodies: BodyMode) -> RecentEntry {
        let h = &entry.header;
        let body = present(entry);
        let texts: Vec<String> = body
            .map(|b| b.index_texts().into_iter().map(|t| t.text).collect())
            .unwrap_or_default();
        RecentEntry {
            summary: self.summary(entry),
            effect: h.effect,
            label: h.label.clone(),
            text: (!texts.is_empty()).then(|| UserText::new(texts.join("\n"))),
            body: match (bodies, body) {
                (BodyMode::Json, Some(EventBody::Area(p))) => Some(p.json.clone()),
                (BodyMode::Json, Some(b)) => serde_json::to_string(b)
                    .ok()
                    .and_then(|json| JsonText::parse(&json).ok()),
                (BodyMode::Json | BodyMode::Without, _) => None,
            },
        }
    }

    pub(crate) fn summary(&self, entry: &Entry) -> EventSummary {
        let h = &entry.header;
        EventSummary {
            event: self.event_ref(entry),
            occurred: h.occurred,
            kind: h.kind.clone(),
            actor: h.actor.clone(),
            things: present(entry)
                .map(|b| b.things().into_iter().map(|(v, _)| v.clone()).collect())
                .unwrap_or_default(),
        }
    }

    /// Events that name `thing`, newest first.
    pub(crate) fn related(
        &mut self,
        cx: &Cx<'_, B>,
        thing: &ThingRef,
    ) -> Result<Vec<EventSummary>, Refusal> {
        let mut entries = self.entries()?;
        entries.retain(|e| {
            present(e).is_some_and(|b| {
                b.names(thing)
                    && almanac_core::Recallable::of_body(b) == almanac_core::Recallable::Yes
            })
        });
        entries.reverse();
        let out: Vec<EventSummary> = entries.iter().map(|e| self.summary(e)).collect();
        self.audit_by(cx, ReadScope::Related, Vec::new(), out.len())?;
        Ok(out)
    }

    /// The changes to a file and why, newest first.
    pub(crate) fn provenance(
        &mut self,
        cx: &Cx<'_, B>,
        path: &almanac_core::SpacePath,
    ) -> Result<FileProvenance, Refusal> {
        let mut history: Vec<FileHistoryEntry> = self
            .entries()?
            .iter()
            .filter_map(|e| match present(e)? {
                EventBody::File { change, file, why } if &file.path == path => {
                    Some(FileHistoryEntry {
                        event: self.event_ref(e),
                        occurred: e.header.occurred,
                        change: change.clone(),
                        why: why.clone(),
                    })
                }
                _ => None,
            })
            .collect();
        history.reverse();
        self.audit_by(cx, ReadScope::Provenance, Vec::new(), history.len())?;
        Ok(FileProvenance {
            path: path.clone(),
            history,
        })
    }

    /// Facts, as the person sees them, with what they came from.
    pub(crate) fn facts(
        &mut self,
        cx: &Cx<'_, B>,
        q: &FactQuery,
    ) -> Result<Vec<FactView>, Refusal> {
        let stored = self.stored()?;
        let wanted = |s: &Stored| match q.state {
            FactFilter::Active => s.state == almanac_core::FactState::Active,
            FactFilter::Pending => s.state == almanac_core::FactState::Pending,
            FactFilter::All => true,
        };
        let mut chosen: Vec<&Stored> = stored
            .iter()
            .filter(|s| wanted(s))
            .filter(|s| q.topic.as_ref().is_none_or(|t| t == &s.topic))
            .filter(|s| {
                q.about
                    .as_ref()
                    .is_none_or(|t| s.fact.links.contains(&Link::Thing(t.clone())))
            })
            .collect();
        chosen.sort_by(|a, b| {
            b.fact
                .recorded
                .cmp(&a.fact.recorded)
                .then(a.fact.id.cmp(&b.fact.id))
        });
        chosen.truncate(usize::try_from(q.limit.0).unwrap_or(usize::MAX));
        let views: Vec<FactView> = self.fact_views(cx, &chosen)?;
        let ids = views.iter().map(|v| v.fact.id.clone()).collect();
        self.audit_by(cx, ReadScope::Facts, ids, 0)?;
        Ok(views)
    }

    /// The primer: `facts/INDEX.md` when the vault holds one (consolidation does not write it), else one line per topic.
    pub(crate) fn primer(&mut self, cx: &Cx<'_, B>) -> Result<String, Refusal> {
        use memfiles::Vault;
        let written = self
            .rt
            .store
            .vault()
            .read(&memfiles::VaultPath::primer())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
        let text = match written {
            Some(text) => text,
            None => {
                let stored = self.stored()?;
                let entries = self
                    .rt
                    .store
                    .topics()
                    .map_err(crate::open::files_refusal)?
                    .into_iter()
                    .map(|topic| {
                        let summary = stored
                            .iter()
                            .find(|s| {
                                s.topic == topic && s.state == almanac_core::FactState::Active
                            })
                            .map(|s| s.fact.text.as_str().to_owned())
                            .unwrap_or_default();
                        memfiles::PrimerEntry {
                            title: UserText::new(topic.to_string()),
                            summary: UserText::new(summary),
                            topic,
                        }
                    })
                    .collect();
                memfiles::Primer { entries }.render()
            }
        };
        self.audit_by(cx, ReadScope::Primer, Vec::new(), 0)?;
        Ok(text)
    }
}
