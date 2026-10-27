//! The inferd-backed embedder, over a scripted transport: what it asks for, and what it makes of
//! every answer.

use crate::support::{Scripted, card, embed_reply, served, texts, usage, vector_of};
use almanac_core::DataClass;
use almanac_infer::InferdEmbedder;
use almanac_store::{EmbedError, EmbedRole, Embedder, RetryClass, Urgency};
use porter_core::capability::Modality;
use porter_core::consent::Usage;
use porter_core::need::DimsNeed;
use porter_core::{Dims, Need, Tier};
use porter_infer::{
    ChatReply, ClientFrame, InferEvent, InferRefusal, InferReply, InferRequest, ModelError,
    StopReason,
};

#[tokio::test]
async fn the_embedder_asks_for_the_cards_model_and_returns_one_vector_per_text() {
    let want = vec![vector_of("alpha"), vector_of("beta")];
    let transport = Scripted::playing(vec![
        InferEvent::Routed(served()),
        InferEvent::Waiting(porter_infer::Readiness::Loading),
        InferEvent::Finished(embed_reply(want.clone())),
    ]);
    let embedder = InferdEmbedder::new(transport.clone(), card());
    let got = embedder
        .embed(&texts(2), EmbedRole::Document, Urgency::Background)
        .await
        .expect("vectors");
    assert_eq!(got.iter().map(|v| v.0.clone()).collect::<Vec<_>>(), want);

    let opened = transport.opened();
    assert_eq!(opened.len(), 1);
    let Need::Embeddings(need) = &opened[0].need else {
        panic!("{:?}", opened[0].need)
    };
    assert_eq!(need.dims, DimsNeed::Exactly(Dims(card().dims)));
    assert!(need.modalities.contains(&Modality::Text));
    assert_eq!(
        (opened[0].class, opened[0].tier),
        (DataClass::Mail, Tier::Fast)
    );
    let sent = transport.sent();
    let [ClientFrame::Request(InferRequest::Embed(request))] = sent.as_slice() else {
        panic!("{:?}", transport.sent())
    };
    assert_eq!(request.inputs, texts(2));
    assert_eq!(request.role, porter_infer::EmbedRole::Document);
    assert_eq!(request.usage, Usage::Background);
    assert_eq!(request.class, DataClass::Mail);
}

#[tokio::test]
async fn a_query_is_interactive_and_the_class_is_the_embedders() {
    let transport = Scripted::finishing(embed_reply(vec![vector_of("q")]));
    let embedder = InferdEmbedder::for_class(transport.clone(), card(), DataClass::Notes);
    embedder
        .embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
        .await
        .expect("vectors");
    let sent = transport.sent();
    let [ClientFrame::Request(InferRequest::Embed(request))] = sent.as_slice() else {
        panic!("one embed request")
    };
    assert_eq!(request.role, porter_infer::EmbedRole::Query);
    assert_eq!(request.usage, Usage::Interactive);
    assert_eq!(request.class, DataClass::Notes);
}

#[tokio::test]
async fn nothing_to_embed_opens_nothing() {
    let transport = Scripted::playing(vec![]);
    let embedder = InferdEmbedder::new(transport.clone(), card());
    let none = embedder
        .embed(&[], EmbedRole::Document, Urgency::Background)
        .await;
    assert_eq!(none, Ok(vec![]));
    assert!(transport.opened().is_empty());
}

fn failed(class: RetryClass) -> impl Fn(&EmbedError) -> bool {
    move |e| matches!(e, EmbedError::Failed { class: c, .. } if *c == class)
}

async fn embed_error(reply: InferReply, count: usize) -> EmbedError {
    let embedder = InferdEmbedder::new(Scripted::finishing(reply), card());
    embedder
        .embed(&texts(count), EmbedRole::Document, Urgency::Background)
        .await
        .expect_err("an error")
}

