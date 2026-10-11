//! Elicitation (`docs/architecture.md` §7.8): an `elicitation/create` becomes a
//! value holding the request and the resolver for it, and the agent waits until
//! somebody answers.
//!
//! Accept, decline and cancel each claim something about a reader, so this
//! client answers only where it showed the reader something; a mode nothing
//! could draw is refused on the wire instead. The one answer sent without a
//! click is the `cancel` a cancelled turn or abandoned session owes (§7.2).
//!
//! The `accept` content is raw JSON rather than the schema's typed `content`,
//! which cannot spell nested objects, nulls or wrongly typed values — traffic
//! this tool needs to be able to send.
//!
//! Completion is a separate fact from the answer: `elicitation/complete` may
//! arrive before anybody answers, or never (it is a MAY).

use std::fmt;
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1;
use serde_json::{Map, Value};

use crate::blocking::{Blocking, RequestId, Resolver};
use crate::call::CallError;
use crate::frame::Frame;
use crate::store::locked;

/// An elicitation the agent is waiting on, and the answer that will release it.
///
/// A handle: clones share one pending elicitation, so answering any clone
/// answers it everywhere.
#[derive(Clone)]
pub struct ElicitationRequest(Arc<Asked>);

struct Asked {
    id: RequestId,
    request: v1::CreateElicitationRequest,
    raw_form: Option<Box<serde_json::value::RawValue>>,
    standing: Mutex<Standing>,
    resolver: Resolver,
}

/// The two facts that change, under one lock because a screen reads them
/// together.
#[derive(Clone, Debug)]
struct Standing {
    state: ElicitationState,
    completed: bool,
}

/// Where an elicitation stands. The controls are live only in
/// [`Waiting`](Self::Waiting).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ElicitationState {
    /// The agent is waiting for an answer. The only state one can be answered
    /// in.
    Waiting,
    /// An answer has been chosen and is on its way to the agent.
    Answering,
    /// The agent was told, and this is what it was told.
    Answered(ElicitationAnswer),
    /// Nobody answered it and nobody can now: the turn it blocked ended, or the
    /// agent went away holding it. The agent was told nothing.
    Abandoned,
    /// The agent withdrew it with `$/cancel_request` and was answered with the
    /// cancellation error.
    CancelledByAgent,
}

/// The answer as it went on the wire, kept so a screen can show what was sent.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ElicitationAnswer {
    /// Filled in, or consented to. The content is whatever was sent, which is
    /// not promised to match the requested schema (§7.8).
    Accepted(Option<Map<String, Value>>),
    /// Explicitly declined.
    Declined,
    /// Dismissed without choosing, including by a turn this client cancelled.
    Cancelled,
}

impl ElicitationRequest {
    pub(crate) fn new(
        id: RequestId,
        request: v1::CreateElicitationRequest,
        frame: &Frame,
        resolver: Resolver,
    ) -> Self {
        // Read the schema from the Frame, independently of the canonical typed
        // copy. RawValue retains every token, including numeric spellings.
        let raw_form = (|| {
            type Object<'a> = std::collections::HashMap<String, &'a serde_json::value::RawValue>;
            let envelope: Object<'_> = serde_json::from_str(frame.as_str()).ok()?;
            let params: Object<'_> = serde_json::from_str(envelope.get("params")?.get()).ok()?;
            Some((*params.get("requestedSchema")?).to_owned())
        })();
        Self(Arc::new(Asked {
            id,
            request,
            raw_form,
            standing: Mutex::new(Standing {
                state: ElicitationState::Waiting,
                completed: false,
            }),
            resolver,
        }))
    }

    /// The id the agent asked under, which the answer will be addressed to.
    pub fn id(&self) -> &RequestId {
        &self.0.id
    }

    /// The request as the agent sent it, decoded as v1.
    pub fn request(&self) -> &v1::CreateElicitationRequest {
        &self.0.request
    }

    /// What the agent said it needs, in its own words.
    pub fn message(&self) -> &str {
        &self.0.request.message
    }

    /// The mode and its own fields: a form's schema, or a URL and its id.
    pub fn mode(&self) -> &v1::ElicitationMode {
        &self.0.request.mode
    }

    /// The schema to build a form from, where this is a form elicitation.
    pub fn form(&self) -> Option<&v1::ElicitationSchema> {
        match self.mode() {
            v1::ElicitationMode::Form(form) => Some(&form.requested_schema),
            _ => None,
        }
    }

    /// The form's requestedSchema tokens exactly as they crossed the wire.
    /// This renderer input never passes through the typed numeric seam.
    pub fn raw_form(&self) -> Option<&str> {
        self.form()?;
        self.0.raw_form.as_ref().map(|schema| schema.get())
    }

