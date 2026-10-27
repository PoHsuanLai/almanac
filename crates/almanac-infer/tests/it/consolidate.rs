//! The inferd-backed consolidator, over a scripted transport: what it asks for, and what it makes
//! of every answer.

use crate::support::{Scripted, chat};
use almanac_core::DataClass;
use almanac_fake::{consolidation_event_link, consolidation_input};
use almanac_infer::InferdConsolidator;
use almanac_tidy::{ConsolidateError, Consolidator, render_prompt};
use porter_core::capability::LlmFeature;
use porter_core::consent::Usage;
use porter_core::{Need, Tier};
use porter_infer::{ClientFrame, InferRefusal, InferReply, InferRequest, ModelError, Task};

#[tokio::test]
async fn the_consolidator_sends_the_prompt_as_a_background_extract_task() {
    let answer = format!(
        r#"{{"hunks":[{{"kind":"promote","text":"Ana archived it.","from":[{}]}}]}}"#,
        consolidation_event_link(1)
    );
    let transport = Scripted::finishing(chat(&answer));
    let consolidator = InferdConsolidator::new(transport.clone());
    let draft = consolidator
        .draft(consolidation_input())
        .await
        .expect("draft");
    assert_eq!(draft.hunks.len(), 1);

    let opened = transport.opened();
    let Need::Llm(need) = &opened[0].need else {
        panic!("{:?}", opened[0].need)
    };
    assert!(need.features.contains(&LlmFeature::Chat));
    assert_eq!(
        (opened[0].class, opened[0].tier),
        (DataClass::Mail, Tier::Balanced),
        "the stranger's mail label carries its class"
    );
    let sent = transport.sent();
    let [ClientFrame::Request(InferRequest::Task(task))] = sent.as_slice() else {
        panic!("one task request")
    };
    assert_eq!(task.task, Task::Extract);
    assert_eq!(task.usage, Usage::Background);
    assert_eq!(task.class, DataClass::Mail);
    assert_eq!(task.input, render_prompt(&consolidation_input()));
}

async fn consolidation_error(reply: InferReply) -> ConsolidateError {
    InferdConsolidator::new(Scripted::finishing(reply))
        .draft(consolidation_input())
        .await
        .expect_err("an error")
}

#[tokio::test]
async fn the_consolidator_maps_every_refusal_and_failure() {
    use ConsolidateError::{Busy, Unavailable, Unparseable};
    for (refusal, expected) in [
        (InferRefusal::Unavailable, Unavailable),
        (InferRefusal::RequiresCloud(DataClass::Mail), Unavailable),
        (InferRefusal::NeedsGrant, Unavailable),
        (InferRefusal::Denied, Unavailable),
        (InferRefusal::Unsupported, Unavailable),
        (InferRefusal::OverBudget, Busy),
    ] {
        assert_eq!(
            consolidation_error(InferReply::Refused(refusal)).await,
            expected
        );
    }
    for (failure, expected) in [
        (ModelError::RateLimited(3), Busy),
        (ModelError::Unreadable, Unparseable),
        (ModelError::Unparseable, Unparseable),
        (ModelError::ContextOverflow, Unparseable),
        (ModelError::Unreachable, Unavailable),
        (ModelError::Unauthorized, Unavailable),
        (ModelError::Refused, Unavailable),
        (ModelError::NotReady, Unavailable),
    ] {
        assert_eq!(
            consolidation_error(InferReply::Failed(failure)).await,
            expected
        );
    }
    assert_eq!(consolidation_error(InferReply::Cancelled).await, Busy);
    assert_eq!(
        consolidation_error(chat("no json here")).await,
        Unparseable,
        "an answer that is not the object"
    );
    let down = InferdConsolidator::new(Scripted::down());
    assert_eq!(down.draft(consolidation_input()).await, Err(Unavailable));
}
