//! A client-service request the agent is waiting on (`docs/client-services.md`):
//! a claimed `fs/*` or `terminal/*` call, or any other request this client does
//! not service when the reader asked to answer those by hand.
//!
//! The third blocking request, beside permission and elicitation, sharing their
//! plumbing ([`crate::blocking`]): pending until answered, pinned against
//! retention, counted against the cap, answered with the cancellation error
//! when the turn is cancelled or the session left, and withdrawn by the agent
//! with `$/cancel_request`. What it is answered with is JSON the reader edits —
//! a result or an error — or a rules file's canned answer.

use std::fmt;
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1;
use serde_json::{Value, json};

use crate::blocking::{Blocking, RequestId, Resolver};
use crate::call::CallError;
use crate::store::locked;

/// A client-service request, and the answer that will release it.
///
/// A handle: cloning shares one request, like [`PermissionRequest`].
///
/// [`PermissionRequest`]: crate::PermissionRequest
#[derive(Clone)]
pub struct ServiceRequest(Arc<Called>);

struct Called {
    id: RequestId,
    method: String,
    params: Value,
    claimed: bool,
    state: Mutex<ServiceState>,
    resolver: Resolver,
}

/// Where a service request stands.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ServiceState {
    /// The agent is waiting for an answer.
    Waiting,
    /// An answer is on its way.
    Answering,
    /// The agent was sent this.
    Answered(ServiceAnswer),
    /// The turn was cancelled or the session left, so it was answered with the
    /// cancellation error.
    Cancelled,
    /// Nobody answered it and nobody can now.
    Abandoned,
    /// The agent withdrew it with `$/cancel_request`.
    CancelledByAgent,
}

/// What a service request was answered with, and who chose it.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceAnswer {
    pub reply: ServiceReply,
    pub by: AnsweredBy,
}

/// A result or an error, as sent.
#[derive(Clone, Debug, PartialEq)]
pub enum ServiceReply {
    Result(Value),
    Error(v1::Error),
}

/// Who chose an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnsweredBy {
    /// The reader, in the response editor.
    Reader,
    /// A rules file; `rule` counts from zero.
    Script { rule: usize },
}

impl ServiceRequest {
    pub(crate) fn new(
        id: RequestId,
        method: String,
        params: Value,
        claimed: bool,
        resolver: Resolver,
    ) -> Self {
        Self(Arc::new(Called {
            id,
            method,
            params,
            claimed,
            state: Mutex::new(ServiceState::Waiting),
            resolver,
        }))
    }

    pub fn id(&self) -> &RequestId {
        &self.0.id
    }

    pub fn method(&self) -> &str {
        &self.0.method
    }

    /// The params as the agent sent them.
    pub fn params(&self) -> &Value {
        &self.0.params
    }

    /// Whether this client claimed the service the method belongs to — `false`
    /// for a request answered only because the reader opted in to answering
    /// unserviced requests, or because a rule matched it.
    pub fn claimed(&self) -> bool {
        self.0.claimed
    }

    /// The session the params name, if they name one.
    pub fn session_id(&self) -> Option<&str> {
        self.0.params.get("sessionId").and_then(Value::as_str)
    }

    pub fn state(&self) -> ServiceState {
        locked(&self.0.state).clone()
    }

    pub fn is_waiting(&self) -> bool {
        matches!(*locked(&self.0.state), ServiceState::Waiting)
    }

    /// A valid result for this method, to start the response editor from.
    pub fn skeleton(&self) -> Value {
        skeleton(&self.0.method)
    }

    /// The reader's result, sent as written.
    pub async fn respond(&self, result: Value) -> Result<(), CallError> {
        self.answer(ServiceReply::Result(result), AnsweredBy::Reader)
            .await
    }

    /// The reader's error, sent as written.
    pub async fn refuse(&self, error: v1::Error) -> Result<(), CallError> {
        self.answer(ServiceReply::Error(error), AnsweredBy::Reader)
            .await
    }

    pub(crate) async fn answer(
        &self,
        reply: ServiceReply,
        by: AnsweredBy,
    ) -> Result<(), CallError> {
        self.claim()?;
        let told = match &reply {
            ServiceReply::Result(result) => {
                self.0.resolver.respond_to(&self.0.id, result.clone()).await
            }
            ServiceReply::Error(error) => self.0.resolver.refuse(&self.0.id, error.clone()).await,
        };
        self.settle(match &told {
            Ok(()) => ServiceState::Answered(ServiceAnswer { reply, by }),
            Err(_) => ServiceState::Abandoned,
        });
        told
    }

