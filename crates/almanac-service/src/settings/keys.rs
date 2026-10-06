//! The key table: every `memory.*` key of the schema (`dist/settings/almanac.settings.toml`), its
//! rule and where its value goes. The tests hold the schema and this table to each other.

use super::{ConsolidateWhen, MemorySettings};
use almanac_core::{DayCount, VaultKind};
use std::ops::RangeInclusive;

/// What a key's value may be and how it lands in the settings.
#[derive(Clone)]
pub(crate) enum Rule {
    /// A whole number inside `range`, in the key's unit.
    Number {
        range: RangeInclusive<i64>,
        set: fn(&mut MemorySettings, i64),
    },
    /// One of `words`, by position.
    Word {
        words: &'static [&'static str],
        set: fn(&mut MemorySettings, usize),
    },
}

/// One key of the table.
#[derive(Clone)]
pub(crate) struct Key {
    pub path: &'static str,
    pub rule: Rule,
}

fn days(v: i64) -> DayCount {
    DayCount(u32::try_from(v).unwrap_or(u32::MAX))
}

fn num(path: &'static str, range: RangeInclusive<i64>, set: fn(&mut MemorySettings, i64)) -> Key {
    Key {
        path,
        rule: Rule::Number { range, set },
    }
}

const AT_REST: [VaultKind; 2] = [VaultKind::Sealed, VaultKind::Plain];
const WHEN: [ConsolidateWhen; 3] = [
    ConsolidateWhen::Nightly,
    ConsolidateWhen::Manual,
    ConsolidateWhen::Never,
];

/// Every key, in the order the schema lists them.
pub(crate) fn table() -> Vec<Key> {
    vec![
        Key {
            path: "memory.files.at_rest",
            rule: Rule::Word {
                words: &["sealed", "plain"],
                set: |s, i| s.at_rest = AT_REST[i],
            },
        },
        Key {
            path: "memory.consolidation.when",
            rule: Rule::Word {
                words: &["nightly", "manual", "never"],
                set: |s, i| s.consolidate = WHEN[i],
            },
        },
        num("memory.retention.search_days", 1..=3650, |s, v| {
            s.retention.search = days(v)
        }),
        num(
            "memory.retention.file_unexplained_days",
            1..=3650,
            |s, v| {
                s.retention.file_unexplained = days(v);
            },
        ),
        num("memory.retention.session_days", 1..=3650, |s, v| {
            s.retention.session = days(v)
        }),
        num("memory.retention.audit_body_days", 1..=3650, |s, v| {
            s.retention.audit_body = days(v);
        }),
        num("memory.retention.audit_header_days", 1..=3650, |s, v| {
            s.retention.audit_header = days(v);
        }),
        num("memory.pending_ttl_days", 1..=365, |s, v| {
            s.pending_ttl = days(v)
        }),
    ]
}