#[tokio::test]
async fn the_embedder_maps_every_refusal_and_failure() {
    let refusals = [
        (InferRefusal::Unavailable, EmbedError::Unavailable),
        (
            InferRefusal::RequiresCloud(DataClass::Mail),
            EmbedError::Refused(InferRefusal::RequiresCloud(DataClass::Mail).to_string()),
        ),
        (
            InferRefusal::NeedsGrant,
            EmbedError::Refused(InferRefusal::NeedsGrant.to_string()),
        ),
        (
            InferRefusal::Denied,
            EmbedError::Refused(InferRefusal::Denied.to_string()),
        ),
        (
            InferRefusal::OverBudget,
            EmbedError::Refused(InferRefusal::OverBudget.to_string()),
        ),
        (
            InferRefusal::Unsupported,
            EmbedError::Refused(InferRefusal::Unsupported.to_string()),
        ),
    ];
    for (refusal, expected) in refusals {
        assert_eq!(embed_error(InferReply::Refused(refusal), 1).await, expected);
    }
    let failures = [
        (ModelError::Unreachable, EmbedError::Unavailable),
        (ModelError::NotReady, EmbedError::Unavailable),
        (ModelError::RateLimited(5), EmbedError::Busy),
        (ModelError::ContextOverflow, EmbedError::TooLong),
        (
            ModelError::Unauthorized,
            EmbedError::Refused(ModelError::Unauthorized.to_string()),
        ),
        (
            ModelError::Refused,
            EmbedError::Refused(ModelError::Refused.to_string()),
        ),
    ];
    for (failure, expected) in failures {
        assert_eq!(embed_error(InferReply::Failed(failure), 1).await, expected);
    }
    for transient in [ModelError::Unreadable, ModelError::Unparseable] {
        let error = embed_error(InferReply::Failed(transient), 1).await;
        assert!(failed(RetryClass::Retry)(&error), "{error:?}");
    }
    let cancelled = embed_error(InferReply::Cancelled, 1).await;
    assert!(failed(RetryClass::Retry)(&cancelled));
    // Retry classes follow the error: refusals are fatal, unavailable and busy are not.
    assert_eq!(EmbedError::Unavailable.retry_class(), RetryClass::Retry);
    assert_eq!(EmbedError::Busy.retry_class(), RetryClass::Retry);
    assert_eq!(
        EmbedError::Refused("x".into()).retry_class(),
        RetryClass::Fatal
    );
}

#[tokio::test]
async fn a_reply_that_does_not_fit_the_index_is_fatal() {
    let wrong_count = embed_error(embed_reply(vec![vector_of("a")]), 2).await;
    assert!(failed(RetryClass::Fatal)(&wrong_count), "{wrong_count:?}");
    let wrong_dims = embed_error(embed_reply(vec![vec![0.5; 7]]), 1).await;
    assert!(failed(RetryClass::Fatal)(&wrong_dims), "{wrong_dims:?}");
    let wrong_kind = embed_error(
        InferReply::Chat(ChatReply::new(
            "hello".into(),
            StopReason::EndTurn,
            usage(),
            served(),
        )),
        1,
    )
    .await;
    assert!(failed(RetryClass::Fatal)(&wrong_kind));
}

#[tokio::test]
async fn an_absent_daemon_or_a_closed_session_is_unavailable() {
    let down = InferdEmbedder::new(Scripted::down(), card());
    assert_eq!(
        down.embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
            .await,
        Err(EmbedError::Unavailable)
    );
    // The session ends before the turn does.
    let closed = InferdEmbedder::new(Scripted::playing(vec![]), card());
    assert_eq!(
        closed
            .embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
            .await,
        Err(EmbedError::Unavailable)
    );
}

#[tokio::test]
async fn a_caller_inferd_refuses_is_a_fatal_failure_with_the_daemons_text() {
    let denied = InferdEmbedder::new(
        Scripted::denying("inferd: the caller is not in its caller table"),
        card(),
    );
    let got = denied
        .embed(&texts(1), EmbedRole::Query, Urgency::Interactive)
        .await;
    assert_eq!(
        got,
        Err(EmbedError::Failed {
            class: RetryClass::Fatal,
            why: "inferd: the caller is not in its caller table".to_owned()
        })
    );
}
