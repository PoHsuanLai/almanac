//! What the settings file loosens below the shipped defaults (ALM-16).
//!
//! The file is writable by any process of the same user, and nothing authenticates it, so a value
//! that keeps less, keeps it for a shorter time or holds files less safely than the shipped
//! default is reported by name on every read (at start and on each reload). This is the audit
//! trail only: refusing such a value until the person confirmed it in the app needs a consent
//! store the agent sandbox cannot write, which is an owner decision (see FINDINGS).

use super::{ConsolidateApply, MemorySettings};
use almanac_core::{DayCount, VaultKind};

/// One day-count key and how to read it from a [`MemorySettings`].
struct Days {
    key: &'static str,
    of: fn(&MemorySettings) -> DayCount,
}

const DAY_KEYS: [Days; 6] = [
    Days {
        key: "memory.retention.search_days",
        of: |s| s.retention.search,
    },
    Days {
        key: "memory.retention.file_unexplained_days",
        of: |s| s.retention.file_unexplained,
    },
    Days {
        key: "memory.retention.session_days",
        of: |s| s.retention.session,
    },
    Days {
        key: "memory.retention.audit_body_days",
        of: |s| s.retention.audit_body,
    },
    Days {
        key: "memory.retention.audit_header_days",
        of: |s| s.retention.audit_header,
    },
    Days {
        key: "memory.pending_ttl_days",
        of: |s| s.pending_ttl,
    },
];

/// The keys of `value` that are looser than the shipped defaults, each with a short description.
pub fn loosenings(value: &MemorySettings) -> Vec<String> {
    let shipped = MemorySettings::default();
    let mut out: Vec<String> = DAY_KEYS
        .iter()
        .filter(|d| (d.of)(value) < (d.of)(&shipped))
        .map(|d| {
            format!(
                "{}: {} days, shorter than the shipped {}",
                d.key,
                (d.of)(value).0,
                (d.of)(&shipped).0
            )
        })
        .collect();
    if value.at_rest != shipped.at_rest && value.at_rest == VaultKind::Plain {
        out.push("memory.files.at_rest: plain, new Spaces keep readable files".to_owned());
    }
    if value.apply != shipped.apply && value.apply == ConsolidateApply::Auto {
        out.push("memory.consolidation.apply: auto, runs apply without review".to_owned());
    }
    out
}
