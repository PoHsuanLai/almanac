//! The metadata trailer under a stamped bullet:
//!
//! ```text
//! - Prefers meetings after 10:00.
//!   <!-- fact: 01j9zk3m0q8h2v6x4c1b7n5t2a; at: 2026-10-01T09:12:44Z; by: {"kind":"user","v":{"via":"org.quire.Mail"}}; label: {...}; from: [{"kind":"thing","v":{...}}]; supersedes: ["01j9..."] -->
//! ```
//!
//! Keys, in the order written: `fact`, `at` (UTC, RFC 3339 with `Z`), `by` (the actor's serde
//! JSON), `label` (the label's serde JSON), then `from` (links) and `supersedes` when not
//! empty. Values are escaped (`%` as `%25`, `;` as `%3B`, `>` as `%3E`) so no value can end the
//! comment or a field. The keys `valid` and `origin` are reserved: a trailer carrying them is
//! refused, not interpreted. Parsing is strict.

use almanac_core::{Actor, Fact, FactId, FactText, Label, Link, UnixSeconds, Validity};
use jiff::Timestamp;

const OPEN: &str = "  <!-- ";
const CLOSE: &str = " -->";

/// Why a trailer was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrailerFault {
    /// Not `  <!-- ... -->`.
    #[error("not a comment")]
    NotAComment,
    /// A key is missing.
    #[error("missing key {0}")]
    MissingKey(&'static str),
    /// A key this format does not have.
    #[error("unknown key {0}")]
    UnknownKey(String),
    /// A key reserved for later (`valid`, `origin`).
    #[error("reserved key {0}")]
    Reserved(String),
    /// A key appears twice.
    #[error("duplicate key {0}")]
    Duplicate(&'static str),
    /// A value is not well-formed.
    #[error("bad value for {0}")]
    BadValue(&'static str),
}

fn escape(text: &str) -> String {
    text.replace('%', "%25")
        .replace(';', "%3B")
        .replace('>', "%3E")
}

fn unescape(text: &str) -> String {
    text.replace("%3E", ">")
        .replace("%3B", ";")
        .replace("%25", "%")
}

const MIN_SECONDS: i64 = -377_705_023_201;
const MAX_SECONDS: i64 = 253_402_207_200;

/// The instant, clamped into the range the calendar can print.
pub(crate) fn timestamp(at: UnixSeconds) -> Timestamp {
    Timestamp::from_second(at.0.clamp(MIN_SECONDS, MAX_SECONDS)).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// The trailer line (without a newline) for `fact`.
pub fn render_trailer(fact: &Fact) -> String {
    let mut parts = vec![
        format!("fact: {}", fact.id),
        format!("at: {}", timestamp(fact.recorded)),
        format!("by: {}", escaped_json(&fact.by)),
        format!("label: {}", escaped_json(&fact.label)),
    ];
    if !fact.links.is_empty() {
        parts.push(format!("from: {}", escaped_json(&fact.links)));
    }
    if !fact.supersedes.is_empty() {
        parts.push(format!("supersedes: {}", escaped_json(&fact.supersedes)));
    }
    format!("{OPEN}{}{CLOSE}", parts.join("; "))
}

/// The fact whose bullet text is `text` and whose trailer is `line`.
pub fn parse_trailer(line: &str, text: FactText) -> Result<Fact, TrailerFault> {
    let inner = line
        .strip_prefix(OPEN)
        .and_then(|rest| rest.strip_suffix(CLOSE))
        .ok_or(TrailerFault::NotAComment)?;
    let mut fields = Fields::default();
    for part in inner.split(';') {
        let part = part.strip_prefix(' ').unwrap_or(part);
        let (key, value) = part.split_once(": ").ok_or(TrailerFault::NotAComment)?;
        fields.set(key, unescape(value))?;
    }
    fields.into_fact(text)
}

#[derive(Default)]
struct Fields {
    id: Option<String>,
    at: Option<String>,
    by: Option<String>,
    label: Option<String>,
    from: Option<String>,
    supersedes: Option<String>,
}

impl Fields {
    fn set(&mut self, key: &str, value: String) -> Result<(), TrailerFault> {
        let (slot, name) = match key {
            "fact" => (&mut self.id, "fact"),
            "at" => (&mut self.at, "at"),
            "by" => (&mut self.by, "by"),
            "label" => (&mut self.label, "label"),
            "from" => (&mut self.from, "from"),
            "supersedes" => (&mut self.supersedes, "supersedes"),
            "valid" | "origin" => return Err(TrailerFault::Reserved(key.to_owned())),
            other => return Err(TrailerFault::UnknownKey(other.to_owned())),
        };
        if slot.replace(value).is_some() {
            return Err(TrailerFault::Duplicate(name));
        }
        Ok(())
    }

    fn into_fact(self, text: FactText) -> Result<Fact, TrailerFault> {
        let need = |v: Option<String>, k| v.ok_or(TrailerFault::MissingKey(k));
        let id =
            FactId::parse(&need(self.id, "fact")?).map_err(|_| TrailerFault::BadValue("fact"))?;
        let at = need(self.at, "at")?;
        let parsed: Timestamp = at.parse().map_err(|_| TrailerFault::BadValue("at"))?;
        if parsed.to_string() != at {
            return Err(TrailerFault::BadValue("at"));
        }
        let by: Actor = from_json(&need(self.by, "by")?, "by")?;
        let label: Label = from_json(&need(self.label, "label")?, "label")?;
        let links: Vec<Link> = self
            .from
            .map_or(Ok(Vec::new()), |v| from_json(&v, "from"))?;
        let supersedes: Vec<FactId> = self
            .supersedes
            .map_or(Ok(Vec::new()), |v| from_json(&v, "supersedes"))?;
        Ok(Fact {
            id,
            text,
            recorded: UnixSeconds(parsed.as_second()),
            by,
            label,
            links,
            supersedes,
            valid: Validity::Unstated,
        })
    }
}

fn from_json<T: serde::de::DeserializeOwned>(
    text: &str,
    key: &'static str,
) -> Result<T, TrailerFault> {
    serde_json::from_str(text).map_err(|_| TrailerFault::BadValue(key))
}

fn escaped_json<T: serde::Serialize>(value: &T) -> String {
    // Plain derives with string keys: serialisation cannot fail.
    escape(&serde_json::to_string(value).unwrap_or_default())
}
