//! The topic-file hunks a consolidation run applies by itself (Me4): Tidy and Stamp rewrite a
//! topic file, ExternalEdit refreshes what the index knows of one, Flag records a note for the
//! person. The pure checks are free functions; the effects take a pre-image first so a revert can
//! put every file back.

use crate::backend::Backend;
use crate::docs::{fact_doc, fact_doc_id};
use crate::open::{Cx, Open, failed, files_refusal};
use crate::search::caller_actor;
use almanac_core::{
    Caller, Fact, FactId, FactState, FactText, Hunk, Label, MemoryOp, Refusal, RunId, TidyHunk,
    TopicPath, UnixSeconds, UserText, Validity,
};
use memfiles::{Block, TopicFile, Vault, VaultPath, parse_topic, render_topic};

/// What a file held before a hunk changed it: restoring it undoes the hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreImage {
    /// The file.
    pub path: VaultPath,
    /// Its bytes, or `None` when the hunk created it.
    pub bytes: Option<Vec<u8>>,
}

/// A tidied file is acceptable when it is the same topic with the same facts: every fact keeps its
/// id, author, label, links and date and only its text may change (a tidy rewords, it neither
/// adds nor removes knowledge, and it cannot launder a label). Everything else (headings,
/// verbatim lines, unstamped bullets, the title) is free.
pub(crate) fn tidy_accepts(current: &TopicFile, after: &TopicFile) -> bool {
    let facts = |file: &TopicFile| -> Vec<Fact> {
        file.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Fact(f) => Some(f.clone()),
                _ => None,
            })
            .collect()
    };
    let (was, now) = (facts(current), facts(after));
    after.topic == current.topic
        && was.len() == now.len()
        && was.iter().all(|old| {
            now.iter().filter(|new| new.id == old.id).any(|new| {
                Fact {
                    text: old.text.clone(),
                    ..new.clone()
                } == *old
            })
        })
}

/// The file with its first unstamped bullet reading `text` replaced by `stamped`, or `None` when
/// no such bullet is there.
pub(crate) fn stamp_in(current: &TopicFile, text: &str, stamped: Fact) -> Option<TopicFile> {
    let index = current
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Unstamped(t) if t.trim() == text.trim()))?;
    let mut file = current.clone();
    file.blocks[index] = Block::Fact(stamped);
    Some(file)
}

fn unix_ms(now: UnixSeconds) -> u64 {
    u64::try_from(now.0)
        .unwrap_or_default()
        .saturating_mul(1000)
}

impl<B: Backend> Open<B> {
    pub(crate) fn topic_pre_image(&self, topic: &TopicPath) -> PreImage {
        let path = VaultPath::topic(topic);
        PreImage {
            bytes: self.rt.store.vault().read(&path).ok(),
            path,
        }
    }

    fn write_topic(&self, file: &TopicFile) -> Result<(), Refusal> {
        let text = render_topic(file, self.rt.store.tz());
        self.rt
            .store
            .vault()
            .write_atomic(&VaultPath::topic(&file.topic), text.as_bytes())
            .map_err(failed)
    }

    /// Tidy: replaces a topic file by its reworded form, if the file is still what the draft was
    /// made from and the rewording changes no fact but its text. Returns the pre-image, or `None`
    /// when the hunk no longer applies (the person edited the file meanwhile, or the rewording
    /// is not a tidy).
    pub(crate) fn apply_tidy(&self, hunk: &TidyHunk) -> Result<Option<PreImage>, Refusal> {
        let Ok(current) = self.rt.store.read(&hunk.topic) else {
            return Ok(None);
        };
        let rendered = render_topic(&current, self.rt.store.tz());
        let Ok(after) = parse_topic(hunk.after.as_str()) else {
            return Ok(None);
        };
        if rendered.trim() != hunk.before.as_str().trim() || !tidy_accepts(&current, &after) {
            return Ok(None);
        }
        let pre = self.topic_pre_image(&hunk.topic);
        self.write_topic(&after)?;
        Ok(Some(pre))
    }

