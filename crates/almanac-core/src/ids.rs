//! The ids almanac mints or names: facts, topics, rules, plans, kinds, paths and the counters
//! of the log. Grammars are checked once where text enters.

use crate::text::{dotted, element_ok, from_hex, hex_of, text_id};
use porter_core::is_id;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Crockford base32 without `i l o u`, lowercase: the alphabet of fact ids.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// 26 lowercase Crockford digits whose first is at most `7` (128 bits).
fn is_fact_id(text: &str) -> bool {
    text.len() == 26
        && text.bytes().all(|b| ALPHABET.contains(&b))
        && text.as_bytes().first().is_some_and(|b| *b <= b'7')
}

/// 1 to 4 `/`-separated segments of `[a-z0-9-]`, none empty or starting with `-`, at most 128
/// bytes.
fn is_topic(text: &str) -> bool {
    let segments: Vec<&str> = text.split('/').collect();
    text.len() <= 128
        && (1..=4).contains(&segments.len())
        && segments.iter().all(|s| {
            !s.is_empty()
                && !s.starts_with('-')
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// A pattern over kind tags or thing kinds: `*`, an exact dotted name, or a dotted prefix
/// followed by `.*`.
fn is_kind_pattern(text: &str) -> bool {
    match text.strip_suffix(".*") {
        Some(prefix) => !prefix.is_empty() && prefix.split('.').all(element_ok),
        None => text == "*" || (!text.is_empty() && text.split('.').all(element_ok)),
    }
}

fn is_path(text: &str) -> bool {
    !text.is_empty() && text.len() <= 4096 && !text.chars().any(char::is_control)
}

fn is_fact_text(text: &str) -> bool {
    !text.trim().is_empty()
        && text.len() <= 2048
        && !text.chars().any(char::is_control)
        && !text.contains("<!--")
        && !text.contains("-->")
}

text_id!(
    /// One fact: 26 lowercase base32 characters, 48 bits of time then 80 random bits, so ids
    /// from two machines never collide and sort by creation.
    FactId,
    "fact id",
    is_fact_id
);
text_id!(
    /// Where a fact lives: `people/sam-lee`, `prefs/meetings`; up to four levels.
    TopicPath,
    "topic path",
    is_topic
);
text_id!(
    /// One remember rule, in porter's id grammar.
    RuleId,
    "rule id",
    is_id
);
text_id!(
    /// The token of one forget plan, in porter's id grammar.
    PlanToken,
    "plan token",
    is_id
);
text_id!(
    /// A header's kind (`thing.archived`, `file.created`): two or more dotted
    /// `[a-z][a-z0-9_]*` elements.
    KindTag,
    "kind tag",
    dotted
);
text_id!(
    /// Which kinds a rule covers: `*`, `files.file`, or `mail.*`.
    KindPattern,
    "kind pattern",
    is_kind_pattern
);
text_id!(
    /// A path as the Space sees it: 1 to 4096 bytes, no control characters.
    SpacePath,
    "space path",
    is_path
);
text_id!(
    /// A path glob: `*` matches within one directory, `**` across directories.
    PathGlob,
    "path glob",
    is_path
);
text_id!(
    redacted
    /// The text of one fact: one paragraph of at most 2 KiB with no control characters and no
    /// comment syntax, so the file's metadata trailer cannot be forged from inside it.
    FactText,
    "fact text",
    is_fact_text
);

impl FactId {
    /// The id for a fact learned at `unix_ms` with 80 injected random bits.
    pub fn mint(unix_ms: u64, random: [u8; 10]) -> FactId {
        let random_bits = random
            .iter()
            .fold(0u128, |acc, byte| acc << 8 | u128::from(*byte));
        let value = u128::from(unix_ms & 0xFFFF_FFFF_FFFF) << 80 | random_bits;
        Self::from_u128(value)
    }

    fn from_u128(value: u128) -> FactId {
        let text: String = (0..26)
            .map(|i| {
                let index = usize::try_from((value >> (5 * (25 - i))) & 31).unwrap_or(0);
                char::from(ALPHABET[index])
            })
            .collect();
        FactId(text)
    }
}

impl KindPattern {
    /// The pattern covering every kind.
    pub fn any() -> KindPattern {
        KindPattern("*".to_owned())
    }

    /// Whether a dotted kind name (a [`KindTag`] or a thing kind) is covered.
    pub fn covers(&self, kind: &str) -> bool {
        match self.0.strip_suffix(".*") {
            Some(prefix) => kind
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.')),
            None => self.0 == "*" || self.0 == kind,
        }
    }

    /// How narrow the pattern is: more elements and no star rank higher.
    pub fn specificity(&self) -> u32 {
        let elements = u32::try_from(self.0.split('.').count()).unwrap_or(u32::MAX);
        match self.0.as_str() {
            "*" => 0,
            text if text.ends_with(".*") => elements,
            _ => elements + 1,
        }
    }
}

impl KindTag {
    /// A tag from two parts that are known to be elements.
    pub(crate) fn of(head: &str, tail: &str) -> KindTag {
        KindTag(format!("{head}.{tail}"))
    }
}

/// A per-(Space, replica) sequence number, from 1 and gapless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seq(pub u64);

/// One replica of a Space on one machine: room for sync, one per Space per machine.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplicaId(pub [u8; 16]);

impl fmt::Debug for ReplicaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReplicaId({})", hex_of(&self.0))
    }
}

impl Serialize for ReplicaId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex_of(&self.0))
    }
}

impl<'de> Deserialize<'de> for ReplicaId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        from_hex::<16>(&text)
            .map(Self)
            .ok_or_else(|| serde::de::Error::custom("expected 32 lowercase hex digits"))
    }
}

/// A number of days (retention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DayCount(pub u32);

/// How often a fact was read into a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UseCount(pub u32);

/// An opaque position in a timeline, newest first: the events strictly before this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(pub Seq);
