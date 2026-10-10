//! JSON-RPC over frames: ids out, answers back, and a name for everything that
//! arrives.
//!
//! Copy-shaped from `app/web/src/acp/wire.rs` in the host repo and adapted, not
//! linked (`docs/architecture.md` §13): the correlation is the same — a map from
//! request id to whoever is waiting — and everything around it is different,
//! because this one sits on the frame seam ([`FrameSink`], [`Frame`]) rather
//! than on a WebSocket, and it is below a layer whose job is to *show* what it
//! cannot decode.
//!
//! **This module knows JSON-RPC and not ACP.** It has no idea what `initialize`
//! is; it allocates ids, matches answers to them, and hands whatever arrives to
//! the typed layer with a label on it. That is the line that keeps the envelope
//! and the protocol version from having to change together (§11 seam 1). The
//! one ACP type it names is [`v1::Error`], which is the JSON-RPC error object
//! with a Rust type over it — the envelope's own shape, not the protocol's.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use agent_client_protocol_schema::v1;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::call::CallError;
use crate::connection::{FrameSink, SendError};
use crate::frame::Frame;

/// A call this client made that has no answer yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCall {
    /// The id it went out under.
    pub id: i64,
    pub method: String,
    /// When it was registered, on the monotonic clock.
    pub since: Instant,
    /// Where the trace put the request frame, once it was sent.
    pub captured: Option<u64>,
}

type Answer = oneshot::Sender<Result<Value, CallError>>;

struct Waiting {
    answer: Answer,
    call: PendingCall,
}

/// The client's half of a JSON-RPC conversation.
///
/// A handle, not the conversation: cloning shares the pending calls, so the
/// task reading answers and the task making calls are the same conversation.
#[derive(Clone)]
pub(crate) struct Rpc {
    outgoing: FrameSink,
    calls: Arc<Mutex<Calls>>,
}

/// A call that has an id and a place to put the answer, and is not on the wire
/// yet.
///
/// Requests are prepared before they are sent because something above needs the
/// id first — the turn records which answer it is waiting for before the
/// question can possibly be answered (§11 seam 2). The prior art splits its
/// `call` the same way, for its own reason (measuring the encoded size).
pub(crate) struct Call {
    id: i64,
    frame: Frame,
    answer: oneshot::Receiver<Result<Value, CallError>>,
}

/// What arrived, named.
///
/// Deliberately not "a JSON-RPC message or an error": a frame that is none of
/// these is [`Unreadable`](Incoming::Unreadable) and goes on to be displayed,
/// because on this seam an unreadable frame is a fact about the agent rather
/// than a failure of the reader (§8).
pub(crate) enum Incoming {
    /// The agent asked for something. `id` is kept as it arrived — JSON-RPC ids
    /// are strings or numbers, and an answer that changed one would be an
    /// answer to a different question.
    ///
    /// The parameters come along undecoded, like a notification's: what they
    /// are is the typed layer's question, asked of one method
    /// (`session/request_permission`) and of nothing else, and the frame they
    /// arrived in stays on the entry either way (§8).
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// The agent said something, expecting no answer.
    Notification { method: String, params: Value },
    /// The agent answered something.
    Answer {
        id: Value,
        outcome: Result<Value, v1::Error>,
    },
    /// An error answering no request: `id` is null or absent, which JSON-RPC
    /// uses when the request's id could not be read.
    Unaddressed(v1::Error),
    /// Not a JSON-RPC message this could make sense of, and what was wrong.
    Unreadable(String),
}

#[derive(Default)]
struct Calls {
    next_id: i64,
    pending: HashMap<i64, Waiting>,
    /// Set once the connection is gone. A call registered after that would wait
    /// for an answer nobody is left to give.
    closed: bool,
}

impl Rpc {
    pub(crate) fn new(outgoing: FrameSink) -> Self {
        Self {
            outgoing,
            calls: Arc::default(),
        }
    }

    pub(crate) fn sink(&self) -> &FrameSink {
        &self.outgoing
    }

