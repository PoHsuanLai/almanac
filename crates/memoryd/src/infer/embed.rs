//! `InferdEmbedder`: embeddings through an inferd session.

use super::turn;
use porter_client::{Transport, TransportError};
use porter_core::capability::Modality;
use porter_core::consent::Usage;
use porter_core::need::{DimsNeed, EmbedNeed};
use porter_core::{DataClass, Dims, Need, Tier};
use porter_infer::{
    EmbedReply, EmbedRequest, EmbedRole as WireRole, InferRefusal, InferReply, InferRequest,
    ModelError,
};
use recall::{EmbedError, EmbedRole, Embedder, EmbedderCard, RetryClass, Urgency, Vector};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Embeds through an inferd session: `Need::Embeddings` for the card's vector length,
/// `InferRequest::Embed` with `Usage::Background` for indexing, and the document class this
/// embedder was built for (the on-device floor of that class applies, so mail text stays
/// local). inferd puts the model's query and document prefixes in front itself (the card
/// carries them only so the index can tell when they changed), and the vectors must have the
/// card's length: a model that answers otherwise is another vector space.
#[derive(Debug)]
pub struct InferdEmbedder<T: Transport> {
    transport: Arc<T>,
    card: EmbedderCard,
    class: DataClass,
}

impl<T: Transport> InferdEmbedder<T> {
    /// Embeds over `transport` with `card`, treating every text as mail (the strictest class
    /// memory holds in quantity; see [`InferdEmbedder::for_class`]).
    pub fn new(transport: Arc<T>, card: EmbedderCard) -> Self {
        Self::for_class(transport, card, DataClass::Mail)
    }

    /// Embeds texts of `class`.
    pub fn for_class(transport: Arc<T>, card: EmbedderCard, class: DataClass) -> Self {
        Self {
            transport,
            card,
            class,
        }
    }
}

/// The need that finds an embedding model for `card`.
pub(crate) fn need_for(card: &EmbedderCard) -> Need {
    Need::Embeddings(EmbedNeed {
        dims: DimsNeed::Exactly(Dims(card.dims)),
        modalities: BTreeSet::from([Modality::Text]),
    })
}

/// The wire request for `texts`.
pub(crate) fn request_for(
    card: &EmbedderCard,
    class: DataClass,
    texts: &[String],
    role: EmbedRole,
    urgency: Urgency,
) -> InferRequest {
    InferRequest::Embed(EmbedRequest {
        inputs: texts.to_vec(),
        role: match role {
            EmbedRole::Query => WireRole::Query,
            EmbedRole::Document => WireRole::Document,
        },
        dims: DimsNeed::Exactly(Dims(card.dims)),
        class,
        usage: match urgency {
            Urgency::Interactive => Usage::Interactive,
            Urgency::Background => Usage::Background,
        },
    })
}

fn fatal(why: impl Into<String>) -> EmbedError {
    EmbedError::Failed {
        class: RetryClass::Fatal,
        why: why.into(),
    }
}

/// Why inferd would not run the request.
pub(crate) fn refusal(why: InferRefusal) -> EmbedError {
    match why {
        InferRefusal::Unavailable => EmbedError::Unavailable,
        InferRefusal::RequiresCloud(_)
        | InferRefusal::NeedsGrant
        | InferRefusal::Denied
        | InferRefusal::OverBudget
        | InferRefusal::Unsupported => EmbedError::Refused(why.to_string()),
    }
}

/// Why the model call failed.
pub(crate) fn model_failure(why: ModelError) -> EmbedError {
    match why {
        ModelError::Unreachable | ModelError::NotReady => EmbedError::Unavailable,
        ModelError::RateLimited(_) => EmbedError::Busy,
        ModelError::ContextOverflow => EmbedError::TooLong,
        ModelError::Unauthorized | ModelError::Refused => EmbedError::Refused(why.to_string()),
        ModelError::Unreadable | ModelError::Unparseable => EmbedError::Failed {
            class: RetryClass::Retry,
            why: why.to_string(),
        },
    }
}

/// The vectors of a reply: one per text, each as long as the card says.
pub(crate) fn vectors_of(
    reply: InferReply,
    texts: usize,
    card: &EmbedderCard,
) -> Result<Vec<Vector>, EmbedError> {
    match reply {
        InferReply::Embed(EmbedReply { vectors, .. }) => {
            if vectors.len() != texts {
                return Err(fatal(format!(
                    "{} vectors for {texts} texts",
                    vectors.len()
                )));
            }
            let dims = usize::try_from(card.dims).unwrap_or(usize::MAX);
            vectors
                .into_iter()
                .map(|v| {
                    if v.0.len() == dims {
                        Ok(Vector(v.0))
                    } else {
                        Err(fatal(format!(
                            "a vector of {} numbers where the index has {dims}",
                            v.0.len()
                        )))
                    }
                })
                .collect()
        }
        InferReply::Refused(why) => Err(refusal(why)),
        InferReply::Failed(why) => Err(model_failure(why)),
        InferReply::Cancelled => Err(EmbedError::Failed {
            class: RetryClass::Retry,
            why: "cancelled".to_owned(),
        }),
        InferReply::Chat(_)
        | InferReply::CuaStep(_)
        | InferReply::Transcribed(_)
        | InferReply::Spoke(_) => Err(fatal("inferd answered with another kind of reply")),
    }
}

fn transport_failure(error: TransportError) -> EmbedError {
    match error {
        TransportError::Unreachable | TransportError::Closed => EmbedError::Unavailable,
        TransportError::Malformed(why) => fatal(why),
    }
}

impl<T: Transport> Embedder for InferdEmbedder<T> {
    fn card(&self) -> &EmbedderCard {
        &self.card
    }

    async fn embed(
        &self,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut session = self
            .transport
            .open(&need_for(&self.card), self.class, Tier::Fast)
            .await
            .map_err(transport_failure)?;
        let request = request_for(&self.card, self.class, texts, role, urgency);
        let reply = turn(&mut session, request)
            .await
            .map_err(|_| EmbedError::Unavailable)?;
        vectors_of(reply, texts.len(), &self.card)
    }
}
