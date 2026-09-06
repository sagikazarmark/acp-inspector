//! The permission request an agent makes (`docs/architecture.md` §5, §7.2): a
//! `session/request_permission` becomes a value holding the request *and the
//! resolver for it*, and the turn stays where it is until somebody answers.
//!
//! The request is an entry where the turn stopped, it renders the tool call
//! under question and the options the agent offered, and answering it is what
//! lets the turn go on. What that has in common with the other blocking request
//! this client services — the id, the wire, the list of the ones still waiting —
//! is [`crate::blocking`]; what is here is what a permission request *is*: one
//! option id out of the set an agent offered, or the admission that nobody
//! chose one.

use std::fmt;
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1;

use crate::blocking::{Blocking, RequestId, Resolver};
use crate::call::CallError;
use crate::client::encode;
use crate::store::locked;

/// A permission request the agent is waiting on, and the answer that will
/// release it.
///
/// A handle, not the request: cloning shares one pending request, so the entry
/// on the timeline, the list of what is still waiting, and whatever the screen
/// is holding are all the same request — and answering any of them answers it
/// everywhere.
#[derive(Clone)]
pub struct PermissionRequest(Arc<Asked>);

struct Asked {
    id: RequestId,
    request: v1::RequestPermissionRequest,
    state: Mutex<PermissionState>,
    resolver: Resolver,
}

/// Where a blocking request stands.
///
/// **Whether the agent is still waiting is core's answer, not a screen's.** The
/// buttons in a panel are live in exactly one of these states, and every other
/// one of them is a sentence to say instead — so a surface that decided for
/// itself, out of a turn state and a connection status, would be answering a
/// question this already answers and drifting from it the moment it got one of
/// them wrong.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PermissionState {
    /// The agent is waiting for an answer. The only state a request can be
    /// answered in.
    Waiting,
    /// An answer has been chosen and is on its way to the agent.
    ///
    /// Its own state rather than a flag, because it is what makes a second
    /// click harmless: an answer on its way out has already claimed the
    /// request, so nothing else can claim it, and nothing has to be true about
    /// the wire yet.
    Answering,
    /// The agent was told, and this is what it was told.
    Answered(v1::RequestPermissionOutcome),
    /// Nobody answered it, and nobody can now: the turn it blocked ended, or
    /// the agent went away holding it.
    ///
    /// Not an answer and never rendered as one — the agent was told nothing.
    /// It exists because the alternative is a request that reads as still
    /// waiting forever, under a row of buttons that would send an answer to a
    /// turn that is over.
    Abandoned,
}

impl PermissionRequest {
    pub(crate) fn new(
        id: RequestId,
        request: v1::RequestPermissionRequest,
        resolver: Resolver,
    ) -> Self {
        Self(Arc::new(Asked {
            id,
            request,
            state: Mutex::new(PermissionState::Waiting),
            resolver,
        }))
    }

    /// The id the agent asked under, which the answer will be addressed to.
    pub fn id(&self) -> &RequestId {
        &self.0.id
    }

    /// The request as the agent sent it, decoded as v1.
    pub fn request(&self) -> &v1::RequestPermissionRequest {
        &self.0.request
    }

    /// The session the request is about. Its own accessor because a request
    /// arriving for a session other than the open one is a thing an agent can
    /// do, and a screen may want to say so.
    pub fn session_id(&self) -> &v1::SessionId {
        &self.0.request.session_id
    }

    /// The tool call under question — what the panel is asking about (§7.2).
    pub fn tool_call(&self) -> &v1::ToolCallUpdate {
        &self.0.request.tool_call
    }

    /// The options the agent offered, in the order it offered them.
    ///
    /// The agent's own, unchanged and unsorted: which options exist, what they
    /// are called and what kind each is are the agent's statement, and a client
    /// that added a fifth or reordered the four would be answering a question
    /// nobody asked.
    pub fn options(&self) -> &[v1::PermissionOption] {
        &self.0.request.options
    }

    /// Where the request stands: waiting, being answered, answered, or
    /// abandoned.
    pub fn state(&self) -> PermissionState {
        self.held().clone()
    }

