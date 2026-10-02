//! The topic file: front matter, dated headings, one bullet per fact with its trailer.
//!
//! ```markdown
//! ---
//! format: quire-memory 1
//! topic: people/sam-lee
//! title: Sam Lee
//! ---
//!
//! ## 2026-10-01
//!
//! - Prefers meetings after 10:00.
//!   <!-- fact: ...; at: 2026-10-01T09:12:44Z; by: ...; label: ... -->
//! ```
//!
//! Date headings are derived from each fact's `at` in the injected time zone: parsing skips
//! them and rendering writes one before the first fact of each local day. A bullet without a
//! trailer is a person's edit (`Block::Unstamped`); any other non-blank line is kept as
//! `Block::Verbatim`. Contract: `render_topic(&parse_topic(s)?, tz) == s` for any `s` that
//! `render_topic` produced.

use crate::trailer::{TrailerFault, parse_trailer, render_trailer, timestamp};
use almanac_core::{Fact, FactText, TopicPath};
use jiff::tz::TimeZone;

/// What the first front matter line must say.
pub const FORMAT_LINE: &str = "quire-memory 1";

/// One block of a topic file.
// A stamped fact is the large case and the common one; boxing it would complicate every match.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A stamped fact.
    Fact(Fact),
    /// A bullet the person wrote: the text after `- `. Consolidation stamps it.
    Unstamped(String),
    /// One other non-blank line, kept as it is. It must not start with `- ` or look like a
    /// date heading, or parsing would read it back as something else.
    Verbatim(String),
}

/// A parsed topic file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicFile {
    /// Which topic.
    pub topic: TopicPath,
    /// Its title, one line.
    pub title: String,
    /// Its blocks, in file order.
    pub blocks: Vec<Block>,
}

/// Why a topic file was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The file does not start with `---`.
    #[error("missing front matter")]
    MissingFrontMatter,
    /// The front matter has no closing `---`.
    #[error("front matter is not closed")]
    UnclosedFrontMatter,
    /// A format this build does not know.
    #[error("unknown format {0:?}")]
    UnknownFormat(String),
    /// A required front matter key is missing.
    #[error("missing front matter key {0}")]
    MissingKey(&'static str),
    /// A front matter key this format does not have.
    #[error("unknown front matter key {0}")]
    UnknownKey(String),
    /// The topic is not a topic path.
    #[error("bad topic {0:?}")]
    BadTopic(String),
    /// A bullet's trailer was refused (1-based line of the trailer).
    #[error("line {line}: {fault}")]
    BadTrailer {
        /// Where.
        line: usize,
        /// Why.
        fault: TrailerFault,
    },
    /// A stamped bullet's text is not fact text (1-based line).
    #[error("line {line}: bullet is not fact text")]
    BadFactText {
        /// Where.
        line: usize,
    },
}

fn is_date_heading(line: &str) -> bool {
    line.strip_prefix("## ").is_some_and(|d| {
        d.len() == 10
            && d.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
    })
}

/// Parses a topic file. Strict on the front matter and on trailers, lenient everywhere else.
pub fn parse_topic(text: &str) -> Result<TopicFile, ParseError> {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.first() != Some(&"---") {
        return Err(ParseError::MissingFrontMatter);
    }
    let close = lines[1..]
        .iter()
        .position(|l| *l == "---")
        .ok_or(ParseError::UnclosedFrontMatter)?
        + 1;
    let (mut format, mut topic, mut title) = (None, None, None);
    for line in &lines[1..close] {
        let (key, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match key {
            "format" => format = Some(value),
            "topic" => topic = Some(value),
            "title" => title = Some(value),
            other => return Err(ParseError::UnknownKey(other.to_owned())),
        }
    }
    let format = format.ok_or(ParseError::MissingKey("format"))?;
    if format != FORMAT_LINE {
        return Err(ParseError::UnknownFormat(format.to_owned()));
    }
    let topic_text = topic.ok_or(ParseError::MissingKey("topic"))?;
    let topic =
        TopicPath::parse(topic_text).map_err(|_| ParseError::BadTopic(topic_text.to_owned()))?;
    let title = title.ok_or(ParseError::MissingKey("title"))?.to_owned();
    let mut blocks = Vec::new();
    let mut i = close + 1;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.is_empty() || is_date_heading(line) {
            continue;
        }
        let Some(bullet) = line.strip_prefix("- ") else {
            blocks.push(Block::Verbatim(line.to_owned()));
            continue;
        };
        match lines.get(i).filter(|next| next.starts_with("  <!-- fact:")) {
            Some(trailer) => {
                let fact_text =
                    FactText::parse(bullet).map_err(|_| ParseError::BadFactText { line: i })?;
                let fact = parse_trailer(trailer, fact_text)
                    .map_err(|fault| ParseError::BadTrailer { line: i + 1, fault })?;
                blocks.push(Block::Fact(fact));
                i += 1;
            }
            None => blocks.push(Block::Unstamped(bullet.to_owned())),
        }
    }
    Ok(TopicFile {
        topic,
        title,
        blocks,
    })
}

/// Renders a topic file; dated headings use `tz`.
pub fn render_topic(file: &TopicFile, tz: &TimeZone) -> String {
    let mut out = format!(
        "---\nformat: {FORMAT_LINE}\ntopic: {}\ntitle: {}\n---\n\n",
        file.topic, file.title
    );
    let mut heading: Option<String> = None;
    for (index, block) in file.blocks.iter().enumerate() {
        match block {
            Block::Fact(fact) => {
                let date = timestamp(fact.recorded)
                    .to_zoned(tz.clone())
                    .date()
                    .to_string();
                if heading.as_deref() != Some(date.as_str()) {
                    if index > 0 {
                        out.push('\n');
                    }
                    out.push_str(&format!("## {date}\n\n"));
                    heading = Some(date);
                }
                out.push_str(&format!("- {}\n{}\n", fact.text, render_trailer(fact)));
            }
            Block::Unstamped(text) => out.push_str(&format!("- {text}\n")),
            Block::Verbatim(line) => out.push_str(&format!("{line}\n")),
        }
    }
    out
}
