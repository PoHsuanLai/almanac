//! Payloads other areas own. almanac stores them as opaque JSON (their serde form) and never
//! names their types: that keeps the repo graph acyclic.

use crate::ids::KindTag;
use crate::slug::slug_enum;
use crate::text::JsonText;
use crate::thing::{ThingRole, ThingView};
use serde::{Deserialize, Serialize};

slug_enum!(
    /// Which area owns a payload.
    AreaTag {
        /// docket: policy decisions, consent, calls, undo, breaker (`AuditRecord`).
        Docket => "docket",
        /// cua: run steps (`CuaRecord`).
        Cua => "cua",
        /// The companion's session records (`SessionRecord`).
        Companion => "companion",
    }
);

/// One event body owned by another area.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AreaPayload {
    /// The owner.
    pub area: AreaTag,
    /// The event's kind tag (`policy.ruled`, `cua.step`): the owner's choice, whole.
    pub kind: KindTag,
    /// The owner's serde form, stored and returned unchanged.
    pub json: JsonText,
    /// The things the payload names, for cascade-forget.
    pub things: Vec<(ThingView, ThingRole)>,
}