    /// Whether the agent is still waiting for an answer to this — the one
    /// question a screen asks of the state above, and the condition the buttons
    /// are live under.
    ///
    /// Read across the pending requests
    /// ([`Inspector::pending_permissions`](crate::Inspector::pending_permissions)),
    /// it is also what "the agent is waiting on you" is derived from. It is
    /// deliberately not a turn state: a turn blocked on a request is still the
    /// turn that was running.
    pub fn is_waiting(&self) -> bool {
        matches!(*self.held(), PermissionState::Waiting)
    }

    /// The user picked an option: `selected` with that option's id, on the wire
    /// (§7.2).
    ///
    /// The id is sent exactly as the agent offered it. Nothing here checks that
    /// it is one of the offered ones — a screen renders the options and can
    /// only click one of them, and a core that second-guessed the id would be
    /// standing between a caller and an agent that is the authority on its own
    /// options.
    pub async fn select(&self, option: &v1::PermissionOptionId) -> Result<(), CallError> {
        self.answer_with(v1::RequestPermissionOutcome::Selected(
            v1::SelectedPermissionOutcome::new(option.clone()),
        ))
        .await
    }

    /// The turn was cancelled out from under it (§7.2).
    ///
    /// Not public: cancelling a turn is what produces this, and a surface that
    /// could answer `cancelled` without cancelling anything would be telling
    /// the agent something that is not so.
    pub(crate) async fn cancelled(&self) -> Result<(), CallError> {
        self.answer_with(v1::RequestPermissionOutcome::Cancelled)
            .await
    }

    /// Nobody answered it and nobody can now: the turn it blocked ended, or the
    /// agent went away holding it.
    ///
    /// Leaves an answer that is already on its way alone — that one is being
    /// decided by the wire, and this is not evidence about it.
    pub(crate) fn abandoned(&self) {
        {
            let mut state = self.held();
            if !matches!(*state, PermissionState::Waiting) {
                return;
            }
            *state = PermissionState::Abandoned;
        }
        self.moved();
    }

    /// Answers the request, once.
    ///
    /// Claimed before the send and settled after it, so that two answers racing
    /// each other produce one frame. The agent is the one thing that decides
    /// whether it heard, and the wire is the only evidence of it — so a send
    /// that failed is a request nobody will ever answer rather than one still
    /// waiting to be, because the only way one fails is the connection being
    /// gone.
    async fn answer_with(&self, outcome: v1::RequestPermissionOutcome) -> Result<(), CallError> {
        self.claim()?;

        let told = self
            .0
            .resolver
            .respond_to(
                &self.0.id,
                encode(&v1::RequestPermissionResponse::new(outcome.clone())),
            )
            .await;

        *self.held() = match &told {
            Ok(()) => PermissionState::Answered(outcome),
            Err(_) => PermissionState::Abandoned,
        };
        self.moved();
        told
    }

    /// Takes the right to answer, or says that it is not there to be taken.
    fn claim(&self) -> Result<(), CallError> {
        let mut state = self.held();
        match *state {
            PermissionState::Waiting => {
                *state = PermissionState::Answering;
                Ok(())
            }
            _ => Err(CallError::NotWaiting),
        }
    }

    /// Says that this request changed.
    ///
    /// The entry showing it is already on the timeline, holding this very
    /// value: what changed is inside it, so what a subscriber needs is to be
    /// told to look again.
    fn moved(&self) {
        self.0.resolver.changed();
    }

    fn held(&self) -> std::sync::MutexGuard<'_, PermissionState> {
        locked(&self.0.state)
    }
}

impl fmt::Debug for PermissionRequest {
    /// The request and where it stands — never the resolver, which is a pipe
    /// and prints as nothing useful in a failed assertion.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PermissionRequest")
            .field("id", &self.0.id)
            .field("request", &self.0.request)
            .field("state", &*self.held())
            .finish()
    }
}

/// Two handles to the same pending request.
///
/// Identity, not contents: what a request *is* does not change, and what
/// changes about it — the answer — changes for every holder at once, so
/// comparing two of them field by field could only ever agree with itself.
impl PartialEq for PermissionRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// What the pending list needs of it, which is the same two things it needs of
/// an elicitation.
impl Blocking for PermissionRequest {
    fn is_waiting(&self) -> bool {
        Self::is_waiting(self)
    }

    fn abandoned(&self) {
        Self::abandoned(self);
    }
}
