//! `InferdEmbedder`: embeddings through an inferd session.

use super::turn;
use almanac_service::class_from_tag;
use porter_client::{Transport, TransportError};
use porter_core::capability::Modality;
use porter_core::consent::Usage;
use porter_core::need::{DimsNeed, EmbedNeed};
use porter_core::{DataClass, Dims, Need, Tier};
use porter_infer::{
    EmbedReply, EmbedRequest, EmbedRole as WireRole, InferRefusal, InferReply, InferRequest,
    ModelError,
};
use recall::{Classed, EmbedError, EmbedRole, Embedder, EmbedderCard, RetryClass, Urgency, Vector};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Embeds through inferd: `Need::Embeddings` for the card's vector length, `InferRequest::Embed`
/// with `Usage::Background` for indexing. inferd puts the model's query and document prefixes in
/// front itself (the card carries them only so the index can tell when they changed), and the
/// vectors must have the card's length: a model that answers otherwise is another vector space.
///
/// **Data classes.** A session and an `EmbedRequest` carry one data class, and inferd applies
/// that class's floor (mail, voice and prompts stay on this computer). [`Embedder::embed_classed`]
/// therefore partitions a batch by the class each text carries, opens one session per class
/// (for that call; a session is not kept), sends each class's texts in one request and returns
/// the vectors in input order.
///
/// **Strictest-class pinning.** One index is one model: a document's class cannot change *which*
/// model embeds it, only whether that model may receive it. The card therefore names a model
/// that satisfies the strictest class the Space holds, which on this computer means an
/// on-device model; an index whose model were a cloud one would have its `Mail` and `Voice`
/// documents refused by inferd (`Refused`, Fatal) and stay lexical. Texts that carry no class,
/// and every search query (the person's words may quote anything), are sent as the embedder's
/// *pin*: [`DataClass::Mail`] from [`InferdEmbedder::new`], the strictest class memory holds in
/// quantity, or the class given to [`InferdEmbedder::for_class`].
#[derive(Debug)]
pub struct InferdEmbedder<T: Transport> {
    transport: Arc<T>,
    card: EmbedderCard,
    pin: DataClass,
}

impl<T: Transport> InferdEmbedder<T> {
    /// Embeds over `transport` with `card`, pinned to mail (see [`InferdEmbedder::for_class`]).
    pub fn new(transport: Arc<T>, card: EmbedderCard) -> Self {
        Self::for_class(transport, card, DataClass::Mail)
    }

    /// Embeds over `transport` with `card`; queries and texts with no class are sent as `pin`.
    pub fn for_class(transport: Arc<T>, card: EmbedderCard, pin: DataClass) -> Self {
        Self {
            transport,
            card,
            pin,
        }
    }

    /// One session of `class`, one request over all of `texts`.
    async fn embed_as(
        &self,
        class: DataClass,
        texts: &[String],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut session = self
            .transport
            .open(&need_for(&self.card), class, Tier::Fast)
            .await
            .map_err(transport_failure)?;
        let request = request_for(&self.card, class, texts, role, urgency);
        let reply = turn(&mut session, request)
            .await
            .map_err(|_| EmbedError::Unavailable)?;
        vectors_of(reply, texts.len(), &self.card)
    }
}

/// The need that finds an embedding model for `card`.
pub(crate) fn need_for(card: &EmbedderCard) -> Need {
    Need::Embeddings(EmbedNeed::new(
        DimsNeed::Exactly(Dims(card.dims)),
        BTreeSet::from([Modality::Text]),
    ))
}

/// The wire request for `texts`.
pub(crate) fn request_for(
    card: &EmbedderCard,
    class: DataClass,
    texts: &[String],
    role: EmbedRole,
    urgency: Urgency,
) -> InferRequest {
    InferRequest::Embed(EmbedRequest::new(
        texts.to_vec(),
        match role {
            EmbedRole::Query => WireRole::Query,
            EmbedRole::Document => WireRole::Document,
        },
        DimsNeed::Exactly(Dims(card.dims)),
        class,
        match urgency {
            Urgency::Interactive => Usage::Interactive,
            Urgency::Background => Usage::Background,
        },
    ))
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
        // porter may add reasons (`InferRefusal` is non-exhaustive); one not named is a refusal.
        _ => EmbedError::Refused(why.to_string()),
    }
}

/// Why the model call failed.
pub(crate) fn model_failure(why: ModelError) -> EmbedError {
    match why {
        ModelError::Unreachable | ModelError::NotReady => EmbedError::Unavailable,
        ModelError::RateLimited(_) => EmbedError::Busy,
        ModelError::ContextOverflow => EmbedError::TooLong,
        ModelError::Unauthorized
        | ModelError::PaymentRequired
        | ModelError::SignInRefused
        | ModelError::Refused => EmbedError::Refused(why.to_string()),
        ModelError::Unreadable | ModelError::Unparseable | ModelError::OnlyThought { .. } => {
            EmbedError::Failed {
                class: RetryClass::Retry,
                why: why.to_string(),
            }
        }
        // porter may add failures (`ModelError` is non-exhaustive); one not named is a refusal.
        _ => EmbedError::Refused(why.to_string()),
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
        // porter may add reply kinds (`InferReply` is non-exhaustive); one not named is unreadable.
        _ => Err(fatal("inferd answered with another kind of reply")),
    }
}

fn transport_failure(error: TransportError) -> EmbedError {
    match error {
        TransportError::Unreachable | TransportError::Closed => EmbedError::Unavailable,
        // A caller inferd's table does not name, or one without the grant: asking again does not
        // help until someone changes who may call, so it is fatal with the daemon's own text.
        TransportError::Denied(why) | TransportError::Malformed(why) => fatal(why),
        // porter may add reasons (`TransportError` is non-exhaustive); one this code does not
        // name is fatal too, with its own text, since asking again will not change it.
        other => fatal(other.to_string()),
    }
}

/// The positions of each class in `texts`, classes in their own order; a text with no known
/// class goes with `pin`.
pub(crate) fn by_class(texts: &[Classed], pin: DataClass) -> BTreeMap<DataClass, Vec<usize>> {
    let mut groups: BTreeMap<DataClass, Vec<usize>> = BTreeMap::new();
    for (at, t) in texts.iter().enumerate() {
        groups
            .entry(class_from_tag(&t.class).unwrap_or(pin))
            .or_default()
            .push(at);
    }
    groups
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
        self.embed_as(self.pin, texts, role, urgency).await
    }

    async fn embed_classed(
        &self,
        texts: &[Classed],
        role: EmbedRole,
        urgency: Urgency,
    ) -> Result<Vec<Vector>, EmbedError> {
        let mut out: Vec<Option<Vector>> = vec![None; texts.len()];
        for (class, at) in by_class(texts, self.pin) {
            let group: Vec<String> = at.iter().map(|&i| texts[i].text.clone()).collect();
            let vectors = self.embed_as(class, &group, role, urgency).await?;
            at.into_iter()
                .zip(vectors)
                .for_each(|(slot, v)| out[slot] = Some(v));
        }
        out.into_iter()
            .map(|v| v.ok_or_else(|| fatal("a text was left without a vector")))
            .collect()
    }
}
