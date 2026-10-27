//! The label check on a `Record`: its label covers its documents' labels, a message is well
//! formed, an episode's skeleton is trusted.

use almanac_core::{EventBody, Integrity, Label, MessageFault, SenderCheck};

/// Whether `outer` is at least as restrictive as `inner`: no more trusted, no less
/// confidential, every class and source of `inner` carried. (`outer` joined with `inner` is
/// `outer`, as the join normalises.)
fn label_covers(outer: &Label, inner: &Label) -> bool {
    outer.join(inner) == outer.join(outer)
}

/// The join of the labels of every document of `body`, if it has any.
fn documents_label(body: &EventBody) -> Option<Label> {
    body.index_texts()
        .into_iter()
        .map(|t| t.label)
        .reduce(|a, b| a.join(&b))
}

/// Why a `Record` is malformed, or `None` if it is well formed: its label covers the join of its
/// documents' labels, a message passes `Message::check` and was sent by its actor, and an
/// episode's skeleton is trusted and its Space is the record's.
pub(crate) fn record_fault(record: &almanac_core::Record) -> Option<String> {
    let body_fault = match &record.body {
        EventBody::Message(m) => m
            .check()
            .err()
            .map(|f: MessageFault| format!("message: {f:?}"))
            .or_else(|| {
                (m.sender_matches(&record.actor) == SenderCheck::Mismatch)
                    .then(|| "message: sender is not the recording actor".to_owned())
            }),
        EventBody::Episode(e) => (e.skeleton.label.integrity != Integrity::Trusted)
            .then(|| "episode: the skeleton must be trusted".to_owned())
            .or_else(|| (e.space != record.space).then(|| "episode: wrong Space".to_owned())),
        EventBody::Thing { .. }
        | EventBody::File { .. }
        | EventBody::Search { .. }
        | EventBody::Memory { .. }
        | EventBody::Area(_) => None,
        // A body kind this build cannot check is refused, not admitted unchecked.
        _ => Some("unknown event body kind".to_owned()),
    };
    body_fault.or_else(|| {
        documents_label(&record.body)
            .filter(|joined| !label_covers(&record.label, joined))
            .map(|_| "label is less restrictive than the documents it carries".to_owned())
    })
}