    /// The turn it belonged to was cancelled, or its session left.
    pub(crate) async fn cancelled(&self) -> Result<(), CallError> {
        self.claim()?;
        let told = self
            .0
            .resolver
            .refuse(&self.0.id, v1::Error::request_cancelled())
            .await;
        self.settle(match &told {
            Ok(()) => ServiceState::Cancelled,
            Err(_) => ServiceState::Abandoned,
        });
        told
    }

    /// The agent withdrew it with `$/cancel_request`.
    pub(crate) async fn withdrawn(&self) -> Result<(), CallError> {
        self.claim()?;
        let told = self
            .0
            .resolver
            .refuse(&self.0.id, v1::Error::request_cancelled())
            .await;
        self.settle(match &told {
            Ok(()) => ServiceState::CancelledByAgent,
            Err(_) => ServiceState::Abandoned,
        });
        told
    }

    pub(crate) fn abandoned(&self) {
        {
            let mut state = locked(&self.0.state);
            if !matches!(*state, ServiceState::Waiting) {
                return;
            }
            *state = ServiceState::Abandoned;
        }
        self.0.resolver.changed();
    }

    fn claim(&self) -> Result<(), CallError> {
        let mut state = locked(&self.0.state);
        match *state {
            ServiceState::Waiting => {
                *state = ServiceState::Answering;
                Ok(())
            }
            _ => Err(CallError::NotWaiting),
        }
    }

    fn settle(&self, state: ServiceState) {
        *locked(&self.0.state) = state;
        self.0.resolver.changed();
    }
}

/// Reads what the reader typed into a response editor as JSON, through the same
/// seam a frame is read through (§8), so a number keeps the spelling it was
/// typed with as far as the wire allows.
pub fn read_json(text: &str) -> Result<Value, String> {
    crate::decode::frame_value(text).map_err(|error| error.to_string())
}

/// A valid result for each client-service method, and `{}` for anything else.
///
/// Values a reader replaces rather than placeholders a decoder would refuse:
/// each one decodes as the method's v1 response.
pub fn skeleton(method: &str) -> Value {
    let names = &v1::CLIENT_METHOD_NAMES;
    if method == names.fs_read_text_file {
        json!({"content": ""})
    } else if method == names.terminal_create {
        json!({"terminalId": "term-1"})
    } else if method == names.terminal_output {
        json!({"output": "", "truncated": false})
    } else if method == names.terminal_wait_for_exit {
        json!({"exitCode": 0})
    } else {
        // `fs/write_text_file`, `terminal/kill`, `terminal/release`, and every
        // method this client has no shape for.
        json!({})
    }
}

impl fmt::Debug for ServiceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceRequest")
            .field("id", &self.0.id)
            .field("method", &self.0.method)
            .field("params", &self.0.params)
            .field("state", &*locked(&self.0.state))
            .finish()
    }
}

/// Identity, like the other blocking requests.
impl PartialEq for ServiceRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Blocking for ServiceRequest {
    fn is_waiting(&self) -> bool {
        Self::is_waiting(self)
    }

    fn abandoned(&self) {
        Self::abandoned(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skeleton_decodes_as_its_methods_response() {
        let names = &v1::CLIENT_METHOD_NAMES;
        let decodes = |method: &str, decode: fn(Value) -> bool| {
            assert!(decode(skeleton(method)), "{method}: {}", skeleton(method));
        };
        decodes(names.fs_read_text_file, |value| {
            crate::decode::from_value::<v1::ReadTextFileResponse>(value).is_ok()
        });
        decodes(names.fs_write_text_file, |value| {
            crate::decode::from_value::<v1::WriteTextFileResponse>(value).is_ok()
        });
        decodes(names.terminal_create, |value| {
            crate::decode::from_value::<v1::CreateTerminalResponse>(value).is_ok()
        });
        decodes(names.terminal_output, |value| {
            crate::decode::from_value::<v1::TerminalOutputResponse>(value).is_ok()
        });
        decodes(names.terminal_wait_for_exit, |value| {
            crate::decode::from_value::<v1::WaitForTerminalExitResponse>(value).is_ok()
        });
        decodes(names.terminal_kill, |value| {
            crate::decode::from_value::<v1::KillTerminalResponse>(value).is_ok()
        });
        decodes(names.terminal_release, |value| {
            crate::decode::from_value::<v1::ReleaseTerminalResponse>(value).is_ok()
        });
    }
}
