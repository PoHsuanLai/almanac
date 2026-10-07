//! `InferdConsolidator`: a consolidation draft from an inferd task session.

use super::prompt::{parse_draft, render_prompt};
use super::turn;
use almanac_service::class_of;
use almanac_service::{ConsolidateError, ConsolidationInput, Consolidator, Draft};
use porter_client::Transport;
use porter_core::capability::LlmFeature;
use porter_core::consent::Usage;
use porter_core::need::LlmNeed;
use porter_core::{Need, Tier, Tokens};
use porter_infer::{InferRefusal, InferReply, InferRequest, ModelError, Task, TaskRequest};
use std::collections::BTreeSet;
use std::sync::Arc;

/// The context window the prompt may need: the active facts and the events of a day.
const CONTEXT: Tokens = Tokens(16_384);

/// Drafts a consolidation through an inferd session (`Need::Llm`, `Task::Extract`,
/// `Usage::Background`): the input is rendered as the prompt (`render_prompt`), the answer parsed
/// into hunks (`parse_draft`, `Unparseable` on anything else), and the request carries the most
/// sensitive data class among the labels of what it reads (`class_of`), so mail-derived facts
/// stay on the device unless the person's grant says otherwise.
#[derive(Debug)]
pub struct InferdConsolidator<T: Transport> {
    transport: Arc<T>,
}

impl<T: Transport> InferdConsolidator<T> {
    /// Drafts over `transport`.
    pub fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }
}

fn need() -> Need {
    Need::Llm(LlmNeed {
        features: BTreeSet::from([LlmFeature::Chat]),
        context: CONTEXT,
    })
}

fn refusal(why: InferRefusal) -> ConsolidateError {
    match why {
        InferRefusal::Unavailable
        | InferRefusal::RequiresCloud(_)
        | InferRefusal::NeedsGrant
        | InferRefusal::Denied
        | InferRefusal::Unsupported => ConsolidateError::Unavailable,
        InferRefusal::OverBudget => ConsolidateError::Busy,
    }
}

fn failure(why: ModelError) -> ConsolidateError {
    match why {
        ModelError::RateLimited(_) => ConsolidateError::Busy,
        // A reply that was all thinking left no draft to read.
        ModelError::Unreadable
        | ModelError::Unparseable
        | ModelError::ContextOverflow
        | ModelError::OnlyThought { .. } => ConsolidateError::Unparseable,
        ModelError::Unreachable
        | ModelError::Unauthorized
        | ModelError::Refused
        | ModelError::NotReady => ConsolidateError::Unavailable,
    }
}

impl<T: Transport> Consolidator for InferdConsolidator<T> {
    async fn draft(&self, input: ConsolidationInput) -> Result<Draft, ConsolidateError> {
        let labels = input
            .facts
            .iter()
            .map(|f| &f.label)
            .chain(input.events.iter().map(|e| &e.label));
        let class = class_of(labels);
        let mut session = self
            .transport
            .open(&need(), class, Tier::Balanced)
            .await
            .map_err(|_| ConsolidateError::Unavailable)?;
        let request = InferRequest::Task(TaskRequest {
            task: Task::Extract,
            input: render_prompt(&input),
            class,
            usage: Usage::Background,
        });
        match turn(&mut session, request)
            .await
            .map_err(|_| ConsolidateError::Unavailable)?
        {
            InferReply::Chat(reply) => parse_draft(&reply.text, &input),
            InferReply::Refused(why) => Err(refusal(why)),
            InferReply::Failed(why) => Err(failure(why)),
            InferReply::Cancelled => Err(ConsolidateError::Busy),
            InferReply::Embed(_)
            | InferReply::CuaStep(_)
            | InferReply::Transcribed(_)
            | InferReply::Spoke(_) => Err(ConsolidateError::Unparseable),
        }
    }
}