    /// Asks, and waits for the answer.
    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value, CallError> {
        self.send(self.prepare(method, params)?).await
    }

    /// Allocates the id and registers the call. Nothing is on the wire yet, and
    /// nothing will be until [`send`](Self::send).
    pub(crate) fn prepare(&self, method: &str, params: Value) -> Result<Call, CallError> {
        self.prepare_bounded(method, params, usize::MAX)
    }

    pub(crate) fn prepare_bounded(
        &self,
        method: &str,
        params: Value,
        max_bytes: usize,
    ) -> Result<Call, CallError> {
        let mut calls = self.calls();
        if calls.closed {
            return Err(CallError::Disconnected);
        }
        let id = calls.next_id + 1;
        let frame = Frame::new(
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string(),
        );
        if frame.as_str().len() > max_bytes {
            return Err(CallError::PromptTooLarge);
        }
        calls.next_id = id;
        let (answer, wait) = oneshot::channel();
        calls.pending.insert(
            id,
            Waiting {
                answer,
                call: PendingCall {
                    id,
                    method: method.to_owned(),
                    since: Instant::now(),
                    captured: None,
                },
            },
        );

        Ok(Call {
            id,
            frame,
            answer: wait,
        })
    }

    /// Sends a prepared call and waits for its answer.
    pub(crate) fn enqueue(&self, call: &Call) -> Result<(), CallError> {
        let captured = self
            .outgoing
            .try_send(call.frame.clone())
            .map_err(|error| match error {
                tokio::sync::mpsc::error::TrySendError::Full(_) => CallError::OutgoingBusy,
                tokio::sync::mpsc::error::TrySendError::Closed(_) => CallError::Disconnected,
            })
            .inspect_err(|_| self.forget(call.id))?;
        self.captured(call.id, captured);
        Ok(())
    }

    pub(crate) async fn wait(&self, call: Call) -> Result<Value, CallError> {
        // A dropped sender is the connection going away under the call.
        call.answer.await.unwrap_or(Err(CallError::Disconnected))
    }

    pub(crate) async fn send(&self, call: Call) -> Result<Value, CallError> {
        let Call { id, frame, answer } = call;
        match self.outgoing.send_captured(frame).await {
            Ok(captured) => self.captured(id, captured),
            Err(error) => {
                self.forget(id);
                return Err(from_send(error));
            }
        }
        answer.await.unwrap_or(Err(CallError::Disconnected))
    }

    fn captured(&self, id: i64, captured: Option<u64>) {
        if let Some(waiting) = self.calls().pending.get_mut(&id) {
            waiting.call.captured = captured;
        }
    }

    /// The calls still waiting for an answer, oldest first.
    pub(crate) fn pending(&self) -> Vec<PendingCall> {
        let mut pending: Vec<_> = self
            .calls()
            .pending
            .values()
            .map(|waiting| waiting.call.clone())
            .collect();
        pending.sort_by_key(|call| call.id);
        pending
    }

    pub(crate) fn is_pending(&self, id: i64) -> bool {
        self.calls().pending.contains_key(&id)
    }

    /// Ends a call without an answer from the agent, with `error` as its
    /// outcome. Says whether it was still waiting.
    pub(crate) fn fail(&self, id: i64, error: CallError) -> bool {
        let Some(waiting) = self.calls().pending.remove(&id) else {
            return false;
        };
        let _ = waiting.answer.send(Err(error));
        true
    }

    /// Says something the agent will not answer, and answers with the frame it
    /// said it in.
    ///
    /// The frame comes back because a notification is the only thing this
    /// client sends that nothing will ever refer to by id: a caller that has to
    /// name what it sent — a conformance rule armed by a `session/cancel` —
    /// otherwise has no handle on it at all.
    pub(crate) async fn notify(&self, method: &str, params: Value) -> Result<Frame, CallError> {
        let frame =
            Frame::new(json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string());
        self.outgoing
            .send(frame.clone())
            .await
            .map(|()| frame)
            .map_err(from_send)
    }

