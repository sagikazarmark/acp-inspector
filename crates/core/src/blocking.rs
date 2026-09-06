//! What every blocking request has in common (`docs/architecture.md` §7.2,
//! §7.8): the id it arrived under, the way an answer reaches the wire, and the
//! list of the ones still waiting.
//!
//! **A pending request is a thing, not an event.** The MCP Inspector's
//! pending-client-request machinery is the shape (§5), with the presentation
//! moved inline into the timeline, because in ACP these are the main traffic of
//! a turn rather than an exceptional alert: the request is an entry where the
//! turn stopped, and answering it is what lets the turn go on. No modal, no
//! toast, nothing that has to be dismissed before the trace can be read.
//!
//! **The answer belongs to the request, not to whoever is holding it.** So the
//! [`Resolver`] rides along: a screen that has one entry out of a list can
//! answer it without knowing which connection it came from or which id it
//! arrived under, and the two facts that make an answer legal — that the agent
//! is still waiting, and that it gets exactly one answer — are checked by the
//! request rather than at each surface that might click a button.
//!
//! **The plumbing is shared and the requests are not** (§7.8). A permission
//! request and an elicitation are answered from the same wire and given up on at
//! the same four places, which is what [`Pending`] holds for both of them; what
//! each *is* — one option id out of a list the agent offered, or one of three
//! things a reader did with a form — stays its own type, because a state machine
//! with three answers is not the other one bent into shape.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::call::CallError;
use crate::rpc::Rpc;
use crate::store::locked;
use crate::timeline::Ticker;

/// The id a request arrived under, kept exactly as it arrived.
///
/// **Opaque, and handed back rather than read**, like [`Cursor`]: JSON-RPC ids
/// are strings or numbers and an answer that changed one would be an answer to
/// a different question. Testy's are UUID strings, this client's own are
/// numbers, and nothing above core has any business telling those apart.
///
/// [`Cursor`]: crate::Cursor
#[derive(Clone, Debug)]
pub struct RequestId(Value);

impl RequestId {
    pub(crate) fn new(id: Value) -> Self {
        Self(id)
    }

    /// The id, for the one caller allowed to have it: the response that is
    /// addressed to it.
    pub(crate) fn value(&self) -> &Value {
        &self.0
    }
}

impl fmt::Display for RequestId {
    /// The id as JSON: `7`, or `"3a9f…"`. What a screen shows beside a request
    /// is the id the agent used, in the form it used it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for RequestId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// JSON-RPC ids are strings or numbers, and neither shape of them has the
/// float-shaped hole `Value` is missing [`Eq`] for. Two ids that answer each
/// other are equal, which is all this is for.
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
/// Deliberately *not* the timeline itself. An answer changes an entry in place
/// — the entry holds the same request — so what a subscriber needs is the
/// revision moving, and a resolver holding the entries as well would be a cycle
/// between a list and the things in it.
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
    /// The body is JSON rather than a typed response, because one of the two
    /// answers this carries is deliberately raw (§7.8): the reader is allowed to
    /// send content the requested schema forbids, and a typed encoder is exactly
    /// the thing that could not express it.
    pub(crate) async fn respond_to(&self, id: &RequestId, body: Value) -> Result<(), CallError> {
        self.rpc.respond(id.value(), Ok(body)).await
    }

    /// Says that a request changed, to whoever is watching the timeline it sits
    /// on.
    pub(crate) fn changed(&self) {
        self.timeline.tick();
    }
}

/// What [`Pending`] needs to know about the requests in it, and no more.
///
/// Two questions, both about all of them at once: which are still waiting, and
/// what to do with one nobody will ever answer. Answering one — which is a
/// different sentence for each kind, and an `await` — stays on the request's own
/// type, where the caller knows which kind it is holding.
pub(crate) trait Blocking: Clone {
    /// Whether the agent is still waiting for an answer to this one.
    fn is_waiting(&self) -> bool;

    /// Nobody answered it and nobody can now.
    fn abandoned(&self);
}

/// The requests the agent is waiting on, on this connection.
///
/// Not a second place a request lives: the screen *renders* them off the
/// timeline, where they sit at the point the turn blocked, and this is the same
/// requests held as the two questions that are about all of them at once — who
/// a cancel owes an answer to (§7.2), and whether the agent is waiting on
/// anybody at all.
pub(crate) struct Pending<T> {
    requests: Arc<Mutex<Vec<T>>>,
}

impl<T> Clone for Pending<T> {
    /// A handle to one list, whatever is in it. Derived would demand `T: Clone`
    /// for a field that is an `Arc`, which is a bound about the wrong thing.
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

    /// The ones still waiting. Answered requests fall out on the way past, so a
    /// list nobody asks about does not grow with a turn.
    pub(crate) fn waiting(&self) -> Vec<T> {
        let mut requests = locked(&self.requests);
        requests.retain(T::is_waiting);
        requests.clone()
    }

    /// Takes everything, leaving nothing pending — for the one caller that
    /// answers what it takes.
    pub(crate) fn take(&self) -> Vec<T> {
        std::mem::take(&mut *locked(&self.requests))
    }

    /// Gives up on everything still waiting: nobody answered these, and nobody
    /// can now.
    ///
    /// **The rule is one rule** — a blocking request belongs to the turn it
    /// blocked (§7.2) — and this is where it is kept: the turn ending and the
    /// connection going away are the two ways a request stops being answerable
    /// without ever being answered, and both of them come through here rather
    /// than each quietly dropping the list.
    pub(crate) fn abandon(&self) {
        for request in self.take() {
            request.abandoned();
        }
    }

    /// Gives up on everything except what the caller says outlives this, which
    /// stays pending and stays answerable.
    ///
    /// The one caller is a turn ending (§7.8): a blocking request belongs to the
    /// turn it blocked, *unless* nothing ties it to one — an elicitation scoped
    /// to a JSON-RPC call rather than to a session is not the turn's to give up
    /// on, and its entry is still on the screen that would answer it.
    pub(crate) fn abandon_unless(&self, outlives: impl Fn(&T) -> bool) {
        // Outside the lock: abandoning one tells whoever is watching the
        // timeline, and a subscriber woken while this list is held would read it
        // through the lock it is already holding open.
        for request in self.take_unless(outlives) {
            request.abandoned();
        }
    }

    /// Takes everything the caller will answer, leaving what outlives this
    /// pending — the same split as [`abandon_unless`](Self::abandon_unless), for
    /// the caller that sends an answer rather than giving up.
    pub(crate) fn take_unless(&self, outlives: impl Fn(&T) -> bool) -> Vec<T> {
        let mut requests = locked(&self.requests);
        let (kept, taken): (Vec<T>, Vec<T>) = std::mem::take(&mut *requests)
            .into_iter()
            .partition(|request| outlives(request));
        *requests = kept;
        taken
    }
}