    /// The URL to hand to the reader's own browser, where this is a URL
    /// elicitation. Shown in full and opened only on a click (§7.8).
    pub fn url(&self) -> Option<&str> {
        match self.mode() {
            v1::ElicitationMode::Url(url) => Some(&url.url),
            _ => None,
        }
    }

    /// The agent's opaque id for a URL elicitation: what an
    /// `elicitation/complete` names, and what another outstanding elicitation
    /// must not reuse.
    pub fn elicitation_id(&self) -> Option<&v1::ElicitationId> {
        match self.mode() {
            v1::ElicitationMode::Url(url) => Some(&url.elicitation_id),
            _ => None,
        }
    }

    /// What the elicitation is tied to: a session, or a single request outside
    /// any session.
    pub fn scope(&self) -> &v1::ElicitationScope {
        self.0.request.scope()
    }

    /// The session it is about, where it is about one. Nothing filters on it.
    pub fn session_id(&self) -> Option<&v1::SessionId> {
        match self.scope() {
            v1::ElicitationScope::Session(session) => Some(&session.session_id),
            _ => None,
        }
    }

    /// The id of the tool call it was raised inside, where the agent named one.
    pub fn tool_call_id(&self) -> Option<&v1::ToolCallId> {
        match self.scope() {
            v1::ElicitationScope::Session(session) => session.tool_call_id.as_ref(),
            _ => None,
        }
    }

    /// The agent's id of the JSON-RPC request it is tied to, where it is scoped
    /// to a request rather than a session.
    pub fn request_scope(&self) -> Option<&v1::RequestId> {
        match self.scope() {
            v1::ElicitationScope::Request(request) => Some(&request.request_id),
            _ => None,
        }
    }

    /// Where it stands: waiting, being answered, answered, or abandoned.
    pub fn state(&self) -> ElicitationState {
        self.held().state.clone()
    }

    /// Whether the agent is still waiting for an answer.
    pub fn is_waiting(&self) -> bool {
        matches!(self.held().state, ElicitationState::Waiting)
    }

    /// Whether the agent has said the out-of-band interaction finished.
    /// Independent of the answer in both directions.
    pub fn completed(&self) -> bool {
        self.held().completed
    }

    /// Answers `accept` with whatever content the reader produced.
    ///
    /// The content is deliberately not checked against the requested schema
    /// (§7.8), so non-conforming answers can be sent on purpose.
    pub async fn accept(&self, content: Option<Map<String, Value>>) -> Result<(), CallError> {
        self.answer_with(ElicitationAnswer::Accepted(content)).await
    }

    /// The reader said no.
    pub async fn decline(&self) -> Result<(), CallError> {
        self.answer_with(ElicitationAnswer::Declined).await
    }

    /// The reader dismissed it without choosing; also what a cancelled turn or
    /// abandoned session answers every elicitation it leaves waiting.
    pub async fn cancel(&self) -> Result<(), CallError> {
        self.answer_with(ElicitationAnswer::Cancelled).await
    }

    /// The agent said the out-of-band half finished (`elicitation/complete`).
    /// Crate-private because only the agent can say so. Leaves the answer alone.
    pub(crate) fn complete(&self) {
        {
            let mut standing = self.held();
            if standing.completed {
                return;
            }
            standing.completed = true;
        }
        self.moved();
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
        self.held().state = match &told {
            Ok(()) => ElicitationState::CancelledByAgent,
            Err(_) => ElicitationState::Abandoned,
        };
        self.moved();
        told
    }

    /// Nobody answered it and nobody can now. Leaves an answer already on its
    /// way alone.
    pub(crate) fn abandoned(&self) {
        {
            let mut standing = self.held();
            if !matches!(standing.state, ElicitationState::Waiting) {
                return;
            }
            standing.state = ElicitationState::Abandoned;
        }
        self.moved();
    }

    /// Answers the request, once.
    ///
    /// Claimed before the send and settled after it, so two racing answers
    /// produce one frame; a failed send leaves it abandoned, not waiting.
    async fn answer_with(&self, answer: ElicitationAnswer) -> Result<(), CallError> {
        self.claim()?;

        let told = self
            .0
            .resolver
            .respond_to(&self.0.id, encode_answer(&answer))
            .await;

        self.held().state = match &told {
            Ok(()) => ElicitationState::Answered(answer),
            Err(_) => ElicitationState::Abandoned,
        };
        self.moved();
        told
    }

    /// Takes the right to answer, or says that it is not there to be taken.
    fn claim(&self) -> Result<(), CallError> {
        let mut standing = self.held();
        match standing.state {
            ElicitationState::Waiting => {
                standing.state = ElicitationState::Answering;
                Ok(())
            }
            _ => Err(CallError::NotWaiting),
        }
    }

