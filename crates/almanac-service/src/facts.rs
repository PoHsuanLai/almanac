//! Facts: proposals, settling pending ones, and the views the UI draws.

use crate::backend::Backend;
use crate::docs::{fact_doc, fact_doc_id};
use crate::events::ServiceEvent;
use crate::fact::{FactEffect, FactEvent, FactLife, lands, step};
use crate::open::{Cx, Open, Stored, failed, files_refusal};
use crate::search::caller_actor;
use crate::timeline::timeline_entry;
use almanac_core::{
    Caller, Confidentiality, Count, DesktopVerdict, EventBody, Fact, FactDraft, FactId, FactState,
    FactView, Integrity, Label, Link, MemoryOp, ModelRole, PENDING_TTL_DAYS, Refusal, Settlement,
    Source, SourceView, SpaceId, UnixSeconds, UseCount, Validity, desktop_admits,
};
use eventlog::Entry;
use std::collections::{BTreeMap, BTreeSet};

const SECONDS_PER_DAY: i64 = 86_400;

/// What a link adds to the label of a fact built from it.
fn link_label(link: &Link, space: &SpaceId, entries: &[Entry], stored: &[Stored]) -> Option<Label> {
    match link {
        Link::Event(e) => entries
            .iter()
            .find(|en| en.header.seq == e.seq && en.header.replica == e.replica)
            .map(|en| en.header.label.clone()),
        Link::Thing(t) => Some(Label {
            integrity: Integrity::Trusted,
            confidentiality: Confidentiality::Public,
            classes: BTreeSet::new(),
            sources: BTreeSet::from([Source::App(t.app.clone())]),
        }),
        Link::Fact(id) => stored
            .iter()
            .find(|s| &s.fact.id == id)
            .map(|s| s.fact.label.clone()),
        Link::Run(_) => Some(Label {
            integrity: Integrity::Untrusted,
            confidentiality: Confidentiality::Private(BTreeSet::from([space.clone()])),
            classes: BTreeSet::new(),
            sources: BTreeSet::from([Source::Model(ModelRole::Cua)]),
        }),
    }
}

/// Where a fact with this label lands.
pub(crate) fn lands_for(label: &Label) -> almanac_core::Lands {
    lands(label)
}

/// The label of a proposal: the person's own words when the shell proposes; text a planner wrote
/// (untrusted, private to the Space) when the router does; joined with what it cites.
fn proposal_label(caller: &Caller, space: &SpaceId, cited: &[Label]) -> Label {
    let base = match caller {
        Caller::ShellUi => Label::trusted_user(),
        Caller::App(_) | Caller::Router | Caller::Cuad => Label {
            integrity: Integrity::Untrusted,
            confidentiality: Confidentiality::Private(BTreeSet::from([space.clone()])),
            classes: BTreeSet::new(),
            sources: BTreeSet::from([Source::Model(ModelRole::Planner)]),
        },
    };
    cited.iter().fold(base, |acc, l| acc.join(l))
}

/// How often each fact was read into a prompt, and when last: the router's `Memory.Read`
/// audit entries still in the log (their bodies live for the `memory.*` retention, 90 days).
fn reads_of(entries: &[Entry]) -> BTreeMap<FactId, (u32, UnixSeconds)> {
    let mut uses: BTreeMap<FactId, (u32, UnixSeconds)> = BTreeMap::new();
    for entry in entries {
        let eventlog::BodyState::Present(EventBody::Memory {
            op: MemoryOp::Read { facts, .. },
        }) = &entry.body
        else {
            continue;
        };
        for id in facts {
            let seen = uses.entry(id.clone()).or_insert((0, entry.header.occurred));
            seen.0 = seen.0.saturating_add(1);
            seen.1 = seen.1.max(entry.header.occurred);
        }
    }
    uses
}

