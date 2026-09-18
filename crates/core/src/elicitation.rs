//! The elicitation an agent makes (`docs/architecture.md` §7.8): an
//! `elicitation/create` becomes a value holding the request *and the resolver
//! for it*, and whatever it was asked inside waits until somebody answers.
//!
//! **Three answers, and every one of them is a claim about a reader.** *Accept*
//! says somebody filled a form or consented to a URL, *decline* says they said
//! no, and *cancel* says they dismissed it without choosing. That is why this
//! client answers only where it showed the reader something: a mode nothing
//! could draw is refused on the wire (§7.8) rather than declined on their
//! behalf, and the one place an answer is sent without a click is the debt a
//! cancelled turn or an abandoned session already owes (§7.2).
//!
//! **The content is raw, deliberately.** `accept` carries JSON rather than the
//! schema crate's typed `content`, whose value set has no spelling for a nested
//! object, a null, or a value of the wrong type — the traffic this tool exists
//! to be able to send. Decoding stays strictly typed (§11 seam 1); encoding this
//! client's own answer is where the raw-first rule applies to a frame it writes.
//!
//! **Completion is not an answer and does not share its state.** A URL
//! elicitation is answered when the reader consents and *completed* when the
//! agent says the out-of-band half finished, and neither implies the other: the
//! notification may arrive before anybody has clicked, and may never arrive at
//! all, since the specification makes it a MAY. So it is a second fact about the
//! same request rather than a state after the answer.

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
/// A handle, not the request: cloning shares one pending elicitation, so the
/// entry on the timeline, the list of what is still waiting, the outstanding
/// ids and whatever the screen is holding are all the same request — and
/// answering any of them answers it everywhere.
#[derive(Clone)]
pub struct ElicitationRequest(Arc<Asked>);

struct Asked {
    id: RequestId,
    request: v1::CreateElicitationRequest,
    raw_form: Option<Box<serde_json::value::RawValue>>,
    standing: Mutex<Standing>,
    resolver: Resolver,
}

/// The two facts about a pending elicitation that change, kept under one lock
/// because a screen reads them together.
#[derive(Clone, Debug)]
struct Standing {
    state: ElicitationState,
    completed: bool,
}

/// Where an elicitation stands.
///
/// **Whether the agent is still waiting is core's answer, not a screen's** — the
/// rule the permission panel already follows: the controls are live in exactly
/// one of these states, and every other one is a sentence to say instead.
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
    /// Nobody answered it, and nobody can now: the turn it blocked ended, or the
    /// agent went away holding it.
    ///
    /// Not an answer and never rendered as one — the agent was told nothing.
    Abandoned,
}