    /// Wakes whoever watches the timeline holding this entry.
    fn moved(&self) {
        self.0.resolver.changed();
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Standing> {
        locked(&self.0.standing)
    }
}

/// The response body, built as JSON rather than the schema's typed response.
///
/// Shapes: `{"action":"accept","content":{…}}`, `{"action":"decline"}` and
/// `{"action":"cancel"}`. Absent content is omitted rather than sent as `null`.
fn encode_answer(answer: &ElicitationAnswer) -> Value {
    let mut body = Map::new();
    match answer {
        ElicitationAnswer::Accepted(content) => {
            body.insert("action".to_owned(), Value::from("accept"));
            if let Some(content) = content {
                body.insert("content".to_owned(), Value::Object(content.clone()));
            }
        }
        ElicitationAnswer::Declined => {
            body.insert("action".to_owned(), Value::from("decline"));
        }
        ElicitationAnswer::Cancelled => {
            body.insert("action".to_owned(), Value::from("cancel"));
        }
    }
    Value::Object(body)
}

impl fmt::Debug for ElicitationRequest {
    /// The request and where it stands, without the resolver.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let standing = self.held();
        f.debug_struct("ElicitationRequest")
            .field("id", &self.0.id)
            .field("request", &self.0.request)
            .field("state", &standing.state)
            .field("completed", &standing.completed)
            .finish()
    }
}

/// Identity, not contents: two handles to the same pending elicitation.
impl PartialEq for ElicitationRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Blocking for ElicitationRequest {
    fn is_waiting(&self) -> bool {
        Self::is_waiting(self)
    }

    fn abandoned(&self) {
        Self::abandoned(self);
    }
}

/// The URL elicitations this connection was sent that nobody has said are
/// finished (§7.8).
///
/// Not the pending list: an accepted URL elicitation leaves that but is still
/// what an `elicitation/complete` matches. An id already here when another
/// arrives is the reuse the specification forbids among outstanding
/// elicitations.
#[derive(Clone, Default)]
pub(crate) struct Outstanding {
    urls: Arc<Mutex<Vec<Held>>>,
}

/// One outstanding URL elicitation, with the frame it arrived in as evidence
/// should a later one reuse its id.
struct Held {
    id: v1::ElicitationId,
    request: ElicitationRequest,
    frame: Frame,
}

impl Outstanding {
    /// Records a URL elicitation, and returns the frame of an outstanding one
    /// that already held its id.
    ///
    /// The duplicate is kept rather than replaced: both are real requests the
    /// agent is waiting on.
    pub(crate) fn arrived(
        &self,
        id: v1::ElicitationId,
        request: ElicitationRequest,
        frame: Frame,
    ) -> Option<Frame> {
        let mut urls = self.swept();
        let reused = urls
            .iter()
            .find(|held| held.id == id)
            .map(|held| held.frame.clone());
        urls.push(Held { id, request, frame });
        reused
    }

    /// Marks every elicitation the completion names as completed, and returns
    /// them. All of them, since a duplicate id is already the agent's fault;
    /// empty means the completion was unsolicited.
    pub(crate) fn completed(&self, id: &v1::ElicitationId) -> Vec<ElicitationRequest> {
        let mut urls = self.swept();
        let mut matched = Vec::new();
        urls.retain(|held| {
            if &held.id == id {
                held.request.complete();
                matched.push(held.request.clone());
                false
            } else {
                true
            }
        });
        matched
    }

    /// Forgets everything: ids are outstanding only on the connection that
    /// minted them.
    pub(crate) fn clear(&self) {
        locked(&self.urls).clear();
    }

    /// The list, with anything no longer outstanding dropped, so a freed id
    /// does not block its own reuse.
    fn swept(&self) -> std::sync::MutexGuard<'_, Vec<Held>> {
        let mut urls = locked(&self.urls);
        urls.retain(|held| outstanding(&held.request));
        urls
    }
}

/// Whether a URL elicitation is still outstanding, in the specification's
/// sense.
///
/// A decline or cancel ends it, so the agent may reuse the id without being
/// annotated for it (`docs/architecture.md` §15 q7). Consent does not: only
/// `elicitation/complete` ends an accepted one. An abandoned one can never be
/// completed, so it ends too.
fn outstanding(request: &ElicitationRequest) -> bool {
    !matches!(
        request.state(),
        ElicitationState::Abandoned
            | ElicitationState::Answered(
                ElicitationAnswer::Declined | ElicitationAnswer::Cancelled
            )
    )
}
