//! A scripted inferd: a transport whose one session plays given events and remembers what was
//! opened and sent, and the replies the tests feed it.

use almanac_core::DataClass;
use almanac_store::{Embedder, EmbedderCard};
use porter_capability::{Locality, ModelId, Tier};
use porter_client::{Transport, TransportError};
use porter_core::AccountId;
use porter_infer::{
    ChatReply, ClientFrame, EmbedReply, EmbedVector, InferEvent, InferReply, InferSession,
    OpenOptions, ServedBy, SessionError, StopReason, TokenUsage,
};
use porter_match::Need;
use porter_wire::{AccountsReply, AccountsRequest};
use recall::FakeEmbedder;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub need: Need,
    pub class: DataClass,
    pub tier: Tier,
}

/// A transport whose one session plays `events`, and which remembers what was opened and sent.
#[derive(Debug, Default)]
pub struct Scripted {
    events: Mutex<VecDeque<InferEvent>>,
    opened: Mutex<Vec<Opened>>,
    sent: Arc<Mutex<Vec<ClientFrame>>>,
    unreachable: bool,
    /// inferd refuses the caller, with this text.
    denied: Option<String>,
    /// The session never answers once its events run out.
    hang: bool,
}

impl Scripted {
    pub fn playing(events: Vec<InferEvent>) -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(events.into()),
            ..Self::default()
        })
    }

    pub fn down() -> Arc<Self> {
        Arc::new(Self {
            unreachable: true,
            ..Self::default()
        })
    }

    pub fn denying(why: &str) -> Arc<Self> {
        Arc::new(Self {
            denied: Some(why.to_owned()),
            ..Self::default()
        })
    }

    pub fn hanging() -> Arc<Self> {
        Arc::new(Self {
            hang: true,
            ..Self::default()
        })
    }

    pub fn finishing(reply: InferReply) -> Arc<Self> {
        Self::playing(vec![InferEvent::Finished(reply)])
    }

    pub fn opened(&self) -> Vec<Opened> {
        self.opened.lock().expect("lock").clone()
    }

    pub fn sent(&self) -> Vec<ClientFrame> {
        self.sent.lock().expect("lock").clone()
    }
}

#[derive(Debug)]
pub struct Session {
    events: VecDeque<InferEvent>,
    hang: bool,
    sent: Arc<Mutex<Vec<ClientFrame>>>,
}

impl InferSession for Session {
    async fn send(&mut self, frame: ClientFrame) -> Result<(), SessionError> {
        self.sent.lock().expect("lock").push(frame);
        Ok(())
    }

    async fn next(&mut self) -> Result<InferEvent, SessionError> {
        match (self.events.pop_front(), self.hang) {
            (Some(event), _) => Ok(event),
            (None, true) => std::future::pending().await,
            (None, false) => Err(SessionError::Closed),
        }
    }
}

impl Transport for Scripted {
    type Session = Session;

    async fn call(&self, _request: AccountsRequest) -> Result<AccountsReply, TransportError> {
        Err(TransportError::Unreachable)
    }

    async fn open_with(
        &self,
        need: &Need,
        class: DataClass,
        tier: Tier,
        _options: &OpenOptions,
    ) -> Result<Session, TransportError> {
        if self.unreachable {
            return Err(TransportError::Unreachable);
        }
        if let Some(why) = &self.denied {
            return Err(TransportError::Denied(why.clone()));
        }
        self.opened.lock().expect("lock").push(Opened {
            need: need.clone(),
            class,
            tier,
        });
        Ok(Session {
            events: std::mem::take(&mut *self.events.lock().expect("lock")),
            hang: self.hang,
            sent: self.sent.clone(),
        })
    }
}

pub fn served() -> ServedBy {
    ServedBy {
        account: AccountId::parse("local").expect("id"),
        model: ModelId::parse("nomic").expect("id"),
        locality: Locality::OnDevice,
    }
}

pub fn usage() -> TokenUsage {
    TokenUsage {
        input: porter_ids::Tokens(3),
        output: porter_ids::Tokens(0),
        cached: porter_ids::Tokens(0),
        charge: None,
    }
}

pub fn embed_reply(vectors: Vec<Vec<f32>>) -> InferReply {
    InferReply::Embed(EmbedReply::new(
        vectors.into_iter().map(EmbedVector).collect(),
        usage(),
        served(),
    ))
}

pub fn card() -> EmbedderCard {
    FakeEmbedder::new().card().clone()
}

pub fn texts(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("text {i}")).collect()
}

pub fn vector_of(text: &str) -> Vec<f32> {
    FakeEmbedder::vector(text).0
}

pub fn chat(text: &str) -> InferReply {
    InferReply::Chat(ChatReply::new(
        text.to_owned(),
        StopReason::EndTurn,
        usage(),
        served(),
    ))
}
