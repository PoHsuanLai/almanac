//! The baseline of the topic files: each file's text as the service last left it
//! (`meta/baseline.json` in the Space's vault, sealed like every file there). A file that no
//! longer says what the baseline says was changed by someone else, and the next consolidation
//! run reports it as an `ExternalEdit` (what changed is the difference between the two texts).
//!
//! The baseline is refreshed after every request that may write a topic file, except for topics
//! that were already different when the request began: those stay as they were so the person's
//! edit is not absorbed by the service's own write.

use almanac_core::{FactId, TopicPath};
use almanac_store::{Vault, VaultError, VaultPath};
use memfiles::{Block, parse_topic, render_topic};
use std::collections::BTreeMap;

fn path() -> Option<VaultPath> {
    VaultPath::parse("meta/baseline.json")
}

/// Topic file texts as the service last left them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Baseline {
    texts: BTreeMap<TopicPath, String>,
}

impl Baseline {
    /// The baseline the vault holds; empty when the file is absent or unreadable (the next
    /// request makes a fresh one from the files as they are).
    pub(crate) fn load(vault: &impl Vault) -> Self {
        let texts: BTreeMap<String, String> = path()
            .and_then(|p| vault.read(&p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            texts: texts
                .into_iter()
                .filter_map(|(topic, text)| Some((TopicPath::parse(&topic).ok()?, text)))
                .collect(),
        }
    }

    pub(crate) fn save(&self, vault: &impl Vault) -> Result<(), VaultError> {
        let texts: BTreeMap<String, String> = self
            .texts
            .iter()
            .map(|(t, text)| (t.to_string(), text.clone()))
            .collect();
        let bytes = serde_json::to_vec(&texts).map_err(|e| VaultError::Io(e.to_string()))?;
        match path() {
            Some(p) => vault.write_atomic(&p, &bytes),
            None => Err(VaultError::Io("baseline path".into())),
        }
    }

    pub(crate) fn get(&self, topic: &TopicPath) -> Option<&str> {
        self.texts.get(topic).map(String::as_str)
    }

    pub(crate) fn topics(&self) -> impl Iterator<Item = &TopicPath> {
        self.texts.keys()
    }

    pub(crate) fn set(&mut self, topic: TopicPath, text: String) {
        self.texts.insert(topic, text);
    }

    pub(crate) fn drop_topic(&mut self, topic: &TopicPath) {
        self.texts.remove(topic);
    }

    /// The facts in the baseline's texts, so a missing file can leave the index.
    pub(crate) fn fact_ids(&self, topic: &TopicPath) -> Vec<FactId> {
        self.get(topic)
            .and_then(|text| parse_topic(text).ok())
            .map(|file| {
                file.blocks
                    .into_iter()
                    .filter_map(|b| match b {
                        Block::Fact(f) => Some(f.id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The baseline without `gone`: forgetting a fact forgets it here too, so a pending edit's
    /// "before" cannot keep what the person erased.
    pub(crate) fn without(&mut self, gone: &[FactId], tz: &jiff::tz::TimeZone) {
        for text in self.texts.values_mut() {
            let Ok(mut file) = parse_topic(text) else {
                continue;
            };
            let before = file.blocks.len();
            file.blocks
                .retain(|b| !matches!(b, Block::Fact(f) if gone.contains(&f.id)));
            if file.blocks.len() != before {
                *text = render_topic(&file, tz);
            }
        }
    }
}
