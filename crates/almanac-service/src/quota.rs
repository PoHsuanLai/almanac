//! What one Space may hold, and how large one record's fields may be: typed limits, checked
//! before an event is appended. A caller that fills a Space gets `SpaceFull`; a record with a
//! field over its limit is `Invalid`.

use almanac_core::{EventBody, Refusal};
use almanac_store::{BodyState, Entry};

/// The most events one Space holds (bodies erased by retention still count as events).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventCap(pub u64);

/// A size limit in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteCap(pub u64);

/// How large one record may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldLimits {
    /// The record's body, as stored.
    pub body: ByteCap,
    /// Any one text inside the body.
    pub text: ByteCap,
}

/// The limits on a Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    /// Events per Space.
    pub events: EventCap,
    /// Bytes of event bodies per Space.
    pub bytes: ByteCap,
    /// Sizes inside one record.
    pub fields: FieldLimits,
}

impl Default for Quota {
    fn default() -> Self {
        Self {
            events: EventCap(5_000_000),
            bytes: ByteCap(4 * 1024 * 1024 * 1024),
            fields: FieldLimits {
                body: ByteCap(1024 * 1024),
                text: ByteCap(256 * 1024),
            },
        }
    }
}

/// What a Space holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Usage {
    pub events: u64,
    pub bytes: u64,
}

/// The size of a body as stored, and the size of its longest text.
fn measure(body: &EventBody) -> (u64, u64) {
    let Ok(value) = serde_json::to_value(body) else {
        return (0, 0);
    };
    let whole = serde_json::to_vec(&value).map_or(0, |v| v.len());
    (to_u64(whole), longest_text(&value))
}

fn to_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn longest_text(value: &serde_json::Value) -> u64 {
    use serde_json::Value;
    match value {
        Value::String(s) => to_u64(s.len()),
        Value::Array(items) => items.iter().map(longest_text).max().unwrap_or(0),
        Value::Object(map) => map.values().map(longest_text).max().unwrap_or(0),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

impl FieldLimits {
    /// What `body` costs the Space, or why it is too large.
    pub(crate) fn check(&self, body: &EventBody) -> Result<u64, Refusal> {
        let (whole, text) = measure(body);
        if whole > self.body.0 {
            return Err(Refusal::Invalid("the record is too large".to_owned()));
        }
        if text > self.text.0 {
            return Err(Refusal::Invalid(
                "a field of the record is too long".to_owned(),
            ));
        }
        Ok(whole)
    }
}

impl Usage {
    /// What the retained `entries` hold.
    pub(crate) fn of(entries: &[Entry]) -> Self {
        let bytes = entries
            .iter()
            .map(|e| match &e.body {
                BodyState::Present(body) => measure(body).0,
                BodyState::Erased => 0,
            })
            .fold(0u64, u64::saturating_add);
        Self {
            events: to_u64(entries.len()),
            bytes,
        }
    }

    /// Whether one more event costing `cost` bytes fits.
    pub(crate) fn fits(&self, quota: &Quota, cost: u64) -> bool {
        self.events < quota.events.0 && self.bytes.saturating_add(cost) <= quota.bytes.0
    }
}