    /// Answers something the agent asked, with the id it asked under.
    pub(crate) async fn respond(
        &self,
        id: &Value,
        outcome: Result<Value, v1::Error>,
    ) -> Result<(), CallError> {
        let frame = match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        self.outgoing
            .send(Frame::new(frame.to_string()))
            .await
            .map_err(from_send)
    }

    /// Hands an answer to whoever is waiting for it, and says whether this was
    /// an answer to a question *this client asked*.
    ///
    /// The distinction is the whole point of the return value: an id nobody
    /// registered is an agent answering something nobody asked, which is
    /// evidence (§8) — while a caller that stopped awaiting is this program's
    /// own business, and blaming the agent for it would be the inspector
    /// inventing misbehaviour it then reports.
    pub(crate) fn answer(&self, id: i64, outcome: Result<Value, v1::Error>) -> bool {
        let Some(waiting) = self.calls().pending.remove(&id) else {
            return false;
        };
        // Dropped receiver: whoever asked has gone away, and the answer was
        // still ours.
        let _ = waiting.answer.send(outcome.map_err(CallError::Rejected));
        true
    }

    /// Ends the conversation: every call still waiting is told there will be no
    /// answer, and no new one is registered.
    ///
    /// Without this, disconnecting would leave a prompt awaiting an answer from
    /// a process that no longer exists — the hang this whole design is against
    /// (§6.1).
    pub(crate) fn close(&self) {
        self.outgoing.shutdown();
        let pending = {
            let mut calls = self.calls();
            calls.closed = true;
            std::mem::take(&mut calls.pending)
        };
        drop(pending);
    }

    fn forget(&self, id: i64) {
        self.calls().pending.remove(&id);
    }

    /// A poisoned map is still a map: a panic elsewhere must not turn a pending
    /// call into a second panic here.
    fn calls(&self) -> MutexGuard<'_, Calls> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Call {
    /// The id this call will be answered under.
    pub(crate) fn id(&self) -> i64 {
        self.id
    }

    /// The frame it will be asked in.
    ///
    /// For the same caller [`notify`](Rpc::notify) hands its frame back to: a
    /// conformance rule reads its claim beside the traffic that decides it, and
    /// the request is half of that traffic (§15 q7).
    pub(crate) fn frame(&self) -> &Frame {
        &self.frame
    }
}

/// Whether an id could be one this client issued.
///
/// Every id it issues is a number, counted up from one, so a string id was
/// somebody else's question and its answer is not ours to match — the one place
/// that rule is stated, and the one place the narrowing happens.
pub(crate) fn ours(id: &Value) -> Option<i64> {
    id.as_i64()
}

/// An id of this client's carried with its type changed: `"3"` or `3.0` for
/// `3`. JSON-RPC does not match those; this is for saying so.
pub(crate) fn mistyped(id: &Value) -> Option<i64> {
    match id {
        Value::String(text) => crate::frame::integral(text),
        Value::Number(number) => crate::frame::integral(&number.to_string()),
        _ => None,
    }
}

/// The id of an answer, read off the start of one the transport cut short:
/// the top-level `id`, when it comes before `result` or `error` and no
/// `method` does.
pub(crate) fn answer_id(prefix: &str) -> Option<i64> {
    let mut rest = prefix.trim_start().strip_prefix('{')?;
    let mut id = None;
    loop {
        let (key, after) = token::<String>(rest)?;
        match key.as_str() {
            "method" => return None,
            "result" | "error" => return id,
            _ => {}
        }
        rest = after.trim_start().strip_prefix(':')?;
        match key.as_str() {
            "id" => {
                let (value, after) = token::<Value>(rest)?;
                id = ours(&value);
                rest = after;
            }
            _ => rest = token::<serde::de::IgnoredAny>(rest)?.1,
        }
        rest = rest.trim_start().strip_prefix(',')?;
    }
}

/// One JSON value off the front of `text`, and what follows it.
fn token<T: serde::de::DeserializeOwned>(text: &str) -> Option<(T, &str)> {
    let mut values = serde_json::Deserializer::from_str(text).into_iter::<T>();
    let value = values.next()?.ok()?;
    Some((value, &text[values.byte_offset()..]))
}

