//! `Memory`: the app-facing API over a [`Transport`].

use crate::transport::{Transport, TransportError};
use almanac_core::{
    EventRef, FactDraft, FactId, FactQuery, FactState, FactView, FileWhyClaim, ForgetPlanView,
    ForgetReport, ForgetScope, InjectQuery, MarkRequest, MemoryReply, MemoryRequest, PlanToken,
    RecallHit, RecallQuery, RecentEntry, RecentQuery, Record, Refusal, Settlement, SpaceId,
    SpaceStatus, SpaceSummary, TimelinePage, TimelineQuery,
};

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// The request did not get an answer.
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    /// memoryd said no.
    #[error("refused: {0:?}")]
    Refused(Refusal),
    /// memoryd answered with a reply of another kind (a protocol bug).
    #[error("unexpected reply")]
    Unexpected,
}

/// What recording an event came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// Stored.
    Stored(EventRef),
    /// There is no memory on this desktop; nothing was kept. Writers carry on.
    NoMemory,
}

/// The app-facing memory API. Writers (`record`, `explain_file`, `mark`) are no-ops over an
/// absent transport; readers fail with `Transport(Absent)`.
#[derive(Debug, Clone)]
pub struct Memory<T: Transport> {
    transport: T,
}

impl<T: Transport> Memory<T> {
    /// Memory over `transport`.
    pub fn over(transport: T) -> Self {
        Self { transport }
    }

    /// Sends a request; a [`MemoryReply::Refused`] becomes [`ClientError::Refused`].
    pub async fn ask(&self, request: MemoryRequest) -> Result<MemoryReply, ClientError> {
        match self.transport.call(request).await? {
            MemoryReply::Refused(refusal) => Err(ClientError::Refused(refusal)),
            reply => Ok(reply),
        }
    }

    async fn write(&self, request: MemoryRequest) -> Result<Recorded, ClientError> {
        match self.ask(request).await {
            Ok(MemoryReply::Recorded(event)) | Ok(MemoryReply::RecordedBatch(event, _)) => {
                Ok(Recorded::Stored(event))
            }
            Ok(MemoryReply::Ok) | Err(ClientError::Transport(TransportError::Absent)) => {
                Ok(Recorded::NoMemory)
            }
            Ok(_) => Err(ClientError::Unexpected),
            Err(other) => Err(other),
        }
    }

    /// Records one event.
    pub async fn record(&self, record: Record) -> Result<Recorded, ClientError> {
        self.write(MemoryRequest::Record(record)).await
    }

    /// Records several events in order.
    pub async fn record_batch(&self, records: Vec<Record>) -> Result<Recorded, ClientError> {
        self.write(MemoryRequest::RecordBatch(records)).await
    }

    /// Says why a file changed.
    pub async fn explain_file(&self, claim: FileWhyClaim) -> Result<Recorded, ClientError> {
        self.write(MemoryRequest::ExplainFile(claim)).await
    }

    /// Marks or unmarks a thing.
    pub async fn mark(&self, mark: MarkRequest) -> Result<Recorded, ClientError> {
        self.write(MemoryRequest::Mark(mark)).await
    }

    /// Searches a Space.
    pub async fn search(&self, query: RecallQuery) -> Result<Vec<RecallHit>, ClientError> {
        match self.ask(MemoryRequest::Search(query)).await? {
            MemoryReply::Hits(hits) => Ok(hits),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Automatic recall for one turn: ranked hits whose text costs at most the query's budget.
    pub async fn inject(&self, query: InjectQuery) -> Result<Vec<RecallHit>, ClientError> {
        match self.ask(MemoryRequest::Inject(query)).await? {
            MemoryReply::Hits(hits) => Ok(hits),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Recent activity in a Space, newest first, with labels.
    pub async fn recent(
        &self,
        space: SpaceId,
        query: RecentQuery,
    ) -> Result<Vec<RecentEntry>, ClientError> {
        match self.ask(MemoryRequest::Recent(space, query)).await? {
            MemoryReply::Recent(entries) => Ok(entries),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Lists facts.
    pub async fn facts(&self, query: FactQuery) -> Result<Vec<FactView>, ClientError> {
        match self.ask(MemoryRequest::Facts(query)).await? {
            MemoryReply::Facts(facts) => Ok(facts),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Proposes a fact; it lands active or pending by its label.
    pub async fn propose(
        &self,
        space: SpaceId,
        draft: FactDraft,
    ) -> Result<(FactId, FactState), ClientError> {
        match self.ask(MemoryRequest::Propose(space, draft)).await? {
            MemoryReply::Proposed(id, state) => Ok((id, state)),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// The Spaces.
    pub async fn spaces(&self) -> Result<Vec<SpaceSummary>, ClientError> {
        match self.ask(MemoryRequest::Spaces).await? {
            MemoryReply::Spaces(spaces) => Ok(spaces),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// One Space's status.
    pub async fn status(&self, space: SpaceId) -> Result<SpaceStatus, ClientError> {
        match self.ask(MemoryRequest::Status(space)).await? {
            MemoryReply::Status(status) => Ok(status),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// A page of the timeline.
    pub async fn timeline(
        &self,
        space: SpaceId,
        query: TimelineQuery,
    ) -> Result<TimelinePage, ClientError> {
        match self.ask(MemoryRequest::Timeline(space, query)).await? {
            MemoryReply::Timeline(page) => Ok(page),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// What a forget would remove.
    pub async fn plan_forget(
        &self,
        space: SpaceId,
        scope: ForgetScope,
    ) -> Result<ForgetPlanView, ClientError> {
        match self.ask(MemoryRequest::PlanForget(space, scope)).await? {
            MemoryReply::Plan(plan) => Ok(plan),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Applies a plan.
    pub async fn forget(&self, token: PlanToken) -> Result<ForgetReport, ClientError> {
        match self.ask(MemoryRequest::Forget(token)).await? {
            MemoryReply::Forgot(report) => Ok(report),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Pending facts.
    pub async fn pending(&self, space: SpaceId) -> Result<Vec<FactView>, ClientError> {
        match self.ask(MemoryRequest::Pending(space)).await? {
            MemoryReply::Pending(facts) => Ok(facts),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Keeps or discards a pending fact.
    pub async fn settle(&self, fact: FactId, settlement: Settlement) -> Result<(), ClientError> {
        match self.ask(MemoryRequest::Settle(fact, settlement)).await? {
            MemoryReply::Ok => Ok(()),
            _ => Err(ClientError::Unexpected),
        }
    }
}
