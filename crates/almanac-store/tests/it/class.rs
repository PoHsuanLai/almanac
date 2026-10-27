//! The data class of a request and its spelling as an index tag.

use almanac_core::*;
use almanac_store::{ClassTag, class_from_tag, class_of, class_tag};
use std::collections::BTreeSet;

fn trusted_label() -> Label {
    Label {
        integrity: Integrity::Trusted,
        confidentiality: Confidentiality::Public,
        classes: BTreeSet::new(),
        sources: BTreeSet::from([Source::User]),
    }
}

#[test]
fn the_class_of_a_request_is_the_most_sensitive_of_its_labels() {
    let with = |classes: &[DataClass]| {
        let mut label = trusted_label();
        label.classes = classes.iter().copied().collect();
        label
    };
    let table: [(Vec<Label>, DataClass); 7] = [
        (vec![], DataClass::AppOwn),
        (vec![with(&[])], DataClass::AppOwn),
        (vec![with(&[DataClass::Public])], DataClass::Public),
        (
            vec![with(&[DataClass::Public]), with(&[DataClass::Mail])],
            DataClass::Mail,
        ),
        (
            vec![with(&[DataClass::Notes, DataClass::Files])],
            DataClass::Notes,
        ),
        (
            vec![with(&[DataClass::Mail]), with(&[DataClass::Voice])],
            DataClass::Voice,
        ),
        (
            vec![with(&[DataClass::Mail, DataClass::Prompt])],
            DataClass::Prompt,
        ),
    ];
    for (labels, expected) in table {
        assert_eq!(class_of(&labels), expected, "{labels:?}");
    }
}

#[test]
fn a_class_tag_is_the_slug_and_every_class_round_trips() {
    // The exhaustive match makes a new class a compile error here, so it gets a row.
    let slug = |class: DataClass| match class {
        DataClass::AppOwn => "app_own",
        DataClass::Mail => "mail",
        DataClass::Calendar => "calendar",
        DataClass::Tasks => "tasks",
        DataClass::Contacts => "contacts",
        DataClass::Notes => "notes",
        DataClass::Files => "files",
        DataClass::Photos => "photos",
        DataClass::Clipboard => "clipboard",
        DataClass::Screen => "screen",
        DataClass::Voice => "voice",
        DataClass::Prompt => "prompt",
        DataClass::Public => "public",
    };
    let all = [
        DataClass::AppOwn,
        DataClass::Mail,
        DataClass::Calendar,
        DataClass::Tasks,
        DataClass::Contacts,
        DataClass::Notes,
        DataClass::Files,
        DataClass::Photos,
        DataClass::Clipboard,
        DataClass::Screen,
        DataClass::Voice,
        DataClass::Prompt,
        DataClass::Public,
    ];
    for class in all {
        let tag = class_tag(class);
        assert_eq!(tag.0, slug(class));
        assert_eq!(class_from_tag(&tag), Some(class));
    }
    assert_eq!(class_from_tag(&ClassTag::default()), None);
    assert_eq!(class_from_tag(&ClassTag("later".into())), None);
}
