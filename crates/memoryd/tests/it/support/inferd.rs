//! A fake `org.quire.Inference1` on a private bus: `Open` returns one end of a socketpair and
//! serves the other, answering embedding requests with `FakeEmbedder`'s vectors and task requests
//! with a fixed text, and remembering the class of every session and request. It speaks the
//! real frames (`porter_core::wire`), so memoryd's own client code runs unchanged against it.

use porter_core::wire::{FrameRead, decode_frame, encode_frame};
use porter_core::{AccountId, Locality, ModelId, Tokens};
use porter_dbus::{Details, INFERENCE_BUS, INFERENCE_PATH, NeedArg};
use porter_infer::{
    ChatReply, ClientFrame, EmbedReply, EmbedVector, InferEvent, InferRefusal, InferReply,
    InferRequest, ServedBy, StopReason, TokenUsage,
};
use recall::FakeEmbedder;
use std::os::unix::net::UnixStream as StdStream;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use zbus::fdo;
use zbus::zvariant::OwnedFd;

/// What the fake saw, in order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Seen {
    /// The class slug of every `Open`.
    pub opens: Vec<String>,
    /// The class and inputs of every embedding request.
    pub embeds: Vec<(String, Vec<String>)>,
    /// The class slug of every task request.
    pub tasks: Vec<String>,
}

/// The interface object.
#[derive(Debug)]
pub struct FakeInferd {
    seen: Arc<Mutex<Seen>>,
    draft: String,
}

impl FakeInferd {
    /// A fake whose task replies are `draft`, and what it saw.
    pub fn new(draft: &str) -> (Self, Arc<Mutex<Seen>>) {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let fake = Self {
            seen: seen.clone(),
            draft: draft.to_owned(),
        };
        (fake, seen)
    }

    /// Serves `self` on `connection` under the real bus name and path.
    pub async fn serve(self, connection: &zbus::Connection) {
        connection
            .object_server()
            .at(INFERENCE_PATH, self)
            .await
            .expect("serve the object");
        connection
            .request_name(INFERENCE_BUS)
            .await
            .expect("own the name");
    }
}

#[zbus::interface(name = "org.quire.Inference1")]
impl FakeInferd {
    async fn open(
        &self,
        _need: NeedArg,
        class: String,
        _tier: String,
        _options: Details,
    ) -> fdo::Result<OwnedFd> {
        self.seen.lock().expect("lock").opens.push(class.clone());
        let (ours, theirs) = StdStream::pair().map_err(|e| fdo::Error::Failed(e.to_string()))?;
        ours.set_nonblocking(true)
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let stream = UnixStream::from_std(ours).map_err(|e| fdo::Error::Failed(e.to_string()))?;
        tokio::spawn(serve_session(
            stream,
            class,
            self.seen.clone(),
            self.draft.clone(),
        ));
        Ok(OwnedFd::from(std::os::fd::OwnedFd::from(theirs)))
    }
}

fn served() -> ServedBy {
    ServedBy {
        account: AccountId::parse("local").expect("account"),
        model: ModelId::parse("fake-embed").expect("model"),
        locality: Locality::OnDevice,
    }
}

fn usage() -> TokenUsage {
    TokenUsage {
        input: Tokens(1),
        output: Tokens(0),
        cached: Tokens(0),
    }
}

fn answer(class: &str, request: InferRequest, seen: &Mutex<Seen>, draft: &str) -> Vec<InferEvent> {
    let reply = match request {
        InferRequest::Embed(embed) => {
            let vectors = embed
                .inputs
                .iter()
                .map(|text| EmbedVector(FakeEmbedder::vector(text).0))
                .collect();
            seen.lock()
                .expect("lock")
                .embeds
                .push((class.to_owned(), embed.inputs));
            InferReply::Embed(EmbedReply::new(vectors, usage(), served()))
        }
        InferRequest::Task(_) => {
            seen.lock().expect("lock").tasks.push(class.to_owned());
            InferReply::Chat(ChatReply::new(
                draft.to_owned(),
                StopReason::EndTurn,
                usage(),
                served(),
            ))
        }
        _ => InferReply::Refused(InferRefusal::Unsupported),
    };
    vec![InferEvent::Routed(served()), InferEvent::Finished(reply)]
}

async fn serve_session(
    mut stream: UnixStream,
    class: String,
    seen: Arc<Mutex<Seen>>,
    draft: String,
) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        while let Ok(FrameRead::Complete(envelope, used)) = decode_frame::<ClientFrame>(&buffer) {
            buffer.drain(..used);
            if let ClientFrame::Request(request) = envelope.body {
                for event in answer(&class, request, &seen, &draft) {
                    let bytes = encode_frame(&event).expect("encode");
                    if stream.write_all(&bytes).await.is_err() {
                        return;
                    }
                }
            }
        }
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
}
