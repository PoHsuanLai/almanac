//! Writers: `Record`, `ExplainFile`, `Mark`.

use crate::backend::Backend;
use crate::docs::{event_doc_id, event_ref, indexed_docs, record_fault};
use crate::open::{Cx, Open, failed};
use crate::space::{SpaceEffect, SpaceEvent, step};
use almanac_core::{
    Admission, Confidentiality, Count, DataClass, DropReason, EventBody, EventRef, FileChange,
    FileView, FileWhy, FileWhyClaim, IndexPart, Integrity, KindPattern, Label, MarkKind,
    MarkRequest, Record, Refusal, Source, Verb, admit_with, withheld,
};
use eventlog::{BodyState, LogRead};
use std::collections::BTreeSet;

/// What a record came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stored {
    /// In the log.
    Event(EventRef),
    /// Admission dropped it (paused, a rule, a mark): nothing was kept.
    Dropped(DropReason),
}

/// A message lands in the receiving Space, with the sender's label.
pub(crate) fn land_in_receiver(record: &mut Record) {
    if let EventBody::Message(m) = &record.body {
        record.space = m.to.space.clone();
    }
}

impl<B: Backend> Open<B> {
    /// Lets time pass: a pause that has run out ends, and says so in the log.
    pub(crate) fn tick(&mut self, cx: &Cx<'_, B>) -> Result<(), Refusal> {
        let now = cx.now();
        let (state, effects) = step(self.rt.state, SpaceEvent::Tick { now });
        self.rt.state = state;
        for effect in effects {
            if let SpaceEffect::LogResumed = effect {
                self.audit(now, almanac_core::MemoryOp::Resumed)?;
            }
        }
        Ok(())
    }

    /// Records one event: checks it, asks admission, chains it and indexes its documents.
    pub(crate) async fn record(
        &mut self,
        cx: &Cx<'_, B>,
        record: Record,
    ) -> Result<Stored, Refusal> {
        if let Some(why) = record_fault(&record) {
            return Err(failed(why));
        }
        self.tick(cx)?;
        let now = cx.now();
        let (body, header_only) = match admit_with(
            &record,
            &cx.rules,
            &self.rt.state,
            &self.rt.marks,
            cx.settings.retention.file_unexplained,
        ) {
            Admission::Drop(why) => return Ok(Stored::Dropped(why)),
            Admission::Keep { .. } => (Some(record.body.clone()), false),
            Admission::HeaderOnly { .. } => (None, true),
        };
        let entry = self.append(now, &record, body)?;
        if header_only {
            self.notes.header_only.insert(entry.header.seq);
        } else {
            self.index_entry(cx, &entry).await;
        }
        Ok(Stored::Event(event_ref(self.space(), &entry)))
    }

    /// Records one event for a durable append: refuses, with the reason, whenever the body would
    /// not be stored whole (a pause, a mark, a `Never` or `HeaderOnly` rule keep at most a header,
    /// even for audit-class records such as a session's), and otherwise answers where it is.
    pub(crate) async fn record_whole(
        &mut self,
        cx: &Cx<'_, B>,
        record: Record,
    ) -> Result<EventRef, Refusal> {
        if let Some(why) = record_fault(&record) {
            return Err(failed(why));
        }
        self.tick(cx)?;
        if let Some(why) = withheld(&record, &cx.rules, &self.rt.state, &self.rt.marks) {
            return Err(Refusal::NotKept(why));
        }
        match self.record(cx, record).await? {
            Stored::Event(event) => Ok(event),
            Stored::Dropped(why) => Err(Refusal::NotKept(why)),
        }
    }

    /// Indexes the documents of a new entry; a narrated episode replaces its earlier events'.
    pub(crate) async fn index_entry(&mut self, cx: &Cx<'_, B>, entry: &eventlog::Entry) {
        if let BodyState::Present(EventBody::Episode(episode)) = &entry.body {
            let older = self.older_episode_docs(&episode.id.to_string(), entry.header.seq);
            self.index_drop(&older);
        }
        let newest = BTreeSet::from([entry.header.seq]);
        let docs = indexed_docs(self.space(), entry, &newest)
            .into_iter()
            .map(|d| d.into_doc(entry.header.occurred))
            .collect();
        self.index_put(cx, docs).await;
    }

    /// The document ids of earlier events of the same episode.
    fn older_episode_docs(&self, id: &str, before: almanac_core::Seq) -> Vec<String> {
        let Ok(kind) = KindPattern::parse("companion.episode") else {
            return Vec::new();
        };
        let mut filter = crate::open::any_filter();
        filter.kinds = vec![kind];
        let query = almanac_core::TimelineQuery {
            before: Some(almanac_core::Cursor(before)),
            limit: Count(u32::MAX),
            filter,
        };
        let entries = self.rt.log.page(&query).unwrap_or_default();
        entries
            .iter()
            .filter(|e| {
                matches!(&e.body, BodyState::Present(EventBody::Episode(ep)) if ep.id.as_str() == id)
            })
            .flat_map(|e| {
                let at = event_ref(self.space(), e);
                [event_doc_id(&at), IndexPart::Narrative.doc_id(&at)]
            })
            .collect()
    }

    /// An app saying why a file changed, recorded as the app said (the join with what the
    /// watcher saw is memoryd's watch loop; a claim that arrives alone is the "why alone" row).
    pub(crate) async fn explain(
        &mut self,
        cx: &Cx<'_, B>,
        claim: FileWhyClaim,
    ) -> Result<Stored, Refusal> {
        let label = Label {
            integrity: Integrity::Trusted,
            confidentiality: Confidentiality::Private(BTreeSet::from([claim.space.clone()])),
            classes: BTreeSet::<DataClass>::new(),
            sources: BTreeSet::from([Source::App(claim.cause.app.clone())]),
        };
        let record = Record {
            space: claim.space,
            occurred: cx.now(),
            actor: claim.by.clone(),
            effect: almanac_core::Effect::UndoableWrite,
            label,
            body: EventBody::File {
                change: change_for(claim.verb),
                file: FileView {
                    path: claim.path,
                    inode: 0,
                    content: claim.content,
                },
                why: FileWhy::Explained {
                    cause: claim.cause,
                    verb: claim.verb,
                    by: claim.by,
                },
            },
            cause: almanac_core::Cause::None,
        };
        self.record(cx, record).await
    }

    /// Marks or unmarks a thing "do not remember".
    pub(crate) fn mark(&mut self, request: MarkRequest) -> Result<(), Refusal> {
        match request.mark {
            MarkKind::DoNotRemember => {
                self.rt.marks.things.insert(request.thing);
            }
            MarkKind::Clear => {
                self.rt.marks.things.remove(&request.thing);
            }
        }
        crate::marks::save(self.rt.store.vault(), &self.rt.marks).map_err(failed)
    }
}

/// The change an app's verb implies when no filesystem observation came with it (the same table
/// as `almanac-watch::change_for`).
fn change_for(verb: Verb) -> FileChange {
    match verb {
        Verb::Created | Verb::Downloaded | Verb::Imported | Verb::Copied | Verb::Exported => {
            FileChange::Created
        }
        Verb::Edited | Verb::Saved => FileChange::Modified,
        Verb::Deleted => FileChange::Deleted,
        _ => FileChange::Closed,
    }
}