/// Reads a frame as a JSON-RPC message, without judging what is in it.
///
/// The four shapes are told apart by the two fields JSON-RPC uses for exactly
/// that — `method` and `id` — and nothing else is inspected. What the message
/// *means* is the typed layer's question, one version at a time (§11 seam 1).
pub(crate) fn classify(frame: &Frame) -> Incoming {
    let message = match crate::decode::frame_value(frame.as_str()) {
        Ok(message) => message,
        Err(error) => return Incoming::Unreadable(error.to_string()),
    };

    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").filter(|id| !id.is_null()).cloned();
    let params = || message.get("params").cloned().unwrap_or(Value::Null);

    match (method, id) {
        (Some(method), Some(id)) => Incoming::Request {
            id,
            method: method.to_owned(),
            params: params(),
        },
        (Some(method), None) => Incoming::Notification {
            method: method.to_owned(),
            params: params(),
        },
        (None, Some(id)) => Incoming::Answer {
            id,
            outcome: outcome(&message),
        },
        (None, None) => match message.get("error") {
            Some(error) => Incoming::Unaddressed(read_error(error)),
            None => Incoming::Unreadable(
                "a JSON-RPC message carries a method, an id, or both; this carried neither"
                    .to_owned(),
            ),
        },
    }
}

/// The result or the error of an answer.
///
/// An error object v1 cannot read still ends the call — the alternative is a
/// caller waiting forever on a malformed refusal — so it becomes an internal
/// error carrying the object it could not read as data. The verbatim frame is
/// in the trace either way.
fn outcome(message: &Value) -> Result<Value, v1::Error> {
    match message.get("error") {
        Some(error) => Err(read_error(error)),
        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    }
}

fn read_error(error: &Value) -> v1::Error {
    crate::decode::from_value::<v1::Error>(error.clone()).unwrap_or_else(|problem| {
        v1::Error::internal_error()
            .data(json!({ "unreadableError": error, "problem": problem.to_string() }))
    })
}

/// A frame that did not go out is a call that will not be answered.
fn from_send(error: SendError) -> CallError {
    match error {
        // Both mean the same thing to a caller: the agent did not hear it. The
        // one frame this end can compose that a message-per-line transport
        // would split is not one this module composes — `serde_json` escapes
        // newlines inside strings — so this arm is a shape, not a path.
        SendError::NotOneMessage | SendError::Disconnected => CallError::Disconnected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answers_id_is_read_off_its_retained_start() {
        assert_eq!(
            answer_id(r#"{"jsonrpc": "2.0", "id": 3, "result": {"pad": "xxx"#),
            Some(3)
        );
        assert_eq!(answer_id(r#"{"id":7,"error":{"code":"#), Some(7));
        assert_eq!(
            answer_id(r#"{"jsonrpc":"2.0","_meta":{"a":[1,"}"]},"id":4,"result""#),
            Some(4)
        );
        // A request the agent made, or an id after the payload, says nothing.
        assert_eq!(answer_id(r#"{"id":3,"method":"x","params":{"#), None);
        assert_eq!(answer_id(r#"{"result":{"pad":"xxx"#), None);
        assert_eq!(answer_id(r#"{"id":"3","result":{"#), None);
        assert_eq!(answer_id("not json"), None);
    }

    #[test]
    fn ids_of_the_wrong_type_are_recognized() {
        assert_eq!(mistyped(&json!("3")), Some(3));
        assert_eq!(mistyped(&json!(3.0)), Some(3));
        assert_eq!(mistyped(&json!(3.5)), None);
        assert_eq!(mistyped(&json!("x")), None);
    }

    #[test]
    fn an_error_without_an_id_is_an_error_answering_nothing() {
        let frame = Frame::new(
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}"#,
        );
        assert!(
            matches!(classify(&frame), Incoming::Unaddressed(error) if i32::from(error.code) == -32700)
        );
        let frame = Frame::new(r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid"}}"#);
        assert!(matches!(classify(&frame), Incoming::Unaddressed(_)));
    }
}
