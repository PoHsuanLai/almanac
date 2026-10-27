//! Readers: `Search` and `Inject` (ranked by `almanac-search`), `Recent`, `Related`, `Provenance`, `Facts`, `Primer`, and the
//! `Memory.Read` audit of the router's reads.

use crate::backend::Backend;
use crate::caller::Caller;
use crate::open::{Cx, Open, Stored};
use almanac_core::{
    Actor, BodyMode, Count, EventBody, EventRef, EventSummary, Fact, FactFilter, FactId, FactQuery,
    FactView, FileHistoryEntry, FileProvenance, InjectQuery, JsonText, Link, MemoryItem, MemoryOp,
    ReadScope, RecallHit, RecallQuery, RecentEntry, RecentQuery, Refusal, Seq, SystemPart,
    ThingRef, TimelineQuery, UserText,
};
use almanac_search::SpaceReader;
use almanac_search::docs::present;
use almanac_store::{Entry, LogRead, SearchIndex, Vector};

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
        // The Settings app records and reads nothing, so no audit entry names it; unknown is the
        // actor that claims nothing.
        Caller::Settings => Actor::Unknown,
    }
}

impl<B: Backend> Open<B> {
    /// Hits for a search; the query is embedded here and ranked in `almanac-search`.
    pub(crate) async fn search(
        &mut self,
        cx: &Cx<'_, B>,
        q: RecallQuery,
    ) -> Result<Vec<RecallHit>, Refusal> {
        let vector = self.query_vector(cx, q.text.as_str()).await;
        let hits = almanac_search::search(&*self, vector.as_ref(), q)?;
        self.audit_read(cx, ReadScope::Search, &hits)?;
        Ok(hits)
    }

    /// A search cut to a token budget.
    pub(crate) async fn inject(
        &mut self,
        cx: &Cx<'_, B>,
        q: InjectQuery,
    ) -> Result<Vec<RecallHit>, Refusal> {
        let vector = self.query_vector(cx, q.text.as_str()).await;
        let hits = almanac_search::inject(&*self, vector.as_ref(), q)?;
        self.audit_read(cx, ReadScope::Inject, &hits)?;
        Ok(hits)
    }

    async fn query_vector(&mut self, cx: &Cx<'_, B>, text: &str) -> Option<Vector> {
        let card = self.rt.index.card().clone();
        almanac_search::embed_query(&card, cx.backend.embedder(), text).await
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
            Caller::App(_) | Caller::Cuad | Caller::ShellUi | Caller::Settings => Ok(()),
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
        use almanac_store::Vault;
        let written = self
            .rt
            .store
            .vault()
            .read(&almanac_store::VaultPath::primer())
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
                        memfiles_format::PrimerEntry {
                            title: UserText::new(topic.to_string()),
                            summary: UserText::new(summary),
                            topic,
                        }
                    })
                    .collect();
                memfiles_format::Primer { entries }.render()
            }
        };
        self.audit_by(cx, ReadScope::Primer, Vec::new(), 0)?;
        Ok(text)
    }
}

impl<B: Backend> SpaceReader for Open<B> {
    type Index = B::Index;

    fn index(&self) -> &B::Index {
        &self.rt.index
    }

    fn entry_at(&self, seq: Seq) -> Option<Entry> {
        Open::entry_at(self, seq)
    }

    fn event_ref(&self, entry: &Entry) -> EventRef {
        Open::event_ref(self, entry)
    }

    fn stored_facts(&self) -> Result<Vec<Fact>, Refusal> {
        Ok(self.stored()?.into_iter().map(|s| s.fact).collect())
    }
}