impl<B: Backend> Open<B> {
    /// Views of `items`, with the events and things they came from (one read of the log).
    pub(crate) fn fact_views(
        &self,
        cx: &Cx<'_, B>,
        items: &[&Stored],
    ) -> Result<Vec<FactView>, Refusal> {
        let entries = self.entries()?;
        let stored = self.stored()?;
        let uses = reads_of(&entries);
        let flags = self.flags_by_fact();
        Ok(items
            .iter()
            .map(|s| FactView {
                fact: s.fact.clone(),
                topic: s.topic.clone(),
                state: s.state.clone(),
                sources: s
                    .fact
                    .links
                    .iter()
                    .filter_map(|l| self.source_view(cx, l, &entries, &stored))
                    .collect(),
                used: UseCount(uses.get(&s.fact.id).map_or(0, |(n, _)| *n)),
                last_used: uses.get(&s.fact.id).map(|(_, at)| *at),
                flagged: flags.get(&s.fact.id).cloned().unwrap_or_default(),
            })
            .collect())
    }

    fn source_view(
        &self,
        cx: &Cx<'_, B>,
        link: &Link,
        entries: &[Entry],
        stored: &[Stored],
    ) -> Option<SourceView> {
        match link {
            Link::Event(e) => Some(
                entries
                    .iter()
                    .find(|en| en.header.seq == e.seq && en.header.replica == e.replica)
                    .map_or(SourceView::Purged, |en| {
                        SourceView::Event(timeline_entry(
                            self.space(),
                            en,
                            self.erase_cause(cx, en),
                            self.derived_from(stored, e),
                        ))
                    }),
            ),
            Link::Thing(t) => Some(
                entries
                    .iter()
                    .filter_map(|en| match &en.body {
                        eventlog::BodyState::Present(b) => Some(b),
                        eventlog::BodyState::Erased => None,
                    })
                    .flat_map(|b| b.things())
                    .find(|(v, _)| &v.thing == t)
                    .map_or(SourceView::Purged, |(v, _)| SourceView::Present(v.clone())),
            ),
            Link::Fact(_) | Link::Run(_) => None,
        }
    }

    /// Lands a proposal in a topic file or in `pending/`.
    pub(crate) async fn propose(
        &mut self,
        cx: &Cx<'_, B>,
        draft: FactDraft,
    ) -> Result<(FactId, FactState), Refusal> {
        let now = cx.now();
        let space = self.space().clone();
        if draft
            .links
            .iter()
            .any(|l| matches!(l, Link::Event(e) if e.space != space))
        {
            return Err(Refusal::OutsideSpace);
        }
        let entries = self.entries()?;
        let stored = self.stored()?;
        let cited: Vec<Label> = draft
            .links
            .iter()
            .filter_map(|l| link_label(l, &space, &entries, &stored))
            .collect();
        let label = proposal_label(cx.caller, &space, &cited);
        if space == SpaceId::desktop() {
            match desktop_admits(&label) {
                DesktopVerdict::Admit => {}
                refused => return Err(failed(format!("the desktop scope refuses: {refused:?}"))),
            }
        }
        if draft
            .supersedes
            .iter()
            .any(|old| !stored.iter().any(|s| &s.fact.id == old))
        {
            return Err(Refusal::NoSuchFact);
        }
        let id = FactId::mint(
            u64::try_from(now.0)
                .unwrap_or_default()
                .saturating_mul(1000),
            self.entropy(cx, draft.text.as_str().as_bytes()),
        );
        let fact = Fact {
            id: id.clone(),
            text: draft.text,
            recorded: now,
            by: caller_actor(cx.caller),
            label,
            links: draft.links,
            supersedes: draft.supersedes,
            valid: Validity::Unstated,
        };
        let (_, effects) = step(FactLife::Unborn, FactEvent::Propose(lands(&fact.label)));
        let mut state = FactState::Pending;
        for effect in effects {
            match effect {
                FactEffect::AppendToTopic => {
                    self.rt
                        .store
                        .append(&draft.topic, fact.clone())
                        .map_err(files_refusal)?;
                    state = FactState::Active;
                    self.index_drop(&fact.supersedes.iter().map(fact_doc_id).collect::<Vec<_>>());
                    self.index_put(cx, vec![fact_doc(&fact)]).await;
                }
                FactEffect::StageInPending => self
                    .rt
                    .store
                    .stage(fact.clone(), draft.topic.clone())
                    .map_err(files_refusal)?,
                FactEffect::LogAdded => {
                    self.audit(
                        now,
                        MemoryOp::FactAdded {
                            fact: id.clone(),
                            topic: draft.topic.clone(),
                        },
                    )?;
                }
                other => return Err(failed(format!("unexpected effect {other:?}"))),
            }
        }
        Ok((id, state))
    }