    /// The Stamp hunks a run adds by itself: one for every bullet a person wrote in a topic file
    /// that has no trailer yet (no model is needed to stamp it).
    pub(crate) fn stamp_hunks(&self) -> Result<Vec<Hunk>, Refusal> {
        let mut out = Vec::new();
        for topic in self.rt.store.topics().map_err(files_refusal)? {
            let file = self.rt.store.read(&topic).map_err(files_refusal)?;
            out.extend(file.blocks.into_iter().filter_map(|b| match b {
                Block::Unstamped(text) => Some(Hunk::Stamp {
                    topic: topic.clone(),
                    text: UserText::new(text),
                }),
                Block::Fact(_) | Block::Verbatim(_) => None,
            }));
        }
        Ok(out)
    }

    /// Stamp: gives a bullet the person wrote an id, a date and the person as its author.
    /// Returns the pre-image and the new fact, or `None` when the bullet is gone or not fact text.
    pub(crate) fn apply_stamp(
        &mut self,
        cx: &Cx<'_, B>,
        topic: &TopicPath,
        text: &UserText,
    ) -> Result<Option<(PreImage, Fact)>, Refusal> {
        let Ok(current) = self.rt.store.read(topic) else {
            return Ok(None);
        };
        let Ok(fact_text) = FactText::parse(text.as_str().trim()) else {
            return Ok(None);
        };
        let now = cx.now();
        let id = FactId::mint(unix_ms(now), self.entropy(cx, text.as_str().as_bytes()));
        let fact = Fact {
            id,
            text: fact_text,
            recorded: now,
            by: caller_actor(&Caller::ShellUi),
            label: Label::trusted_user(),
            links: Vec::new(),
            supersedes: Vec::new(),
            valid: Validity::Unstated,
        };
        let Some(file) = stamp_in(&current, text.as_str(), fact.clone()) else {
            return Ok(None);
        };
        let pre = self.topic_pre_image(topic);
        self.write_topic(&file)?;
        self.audit(
            now,
            MemoryOp::FactAdded {
                fact: fact.id.clone(),
                topic: topic.clone(),
            },
        )?;
        Ok(Some((pre, fact)))
    }

    /// ExternalEdit: the person already changed the file; the index learns it. Facts the file
    /// still holds are indexed again with their present text, and facts the earlier text had that
    /// are gone leave the index.
    pub(crate) async fn apply_external_edit(
        &mut self,
        cx: &Cx<'_, B>,
        topic: &TopicPath,
        before: &UserText,
    ) -> Result<(), Refusal> {
        let now_facts: Vec<Fact> = self
            .rt
            .store
            .read(topic)
            .map_err(files_refusal)?
            .blocks
            .into_iter()
            .filter_map(|b| match b {
                Block::Fact(f) => Some(f),
                _ => None,
            })
            .collect();
        let gone: Vec<String> = parse_topic(before.as_str())
            .map(|old| {
                old.blocks
                    .into_iter()
                    .filter_map(|b| match b {
                        Block::Fact(f) if !now_facts.iter().any(|n| n.id == f.id) => {
                            Some(fact_doc_id(&f.id))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.index_drop(&gone);
        let stored = self.stored()?;
        let docs = now_facts
            .iter()
            .filter(|f| {
                stored
                    .iter()
                    .any(|s| s.fact.id == f.id && s.state == FactState::Active)
            })
            .map(fact_doc)
            .collect();
        self.index_put(cx, docs).await;
        Ok(())
    }

    /// Flag: keeps the model's note about facts that look wrong, for the person to read next to
    /// the run (`flags/<run>.json` in the vault). Nothing about the facts changes.
    pub(crate) fn apply_flag(
        &self,
        run: &RunId,
        facts: &[FactId],
        note: &UserText,
    ) -> Result<(), Refusal> {
        let Some(path) = VaultPath::parse(&format!("flags/{run}.json")) else {
            return Err(failed("flag path"));
        };
        let vault = self.rt.store.vault();
        let mut notes: Vec<serde_json::Value> = vault
            .read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        notes.push(serde_json::json!({
            "facts": facts.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "note": note.as_str(),
        }));
        let bytes = serde_json::to_vec(&notes).map_err(failed)?;
        vault.write_atomic(&path, &bytes).map_err(failed)
    }

    /// Puts files back as they were, last change first.
    pub(crate) fn restore(&self, pre_images: &[PreImage]) -> Result<(), Refusal> {
        let vault = self.rt.store.vault();
        for pre in pre_images.iter().rev() {
            match &pre.bytes {
                Some(bytes) => vault.write_atomic(&pre.path, bytes).map_err(failed)?,
                None => drop(vault.remove(&pre.path)),
            }
        }
        Ok(())
    }
}
