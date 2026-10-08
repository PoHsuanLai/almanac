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
const BY_SENSITIVITY: [DataClass; 13] = [
    DataClass::Voice,
    DataClass::Prompt,
    DataClass::Mail,
    DataClass::Contacts,
    DataClass::Calendar,
    DataClass::Tasks,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each class's place in [`BY_SENSITIVITY`]. The match is exhaustive, so a class porter adds
    /// is a compile error here until it is given a place, and the test below fails until the
    /// list holds it there: a new class is never silently classed as the app's own.
    fn rank(class: DataClass) -> usize {
        match class {
            DataClass::Voice => 0,
            DataClass::Prompt => 1,
            DataClass::Mail => 2,
            DataClass::Contacts => 3,
            DataClass::Calendar => 4,
            DataClass::Tasks => 5,
            DataClass::Notes => 6,
            DataClass::Files => 7,
            DataClass::Photos => 8,
            DataClass::Clipboard => 9,
            DataClass::Screen => 10,
            DataClass::AppOwn => 11,
            DataClass::Public => 12,
        }
    }

    #[test]
    fn every_class_has_its_place_in_the_sensitivity_order() {
        for (place, class) in BY_SENSITIVITY.into_iter().enumerate() {
            assert_eq!(rank(class), place, "{class:?}");
        }
    }
}
