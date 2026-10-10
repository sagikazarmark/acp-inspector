//! The permission request an agent makes (`docs/architecture.md` §7.2): a
//! `session/request_permission` becomes a value holding the request and its
//! resolver, and the turn waits until somebody answers.
//!
//! What it shares with the other blocking requests is in [`crate::blocking`].

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
/// A handle: clones share one pending request, so answering any clone answers
/// it everywhere.
#[derive(Clone)]
pub struct PermissionRequest(Arc<Asked>);

struct Asked {
    id: RequestId,
    request: v1::RequestPermissionRequest,
    state: Mutex<PermissionState>,
    resolver: Resolver,
    /// The policy that answered it, when one did.
    policy: Mutex<Option<PermissionPolicy>>,
}

/// How `session/request_permission` is answered in the open session.
///
/// Session-scoped: it goes back to [`Ask`](Self::Ask) when the session changes
/// or the agent disconnects. Never a standing grant: the automatic answers pick
/// only an `allow_once` or `reject_once` option, and a request offering neither
/// waits for the reader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionPolicy {
    /// Every request waits for the reader.
    #[default]
    Ask,
    /// Pick the first `allow_once` option.
    AllowOnce,
    /// Pick the first `reject_once` option.
    RejectOnce,
}

impl PermissionPolicy {
    /// The option this policy answers with, out of what the agent offered.
    ///
    /// `None` for [`Ask`](Self::Ask), and when no option of the policy's kind is
    /// offered. An `*_always` option is never chosen automatically.
    pub fn pick(self, options: &[v1::PermissionOption]) -> Option<&v1::PermissionOption> {
        let kind = match self {
            Self::Ask => return None,
            Self::AllowOnce => v1::PermissionOptionKind::AllowOnce,
            Self::RejectOnce => v1::PermissionOptionKind::RejectOnce,
        };
        options.iter().find(|option| option.kind == kind)
    }
}

/// Where a blocking request stands. Screens read whether the agent is still
/// waiting from this rather than deriving it themselves.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PermissionState {
    /// The agent is waiting for an answer. The only state a request can be
    /// answered in.
    Waiting,
    /// An answer has been chosen and is on its way to the agent. Claiming the
    /// request here makes a second click harmless.
    Answering,
    /// The agent was told, and this is what it was told.
    Answered(v1::RequestPermissionOutcome),
    /// Nobody answered it, and nobody can now: the turn it blocked ended, or
    /// the agent went away holding it. The agent was told nothing.
    Abandoned,
    /// The agent withdrew it with `$/cancel_request` and was answered with the
    /// cancellation error.
    CancelledByAgent,
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
            policy: Mutex::new(None),
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

    /// The session the request is about, which may not be the open one.
    pub fn session_id(&self) -> &v1::SessionId {
        &self.0.request.session_id
    }

    /// The tool call the request is about.
    pub fn tool_call(&self) -> &v1::ToolCallUpdate {
        &self.0.request.tool_call
    }

    /// The options the agent offered, unchanged and in the order offered.
    pub fn options(&self) -> &[v1::PermissionOption] {
        &self.0.request.options
    }

    /// Where the request stands: waiting, being answered, answered, or
    /// abandoned.
    pub fn state(&self) -> PermissionState {
        self.held().clone()
    }

    /// Whether the agent is still waiting for an answer; the condition the
    /// buttons are live under. Not a turn state: a blocked turn is still running.
    pub fn is_waiting(&self) -> bool {
        matches!(*self.held(), PermissionState::Waiting)
    }

    /// Answers `selected` with `option`, sent exactly as given. Nothing checks
    /// it is one of the offered ids: the agent is the authority on its options.
    pub async fn select(&self, option: &v1::PermissionOptionId) -> Result<(), CallError> {
        self.answer_with(v1::RequestPermissionOutcome::Selected(
            v1::SelectedPermissionOutcome::new(option.clone()),
        ))
        .await
    }

    /// The policy that answered this request, if one did rather than the
    /// reader.
    pub fn by_policy(&self) -> Option<PermissionPolicy> {
        *locked(&self.0.policy)
    }

    /// Answers with the option `policy` picks, if it picks one. Says whether
    /// it did; a request it leaves alone is still waiting for the reader.
    pub(crate) async fn answer_by_policy(&self, policy: PermissionPolicy) -> bool {
        let Some(option) = policy
            .pick(self.options())
            .map(|option| option.option_id.clone())
        else {
            return false;
        };
        // Recorded before the answer, so the card never shows a policy's
        // answer as the reader's.
        *locked(&self.0.policy) = Some(policy);
        if self.select(&option).await.is_err() {
            *locked(&self.0.policy) = None;
        }
        true
    }

    /// Answers `cancelled` because the turn was cancelled (§7.2). Crate-only:
    /// only cancelling a turn may produce this answer.
    pub(crate) async fn cancelled(&self) -> Result<(), CallError> {
        self.answer_with(v1::RequestPermissionOutcome::Cancelled)
            .await
    }

    /// The agent withdrew the request with `$/cancel_request`, and is answered
    /// with the cancellation error the protocol asks for.
    pub(crate) async fn withdrawn(&self) -> Result<(), CallError> {
        self.claim()?;
        let told = self
            .0
            .resolver
            .refuse(&self.0.id, v1::Error::request_cancelled())
            .await;
        *self.held() = match &told {
            Ok(()) => PermissionState::CancelledByAgent,
            Err(_) => PermissionState::Abandoned,
        };
        self.moved();
        told
    }

    /// Marks a waiting request abandoned. An answer already on its way is left
    /// for the wire to decide.
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
    /// Claimed before the send and settled after it, so two racing answers
    /// produce one frame. A failed send means the connection is gone, so the
    /// request becomes abandoned rather than waiting again.
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

    /// Tells timeline subscribers to look again; the entry holds this value.
    fn moved(&self) {
        self.0.resolver.changed();
    }

    fn held(&self) -> std::sync::MutexGuard<'_, PermissionState> {
        locked(&self.0.state)
    }
}

impl fmt::Debug for PermissionRequest {
    /// The request and where it stands, without the resolver.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PermissionRequest")
            .field("id", &self.0.id)
            .field("request", &self.0.request)
            .field("state", &*self.held())
            .finish()
    }
}

/// Two handles to the same pending request (identity, not contents).
impl PartialEq for PermissionRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Blocking for PermissionRequest {
    fn is_waiting(&self) -> bool {
        Self::is_waiting(self)
    }

    fn abandoned(&self) {
        Self::abandoned(self);
    }
}
