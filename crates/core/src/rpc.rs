//! JSON-RPC over frames: ids out, answers back, and a name for everything that
//! arrives.
//!
//! This module knows JSON-RPC and not ACP, so the envelope and the protocol
//! version can change independently (`docs/architecture.md` §11 seam 1). The
//! one ACP type it names is [`v1::Error`], the JSON-RPC error object.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use agent_client_protocol_schema::v1;
use serde_json::value::RawValue;
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
    /// Whether a person sent it from the raw request composer.
    pub hand_sent: bool,
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
/// Prepared before sending so callers can register the id before the answer
/// could possibly arrive.
pub(crate) struct Call {
    id: i64,
    frame: Frame,
    answer: oneshot::Receiver<Result<Value, CallError>>,
}

/// What arrived, named.
///
/// A frame that is not a JSON-RPC message is [`Unreadable`](Incoming::Unreadable)
/// and is still displayed: it is a fact about the agent (§8).
pub(crate) enum Incoming {
    /// The agent asked for something. `id` is kept as it arrived (string or
    /// number) so the answer carries the same id. Params are undecoded.
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
                    hand_sent: false,
                },
            },
        );

        Ok(Call {
            id,
            frame,
            answer: wait,
        })
    }

    /// Allocates an id for a request a person composed, and registers it
    /// like any other call: answered under that id, listed as pending, and
    /// cancellable. `params` is carried as written, only made one line.
    ///
    /// The answer still goes through [`answer`](Self::answer), but the typed
    /// layer asks [`is_hand_sent`](Self::is_hand_sent) first and keeps it out
    /// of every store.
    pub(crate) fn prepare_by_hand(
        &self,
        method: &str,
        params: Option<&RawValue>,
    ) -> Result<Call, CallError> {
        let mut calls = self.calls();
        if calls.closed {
            return Err(CallError::Disconnected);
        }
        let id = calls.next_id + 1;
        calls.next_id = id;
        let frame = Frame::by_hand(composed(Some(id), method, params));
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
                    hand_sent: true,
                },
            },
        );
        Ok(Call {
            id,
            frame,
            answer: wait,
        })
    }

    /// Sends a prepared call without waiting for its answer, and answers with
    /// where the trace put it. Whoever wants the answer reads it off the trace
    /// ([`Trace::counterpart`](crate::Trace::counterpart)).
    pub(crate) async fn send_unawaited(&self, call: Call) -> Result<Option<u64>, CallError> {
        let Call { id, frame, .. } = call;
        match self.outgoing.send_captured(frame).await {
            Ok(captured) => {
                self.captured(id, captured);
                Ok(captured)
            }
            Err(error) => {
                self.forget(id);
                Err(from_send(error))
            }
        }
    }

    /// Sends a hand-composed notification, and answers
    /// with where the trace put it.
    pub(crate) async fn notify_by_hand(
        &self,
        method: &str,
        params: Option<&RawValue>,
    ) -> Result<Option<u64>, CallError> {
        if self.calls().closed {
            return Err(CallError::Disconnected);
        }
        let frame = Frame::by_hand(composed(None, method, params));
        self.outgoing.send_captured(frame).await.map_err(from_send)
    }

    /// Whether the call under this id was sent by hand and is still waiting.
    pub(crate) fn is_hand_sent(&self, id: i64) -> bool {
        self.calls()
            .pending
            .get(&id)
            .is_some_and(|waiting| waiting.call.hand_sent)
    }

    /// Queues a prepared call without waiting: fails with `OutgoingBusy` when
    /// the outgoing queue is full. Collect the answer with [`wait`](Self::wait).
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

    /// Sends a notification and returns the frame sent, since a notification
    /// has no id for a conformance rule to refer back to.
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

    /// Hands an answer to whoever is waiting for it. Returns false when no call
    /// is registered under `id` (an unsolicited answer, §8); a caller that
    /// stopped awaiting still counts as ours.
    pub(crate) fn answer(&self, id: i64, outcome: Result<Value, v1::Error>) -> bool {
        let Some(waiting) = self.calls().pending.remove(&id) else {
            return false;
        };
        let _ = waiting.answer.send(outcome.map_err(CallError::Rejected));
        true
    }

    /// Ends the conversation: every call still waiting is told there will be no
    /// answer, and no new one is registered, so nothing hangs on a dead agent
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

    /// The frame it will be asked in, kept as evidence for conformance rules.
    pub(crate) fn frame(&self) -> &Frame {
        &self.frame
    }
}

/// The text of a request or notification a person composed: the envelope's
/// fields, then `params` exactly as written but on one line, because a frame
/// is one line on a message-per-line transport.
fn composed(id: Option<i64>, method: &str, params: Option<&RawValue>) -> String {
    let method = Value::from(method).to_string();
    let mut text = String::from(r#"{"jsonrpc":"2.0","#);
    if let Some(id) = id {
        text.push_str(&format!(r#""id":{id},"#));
    }
    text.push_str(&format!(r#""method":{method}"#));
    if let Some(params) = params {
        text.push_str(r#","params":"#);
        text.push_str(&one_line(params.get()));
    }
    text.push('}');
    text
}

/// JSON with the whitespace between its tokens taken out, and nothing else
/// changed: keys stay in their order and numbers keep their spelling. Valid
/// JSON has no raw newline inside a string, so what is left is one line.
fn one_line(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let (mut quoted, mut escaped) = (false, false);
    for character in json.chars() {
        if quoted {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
        } else if character == '"' {
            quoted = true;
            out.push(character);
        } else if !matches!(character, ' ' | '\t' | '\n' | '\r') {
            out.push(character);
        }
    }
    out
}

/// Whether an id could be one this client issued. Every id it issues is an
/// integer, so a string id is never ours.
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

/// Reads a frame as a JSON-RPC message, told apart by `method` and `id` only.
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
/// An unreadable error object still ends the call, as an internal error
/// carrying the object as data, so a caller never waits on a malformed refusal.
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
        // Both mean the agent did not hear it. `NotOneMessage` cannot happen
        // here: `serde_json` escapes newlines inside strings.
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
    fn a_hand_composed_frame_keeps_its_params_as_written_on_one_line() {
        let params: Box<RawValue> =
            serde_json::from_str("{\n  \"z\": 1.50,\n  \"a\": \"two words\\n\"\n}").unwrap();
        assert_eq!(
            composed(Some(7), "x/y", Some(&params)),
            r#"{"jsonrpc":"2.0","id":7,"method":"x/y","params":{"z":1.50,"a":"two words\n"}}"#
        );
        assert_eq!(
            composed(None, "note", None),
            r#"{"jsonrpc":"2.0","method":"note"}"#
        );
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
