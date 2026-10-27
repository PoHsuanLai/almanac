//! When a model's tidy of a topic file is acceptable: the same topic with the same facts, so a
//! tidy cannot launder model words into a trusted fact or plant a bullet. Pure checks over the
//! topic file format.

use almanac_core::{Fact, Integrity};
use memfiles_format::{Block, TopicFile, parse_topic};

/// The bullets and lines in a file that are not facts, as sorted keys: a tidy must leave this
/// multiset as it was (it may not add a bullet that a later run would stamp as the person's).
fn loose_blocks(file: &TopicFile) -> Vec<String> {
    let mut keys: Vec<String> = file
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Unstamped(t) => Some(format!("bullet:{t}")),
            Block::Verbatim(l) => Some(format!("line:{l}")),
            Block::Fact(_) => None,
        })
        .collect();
    keys.sort();
    keys
}

/// A tidied file is acceptable when it is the same topic with the same facts: every fact keeps its
/// id, author, label, links and date, and only the text of a fact that is not trusted may change
/// (rewording a trusted fact is the person's to do, so a tidy cannot launder model words into
/// it). The unstamped bullets and verbatim lines stay exactly as they were (a tidy adds no
/// knowledge, and cannot plant a bullet for a later run to stamp). Headings and the title are
/// free.
pub fn tidy_accepts(current: &TopicFile, after: &TopicFile) -> bool {
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
        && loose_blocks(current) == loose_blocks(after)
        && was.len() == now.len()
        && was.iter().all(|old| {
            now.iter().filter(|new| new.id == old.id).any(|new| {
                let reworded = new.text != old.text;
                let may_reword = !matches!(old.label.integrity, Integrity::Trusted);
                (may_reword || !reworded)
                    && Fact {
                        text: old.text.clone(),
                        ..new.clone()
                    } == *old
            })
        })
}

/// Whether `after` is an acceptable tidy of the file `before` (both as text): both parse and
/// [`tidy_accepts`] says so. A consolidator checks its own tidies with this before it proposes
/// them.
pub fn tidy_is_acceptable(before: &str, after: &str) -> bool {
    match (parse_topic(before), parse_topic(after)) {
        (Ok(current), Ok(tidied)) => tidy_accepts(&current, &tidied),
        _ => false,
    }
}
