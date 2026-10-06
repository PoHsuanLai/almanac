//! The lenient reader (design/22 section 2): a bad value falls back to the base value for that
//! key alone, never fails the file, and is reported so the daemon can log it.

use super::MemorySettings;
use super::keys::{Rule, table};
use std::collections::BTreeSet;
use std::fmt;

/// Why one key (or the whole file) was not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The text is not TOML; every key keeps its base value.
    NotToml(String),
    /// The value is not a whole number.
    NotANumber,
    /// The number is outside the key's range.
    OutOfRange {
        /// The smallest the key takes.
        min: i64,
        /// The largest the key takes.
        max: i64,
    },
    /// The value is not one of the key's words.
    NotAWord {
        /// The words the key takes.
        words: &'static [&'static str],
    },
}

impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Why::NotToml(why) => write!(f, "not TOML: {why}"),
            Why::NotANumber => write!(f, "not a whole number"),
            Why::OutOfRange { min, max } => write!(f, "outside {min}..={max}"),
            Why::NotAWord { words } => write!(f, "not one of {}", words.join(", ")),
        }
    }
}

/// A key whose value was refused and fell back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fallback {
    /// The dotted key, or `settings.toml` for a file that is not TOML.
    pub key: String,
    /// Why.
    pub why: Why,
}

/// What a read gives: the values, and what was refused or not recognised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The values in force.
    pub value: MemorySettings,
    /// Keys that fell back to their base value.
    pub fallbacks: Vec<Fallback>,
    /// Dotted paths no key reads (reported, not kept).
    pub unknown: Vec<String>,
}

impl Loaded {
    /// A read of nothing: the base values, nothing to report.
    pub fn of(value: MemorySettings) -> Self {
        Self {
            value,
            fallbacks: Vec::new(),
            unknown: Vec::new(),
        }
    }

    /// One line per fallback and per unknown key, for the daemon's log.
    pub fn lines(&self, daemon: &str) -> Vec<String> {
        let fell = self.fallbacks.iter().map(|f| {
            format!(
                "{daemon}: settings: {}: {}; using the previous value",
                f.key, f.why
            )
        });
        let unknown = self
            .unknown
            .iter()
            .map(|path| format!("{daemon}: settings: {path}: no such key, ignored"));
        fell.chain(unknown).collect()
    }
}

fn find<'a>(table: &'a toml::Table, path: &str) -> Option<&'a toml::Value> {
    let mut parts = path.split('.');
    let mut value = table.get(parts.next()?)?;
    for part in parts {
        value = value.as_table()?.get(part)?;
    }
    Some(value)
}

fn leaves(prefix: &str, table: &toml::Table, out: &mut Vec<String>) {
    for (name, value) in table {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match value {
            toml::Value::Table(inner) => leaves(&path, inner, out),
            _ => out.push(path),
        }
    }
}

/// Reads `text` over `base`: each key the file sets and the table accepts replaces its base
/// value; every other key keeps it.
pub fn read(text: &str, base: MemorySettings) -> Loaded {
    let doc = match text.parse::<toml::Table>() {
        Ok(doc) => doc,
        Err(why) => {
            return Loaded {
                value: base,
                fallbacks: vec![Fallback {
                    key: "settings.toml".to_owned(),
                    why: Why::NotToml(why.message().to_owned()),
                }],
                unknown: Vec::new(),
            };
        }
    };
    let mut value = base;
    let mut fallbacks = Vec::new();
    let keys = table();
    for key in &keys {
        let Some(found) = find(&doc, key.path) else {
            continue;
        };
        let refused = match &key.rule {
            Rule::Number { range, set } => match found.as_integer() {
                None => Some(Why::NotANumber),
                Some(n) if range.contains(&n) => {
                    set(&mut value, n);
                    None
                }
                Some(_) => Some(Why::OutOfRange {
                    min: *range.start(),
                    max: *range.end(),
                }),
            },
            Rule::Word { words, set } => {
                match found
                    .as_str()
                    .and_then(|w| words.iter().position(|x| *x == w))
                {
                    Some(at) => {
                        set(&mut value, at);
                        None
                    }
                    None => Some(Why::NotAWord { words }),
                }
            }
        };
        fallbacks.extend(refused.map(|why| Fallback {
            key: key.path.to_owned(),
            why,
        }));
    }
    let known: BTreeSet<&str> = keys.iter().map(|k| k.path).chain(["version"]).collect();
    let mut seen = Vec::new();
    leaves("", &doc, &mut seen);
    let unknown = seen
        .into_iter()
        .filter(|path| !known.contains(path.as_str()))
        .collect();
    Loaded {
        value,
        fallbacks,
        unknown,
    }
}
