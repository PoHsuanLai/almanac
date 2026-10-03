//! The data class of a text, and its spelling as a recall tag.
//!
//! Consent and the on-device floor are per class, so a request over several labels takes the
//! most sensitive class present, and each indexed document carries the class of its own label.
//! recall names no porter type: its `ClassTag` is the data-class slug, written and read only
//! here.

use almanac_core::{DataClass, Label};
use recall::ClassTag;

/// The data class a request carries when its input mixes several: the most sensitive present, in
/// this order (the grant and the on-device floor are per class, so the request takes the
/// strictest of them). Nothing classed at all is the app's own data.
const BY_SENSITIVITY: [DataClass; 12] = [
    DataClass::Voice,
    DataClass::Prompt,
    DataClass::Mail,
    DataClass::Contacts,
    DataClass::Calendar,
    DataClass::Notes,
    DataClass::Files,
    DataClass::Photos,
    DataClass::Clipboard,
    DataClass::Screen,
    DataClass::AppOwn,
    DataClass::Public,
];

/// The class a request over `labels` is sent as.
pub fn class_of<'a>(labels: impl IntoIterator<Item = &'a Label>) -> DataClass {
    let present: Vec<&Label> = labels.into_iter().collect();
    BY_SENSITIVITY
        .into_iter()
        .find(|class| present.iter().any(|l| l.classes.contains(class)))
        .unwrap_or(DataClass::AppOwn)
}

/// The tag of `class`: its serde slug (`mail`, `app_own`).
pub fn class_tag(class: DataClass) -> ClassTag {
    let slug = serde_json::to_value(class)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    ClassTag(slug)
}

/// The class a tag spells; `None` for the empty tag and for one this build does not know (an
/// index written by a newer build), which an embedder treats as its own strictest pin.
pub fn class_from_tag(tag: &ClassTag) -> Option<DataClass> {
    serde_json::from_value(serde_json::Value::String(tag.0.clone())).ok()
}

/// The tag of the strictest class among `label`'s: what a document with this label carries.
pub(crate) fn tag_of(label: &Label) -> ClassTag {
    class_tag(class_of([label]))
}
