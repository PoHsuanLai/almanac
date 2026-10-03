//! Topic files edited outside the service: found by comparing each file with the baseline
//! (`baseline.rs`) and reported by the next consolidation run as `ExternalEdit` hunks.

use crate::backend::Backend;
use crate::consolidation::InputTopic;
use crate::docs::fact_doc_id;
use crate::open::{Open, files_refusal};
use almanac_core::{Hunk, Refusal, TopicPath, UserText};
use memfiles::render_topic;
use std::collections::BTreeMap;

impl<B: Backend> Open<B> {
    /// Every topic file as text.
    pub(crate) fn topic_texts(&self) -> Result<BTreeMap<TopicPath, String>, Refusal> {
        let store = &self.rt.store;
        store
            .topics()
            .map_err(files_refusal)?
            .into_iter()
            .map(|topic| {
                let file = store.read(&topic).map_err(files_refusal)?;
                Ok((topic, render_topic(&file, store.tz())))
            })
            .collect()
    }

    /// The topic files as the consolidator reads them.
    pub(crate) fn input_topics(&self) -> Result<Vec<InputTopic>, Refusal> {
        Ok(self
            .topic_texts()?
            .into_iter()
            .map(|(topic, text)| InputTopic {
                topic,
                text: UserText::new(text),
            })
            .collect())
    }

    /// Before a request that may write topic files: notes which files are not what the
    /// baseline says (someone edited them), makes a baseline for files it has not seen, and
    /// forgets files that are gone (their facts leave the index).
    pub(crate) fn guard_topics(&mut self) -> Result<(), Refusal> {
        let now = self.topic_texts()?;
        self.dirty.clear();
        let mut changed = false;
        for (topic, text) in &now {
            match self.baseline.get(topic) {
                None => {
                    self.baseline.set(topic.clone(), text.clone());
                    changed = true;
                }
                Some(was) if was != text => {
                    self.dirty.insert(topic.clone());
                }
                Some(_) => {}
            }
        }
        let missing: Vec<TopicPath> = self
            .baseline
            .topics()
            .filter(|t| !now.contains_key(*t))
            .cloned()
            .collect();
        for topic in missing {
            let docs: Vec<String> = self
                .baseline
                .fact_ids(&topic)
                .iter()
                .map(fact_doc_id)
                .collect();
            self.index_drop(&docs);
            self.baseline.drop_topic(&topic);
            changed = true;
        }
        if changed {
            self.save_baseline()?;
        }
        Ok(())
    }

    /// After a request that may have written topic files: the files are what the service left,
    /// except those that were already someone else's when it began.
    pub(crate) fn accept_topics(&mut self) -> Result<(), Refusal> {
        let now = self.topic_texts()?;
        let mut changed = false;
        for (topic, text) in now {
            if !self.dirty.contains(&topic) && self.baseline.get(&topic) != Some(text.as_str()) {
                self.baseline.set(topic, text);
                changed = true;
            }
        }
        if changed {
            self.save_baseline()?;
        }
        Ok(())
    }

    fn save_baseline(&self) -> Result<(), Refusal> {
        self.baseline
            .save(self.rt.store.vault())
            .map_err(crate::open::failed)
    }

    /// One `ExternalEdit` for every file found edited by the last `guard_topics`: the baseline
    /// text and the file as it is now.
    pub(crate) fn external_edit_hunks(&self) -> Result<Vec<Hunk>, Refusal> {
        let now = self.topic_texts()?;
        Ok(self
            .dirty
            .iter()
            .filter_map(|topic| {
                Some(Hunk::ExternalEdit {
                    topic: topic.clone(),
                    before: UserText::new(self.baseline.get(topic)?.to_owned()),
                    after: UserText::new(now.get(topic)?.clone()),
                })
            })
            .collect())
    }

    /// The edit of `topic` was reported and applied: the file as it is now is the baseline.
    pub(crate) fn absorb_edit(&mut self, topic: &TopicPath) {
        self.dirty.remove(topic);
    }

    /// Forgetting facts forgets them in the baseline too.
    pub(crate) fn forget_in_baseline(
        &mut self,
        gone: &[almanac_core::FactId],
    ) -> Result<(), Refusal> {
        self.baseline.without(gone, self.rt.store.tz());
        self.save_baseline()
    }
}