    /// Pending facts, after letting the old ones age out (14 days, QUESTIONS Me5).
    pub(crate) fn pending(&mut self, cx: &Cx<'_, B>) -> Result<Vec<FactView>, Refusal> {
        self.age_pending(cx.now())?;
        let stored = self.stored()?;
        let pending: Vec<&Stored> = stored
            .iter()
            .filter(|s| s.state == FactState::Pending)
            .collect();
        self.fact_views(cx, &pending)
    }

    /// Discards the pending facts that waited past their 14 days, and says so.
    pub(crate) fn age_pending(&mut self, now: UnixSeconds) -> Result<(), Refusal> {
        let old: Vec<FactId> = self
            .stored()?
            .into_iter()
            .filter(|s| s.state == FactState::Pending)
            .filter(|s| {
                let days = now.0.saturating_sub(s.fact.recorded.0) / SECONDS_PER_DAY;
                step(
                    FactLife::Pending,
                    FactEvent::Age(almanac_core::DayCount(
                        u32::try_from(days).unwrap_or(u32::MAX),
                    )),
                )
                .0 == FactLife::Removed
                    && days >= i64::from(PENDING_TTL_DAYS.0)
            })
            .map(|s| s.fact.id)
            .collect();
        let mut aged = false;
        for id in old {
            self.rt
                .store
                .settle(&id, Settlement::Discard)
                .map_err(files_refusal)?;
            self.audit(now, MemoryOp::FactRejected { fact: id })?;
            aged = true;
        }
        if aged {
            self.outbox
                .push(ServiceEvent::PendingChanged(self.space().clone()));
        }
        Ok(())
    }

    /// Keeps or discards a pending fact.
    pub(crate) async fn settle(
        &mut self,
        cx: &Cx<'_, B>,
        id: &FactId,
        settlement: Settlement,
    ) -> Result<(), Refusal> {
        let now = cx.now();
        let stored = self.stored()?;
        let life = match stored.iter().find(|s| &s.fact.id == id).map(|s| &s.state) {
            None => FactLife::Unborn,
            Some(FactState::Pending) => FactLife::Pending,
            Some(FactState::Active) => FactLife::Active,
            Some(FactState::Superseded { by }) => FactLife::Superseded(by.clone()),
        };
        let event = match settlement {
            Settlement::Keep(_) => FactEvent::Keep,
            Settlement::Discard => FactEvent::Discard,
        };
        let (_, effects) = step(life, event);
        if let Some(FactEffect::Refuse(why)) =
            effects.iter().find(|e| matches!(e, FactEffect::Refuse(_)))
        {
            return Err(why.clone());
        }
        let kept = matches!(settlement, Settlement::Keep(_));
        self.rt
            .store
            .settle(id, settlement)
            .map_err(files_refusal)?;
        if kept {
            self.audit(now, MemoryOp::FactConfirmed { fact: id.clone() })?;
            if let Some(s) = self.stored()?.into_iter().find(|s| &s.fact.id == id) {
                self.index_put(cx, vec![fact_doc(&s.fact)]).await;
            }
        } else {
            self.audit(now, MemoryOp::FactRejected { fact: id.clone() })?;
        }
        self.outbox
            .push(ServiceEvent::PendingChanged(self.space().clone()));
        Ok(())
    }

    /// The number of facts by state: active, pending.
    pub(crate) fn fact_counts(&self) -> Result<(Count, Count), Refusal> {
        let stored = self.stored()?;
        let n = |state: &FactState| {
            Count(
                u32::try_from(stored.iter().filter(|s| &s.state == state).count())
                    .unwrap_or(u32::MAX),
            )
        };
        Ok((n(&FactState::Active), n(&FactState::Pending)))
    }
}