/// What the reader did about it, as it went on the wire.
///
/// The content is kept because a screen shows what was sent: an accepted form
/// whose answer is no longer on screen is a record of an answer, and reading it
/// back off the trace would be the surface asking the wire what it just said.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ElicitationAnswer {
    /// Filled in, or consented to. The content is whatever was sent, which is
    /// not promised to match the requested schema (§7.8).
    Accepted(Option<Map<String, Value>>),
    /// Explicitly declined.
    Declined,
    /// Dismissed without choosing — including by a turn this client cancelled,
    /// which owes every request it left waiting exactly this.
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

    /// What the agent said it needs, in its own words. The specification asks a
    /// client to present it, and this is the whole of what an unknown mode would
    /// still have had to show.
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

    /// The agent's id for a URL elicitation — what an `elicitation/complete`
    /// will name, and what a second outstanding elicitation must not reuse.
    ///
    /// Opaque: the specification says a client treats it as such, so nothing
    /// here reads it for meaning.
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

    /// The session it is about, where it is about one.
    ///
    /// Its own accessor for the reason the permission request has one: an
    /// elicitation arriving for a session other than the open one is a thing an
    /// agent can do, and a screen may want to say so. Nothing here filters on
    /// it.
    pub fn session_id(&self) -> Option<&v1::SessionId> {
        match self.scope() {
            v1::ElicitationScope::Session(session) => Some(&session.session_id),
            _ => None,
        }
    }

    /// The tool call it was raised inside, where the agent named one.
    ///
    /// An id and never the call: the scope carries this and nothing else, so a
    /// screen links to the call rather than drawing an update it was never sent.
    pub fn tool_call_id(&self) -> Option<&v1::ToolCallId> {
        match self.scope() {
            v1::ElicitationScope::Session(session) => session.tool_call_id.as_ref(),
            _ => None,
        }
    }

    /// The JSON-RPC request it is tied to, where it is tied to one rather than
    /// to a session — the agent's own id for a call this client may never have
    /// seen.
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

    /// Whether the agent is still waiting for an answer — the condition the
    /// controls are live under, and half of "the agent is waiting on you".
    pub fn is_waiting(&self) -> bool {
        matches!(self.held().state, ElicitationState::Waiting)
    }

    /// Whether the agent has said the out-of-band interaction finished.
    ///
    /// Independent of the answer, in both directions: a completed elicitation
    /// may still be waiting for one, and an accepted one may never be completed
    /// at all, because `elicitation/complete` is a MAY.
    pub fn completed(&self) -> bool {
        self.held().completed
    }

    /// The reader filled it in, or consented to the URL: `accept`, with whatever
    /// content they produced.
    ///
    /// **The content is not checked against the requested schema, on purpose**
    /// (§7.8). A client that could only send conforming answers could not find
    /// out what an agent does with one that does not conform, which is a thing
    /// somebody came here to find out. The map is the one shape the method has —
    /// an object or nothing — and that much is a type here rather than a rule.
    pub async fn accept(&self, content: Option<Map<String, Value>>) -> Result<(), CallError> {
        self.answer_with(ElicitationAnswer::Accepted(content)).await
    }

    /// The reader said no.
    pub async fn decline(&self) -> Result<(), CallError> {
        self.answer_with(ElicitationAnswer::Declined).await
    }

    /// The reader dismissed it without choosing — and the answer a cancelled
    /// turn or an abandoned session owes every elicitation it leaves waiting
    /// (§7.2, §7.8).
    pub async fn cancel(&self) -> Result<(), CallError> {
        self.answer_with(ElicitationAnswer::Cancelled).await
    }

    /// The agent said the out-of-band half finished (`elicitation/complete`).
    ///
    /// Not public: this is the agent's statement, arriving as a notification,
    /// and a surface that could make it would be putting words in the agent's
    /// mouth. Leaves the answer alone, whatever it is — including *none yet*.
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

    /// Nobody answered it and nobody can now: the turn it blocked ended, or the
    /// agent went away holding it.
    ///
    /// Leaves an answer that is already on its way alone — that one is being
    /// decided by the wire, and this is not evidence about it.
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
    /// Claimed before the send and settled after it, so that two answers racing
    /// each other produce one frame — the permission request's rule, for its
    /// reason: the agent is the one thing that decides whether it heard, and a
    /// send that failed is a request nobody will ever answer rather than one
    /// still waiting to be.
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

    /// Says that this request changed, to whoever is watching the timeline the
    /// entry holding it sits on.
    fn moved(&self) {
        self.0.resolver.changed();
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Standing> {
        locked(&self.0.standing)
    }
}

/// The response body, built as JSON rather than through the schema crate's
/// typed response (§7.8).
///
/// `action` is flattened onto the response, so the shapes are
/// `{"action":"accept","content":{…}}`, `{"action":"decline"}` and
/// `{"action":"cancel"}`. Content is omitted rather than sent as `null` when
/// there is none: the field is optional, and the specification says a URL
/// acceptance normally carries no content at all.
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
    /// The request and where it stands — never the resolver, which is a pipe and
    /// prints as nothing useful in a failed assertion.
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

