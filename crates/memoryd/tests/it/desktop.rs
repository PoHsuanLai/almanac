//! The `desktop` Space: memory outside any Space. It is never unknown (a first record
//! provisions it like any Space) and it cannot be deleted.

use crate::support::{SharedKeys, dirs_in, service};
use almanac_core::*;
use almanac_fake::mail_thread_archived;

fn desktop_record() -> Record {
    Record {
        space: SpaceId::desktop(),
        ..mail_thread_archived().expect("fixture")
    }
}

#[tokio::test]
async fn a_record_for_desktop_needs_no_registration() {
    let scratch = tempfile::tempdir().expect("scratch");
    let svc = service(&dirs_in(scratch.path()), &SharedKeys::default());
    let reply = svc
        .handle(&Caller::Router, MemoryRequest::Record(desktop_record()))
        .await;
    let MemoryReply::Recorded(event) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(event.space, SpaceId::desktop());
    let MemoryReply::Spaces(spaces) = svc.handle(&Caller::ShellUi, MemoryRequest::Spaces).await
    else {
        panic!("spaces")
    };
    assert!(spaces.iter().any(|s| s.id == SpaceId::desktop()));
}

#[tokio::test]
async fn desktop_cannot_be_deleted() {
    let scratch = tempfile::tempdir().expect("scratch");
    let svc = service(&dirs_in(scratch.path()), &SharedKeys::default());
    svc.handle(&Caller::Router, MemoryRequest::Record(desktop_record()))
        .await;
    let plan = svc
        .handle(
            &Caller::ShellUi,
            MemoryRequest::PlanForget(SpaceId::desktop(), ForgetScope::Space),
        )
        .await;
    assert!(
        matches!(plan, MemoryReply::Refused(Refusal::Invalid(ref why)) if why.contains("desktop")),
        "{plan:?}"
    );
    // The Space is still there and still takes records.
    let again = svc
        .handle(&Caller::Router, MemoryRequest::Record(desktop_record()))
        .await;
    assert!(matches!(again, MemoryReply::Recorded(_)), "{again:?}");
}
