//! What every blocking request has in common (`docs/architecture.md` §7.2,
//! §7.8): the id it arrived under, the way an answer reaches the wire, and the
//! list of the ones still waiting.
//!
//! Each request carries its own [`Resolver`], so a screen can answer it without
//! knowing which connection it came from, and the request itself enforces that
//! the agent is still waiting and gets exactly one answer. The plumbing is
//! shared ([`Pending`]); each kind of request keeps its own state machine.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::rpc::Rpc;
use crate::store::locked;
use crate::timeline::Ticker;

/// The id a request arrived under, kept exactly as it arrived (string or
/// number). Opaque: handed back in the answer, never interpreted.
#[derive(Clone, Debug)]
pub struct RequestId(Value);

impl RequestId {
    pub(crate) fn new(id: Value) -> Self {
        Self(id)
    }

    /// The raw id, for addressing the response.
    pub(crate) fn value(&self) -> &Value {
        &self.0
    }
}

impl fmt::Display for RequestId {
    /// The id as JSON, in the form the agent used: `7`, or `"3a9f…"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for RequestId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// JSON-RPC ids are strings or integers, so `Value`'s float caveat for [`Eq`]
/// does not apply.
impl Eq for RequestId {}

impl Hash for RequestId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // By the JSON, which is the one form every shape of id has.
        self.0.to_string().hash(state);
    }
}

/// What it takes to answer a blocking request: the wire to answer on, and a way
/// to say that an entry already on screen has changed.
///
/// Holds a [`Ticker`] rather than the timeline, to avoid a cycle between the
/// timeline and the requests in it.
#[derive(Clone)]
pub(crate) struct Resolver {
    rpc: Rpc,
    timeline: Ticker,
}

impl Resolver {
    pub(crate) fn new(rpc: Rpc, timeline: Ticker) -> Self {
        Self { rpc, timeline }
    }

    /// Puts an answer on the wire, addressed to the id it answers.
    ///
    /// The body is raw JSON because an elicitation answer may deliberately
    /// violate the requested schema (§7.8).
    pub(crate) async fn respond_to(&self, id: &RequestId, body: Value) -> Result<(), CallError> {
        self.rpc.respond(id.value(), Ok(body)).await
    }

    /// Puts an error answer on the wire, addressed to the id it answers.
    pub(crate) async fn refuse(&self, id: &RequestId, error: v1::Error) -> Result<(), CallError> {
        self.rpc.respond(id.value(), Err(error)).await
    }

    /// Says that a request changed, to whoever is watching the timeline it sits
    /// on.
    pub(crate) fn changed(&self) {
        self.timeline.tick();
    }
}

/// What [`Pending`] needs to know about the requests in it. Answering stays on
/// each request's own type.
pub(crate) trait Blocking: Clone {
    /// Whether the agent is still waiting for an answer to this one.
    fn is_waiting(&self) -> bool;

    /// Nobody answered it and nobody can now.
    fn abandoned(&self);
}

/// The requests the agent is waiting on, on this connection.
///
/// Screens render requests off the timeline; this list answers the questions
/// about all of them at once: who a cancel owes an answer to (§7.2), and
/// whether the agent is waiting on anybody.
pub(crate) struct Pending<T> {
    requests: Arc<Mutex<Vec<T>>>,
}

impl<T> Clone for Pending<T> {
    /// A handle to one list. Not derived, which would demand `T: Clone`.
    fn clone(&self) -> Self {
        Self {
            requests: Arc::clone(&self.requests),
        }
    }
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl<T: Blocking> Pending<T> {
    pub(crate) fn arrived(&self, request: T) {
        locked(&self.requests).push(request);
    }

    /// The ones still waiting. Answered requests are pruned on the way, so the
    /// list does not grow with a turn.
    pub(crate) fn waiting(&self) -> Vec<T> {
        let mut requests = locked(&self.requests);
        requests.retain(T::is_waiting);
        requests.clone()
    }

    /// Takes everything, leaving nothing pending, for a caller that answers
    /// what it takes.
    pub(crate) fn take(&self) -> Vec<T> {
        std::mem::take(&mut *locked(&self.requests))
    }

    /// Gives up on everything still waiting. Used when the turn ends or the
    /// connection goes away (§7.2).
    pub(crate) fn abandon(&self) {
        for request in self.take() {
            request.abandoned();
        }
    }

    /// Gives up on everything except what `outlives` keeps pending — e.g. an
    /// elicitation scoped to a JSON-RPC call outlives the turn (§7.8).
    pub(crate) fn abandon_unless(&self, outlives: impl Fn(&T) -> bool) {
        // Outside the lock: abandoning wakes timeline subscribers, who may read
        // this list.
        for request in self.take_unless(outlives) {
            request.abandoned();
        }
    }

    /// Takes everything `outlives` does not keep, leaving the rest pending.
    pub(crate) fn take_unless(&self, outlives: impl Fn(&T) -> bool) -> Vec<T> {
        let mut requests = locked(&self.requests);
        let (kept, taken): (Vec<T>, Vec<T>) = std::mem::take(&mut *requests)
            .into_iter()
            .partition(|request| outlives(request));
        *requests = kept;
        taken
    }
}