/// Two handles to the same pending elicitation.
///
/// Identity, not contents, for the permission request's reason: what a request
/// *is* does not change, and what changes about it changes for every holder at
/// once.
impl PartialEq for ElicitationRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// What the pending list needs of it, which is the same two things it needs of
/// a permission request.
impl Blocking for ElicitationRequest {
    fn is_waiting(&self) -> bool {
        Self::is_waiting(self)
    }

    fn abandoned(&self) {
        Self::abandoned(self);
    }
}

/// The URL elicitations this connection has been sent and nobody has said are
/// finished (§7.8).
///
/// **Not the pending list.** That one holds what is still *unanswered*, and an
/// accepted URL elicitation has left it while remaining exactly the thing an
/// `elicitation/complete` is about — so this is what a completion is matched
/// against, and it is per connection because the ids are.
///
/// It is also what decides the one annotation this ring adds: an id already in
/// here when another arrives is an agent reusing an id among *outstanding*
/// elicitations, which is the MUST the specification states.
#[derive(Clone, Default)]
pub(crate) struct Outstanding {
    urls: Arc<Mutex<Vec<Held>>>,
}

/// One outstanding URL elicitation: the id the agent minted, the request it is
/// about, and the frame it arrived in — which is the other half of the evidence
/// when a second one reuses the id.
struct Held {
    id: v1::ElicitationId,
    request: ElicitationRequest,
    frame: Frame,
}

impl Outstanding {
    /// Records a URL elicitation, and hands back the frame of the one that
    /// already held its id, where there was one.
    ///
    /// The duplicate is kept rather than replaced: both are real requests the
    /// agent is waiting on, and dropping one would be this tool tidying away the
    /// evidence for the annotation it just made. The frame comes back because an
    /// annotation is read beside the frames that decide it (§15 q7), and this
    /// rule takes two — the create that minted the id, and the create that
    /// reused it.
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

    /// Marks every elicitation the completion names, and hands them back.
    ///
    /// **Every one, not the first.** Two entries under one id is only reachable
    /// once the agent has already broken the uniqueness MUST, and choosing
    /// between them would be this tool guessing where it has just reported that
    /// it cannot know. An empty answer is what makes a completion nobody asked
    /// for unsolicited traffic (§8).
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

    /// Forgets everything: the connection that issued these ids is gone, and an
    /// id is only ever outstanding on the connection it was minted for.
    pub(crate) fn clear(&self) {
        locked(&self.urls).clear();
    }

    /// The list, with everything that has stopped being outstanding dropped on
    /// the way past — so an id the agent is free to use again is not standing in
    /// the way of its own reuse.
    fn swept(&self) -> std::sync::MutexGuard<'_, Vec<Held>> {
        let mut urls = locked(&self.urls);
        urls.retain(|held| outstanding(&held.request));
        urls
    }
}

/// Whether a URL elicitation is still *outstanding*, which is the rule's own
/// word and not a synonym for unanswered.
///
/// **A refusal ends it.** Declining or cancelling is the interaction not
/// happening, so the agent may mint the id again — and a client that went on
/// holding it would annotate that agent for breaking a MUST it did not break.
/// An annotation this tool cannot stand behind is worse than one it never
/// makes ([§15](../../../docs/architecture.md) q7).
///
/// **Consent does not.** An accepted URL elicitation is one whose interaction
/// may be running right now, somewhere this tool cannot see; only the agent's
/// own `elicitation/complete` ends that, which is the difference the panel
/// draws and the reason the two facts are kept apart at all.
///
/// And a request nobody can answer any more was never going to be completed,
/// so it goes with them.
fn outstanding(request: &ElicitationRequest) -> bool {
    !matches!(
        request.state(),
        ElicitationState::Abandoned
            | ElicitationState::Answered(
                ElicitationAnswer::Declined | ElicitationAnswer::Cancelled
            )
    )
}
