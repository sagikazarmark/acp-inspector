//! The typed ACP client (`docs/architecture.md` §5, §14 step 3): the driven
//! methods on the way out, and one place that reads everything on the way in.
//!
//! **It decorates; it does not replace.** By the time a frame reaches this
//! layer the trace already has it (§6.2), so nothing here can lose a frame,
//! misreport one, or turn one into an error — the worst it can do is fail to
//! recognize one, and a frame it fails to recognize becomes a first-class
//! unrecognized entry carrying its own JSON (§8). That is the raw-first rule as
//! a program: the types are a decoration over a record that is already
//! complete.
//!
//! **It decodes as v1, explicitly** (§11 seam 1). Every type it names comes out
//! of the schema crate's `v1` module, so a v2 decoder is a second one of these
//! rather than an excavation of this one, and v2 traffic arriving today degrades
//! to something visible instead of breaking anything.
//!
//! **The conformant path only** (§7.1, §7.5, §7.6): `initialize`,
//! `session/new`, `session/prompt`, `session/cancel`, `session/list`,
//! `session/load`, `session/resume`, `session/close`, `session/delete`,
//! `session/set_mode`, `session/set_config_option`, `authenticate` and
//! `logout` — the six that open, close or list a session driven from what the
//! agent advertised about *itself*, which is the same `initialize` result the
//! capability display renders, `logout` from the auth block of that same
//! result, and the two setters from what the live session's setup response
//! published, which is an advertisement of a different shape and not a
//! capability at all. What the inspector does not offer, it declines by
//! advertising no capabilities (§7.3) — and answers method-not-found to,
//! loudly, when an agent calls it anyway. The filesystem and terminal services
//! a launch claims, and any other request the reader opts in to, are answered
//! by the reader or by a rules file (F3, `docs/client-services.md`).

mod driving;
mod inbound;
mod lifecycle;
mod requests;
mod rules;
mod settings;
mod turn;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1;
use serde::Serialize;
use serde_json::Value;

use crate::auth::AuthState;
use crate::blocking::Pending;
use crate::call::CallError;
use crate::connection::FrameSink;
use crate::driven::{AgentCapability, DrivenRecord};
use crate::elicitation::{ElicitationRequest, Outstanding};
use crate::frame::Frame;
use crate::listing::SessionListing;
use crate::permission::{PermissionPolicy, PermissionRequest};
use crate::rpc::{PendingCall, Rpc};
use crate::service::ServiceRequest;
use crate::services::{ClientClaim, ClientServices};
use crate::session_settings::SessionSettings;
use crate::store::{Field, locked};
use crate::timeline::Timeline;
use crate::turn::TurnState;

pub(crate) use driving::drives;
use rules::Loading;
use settings::{ChangingMode, Opening, SettingOption};

/// What the agent is told this client is.
///
/// The crate's name and version, which are the provisional ones the whole
/// project carries (§3) — an agent that logs its clients should log something
/// true, and "ACP Inspector" is what this is until the name is settled.
const CLIENT_NAME: &str = "acp-inspector";
const CLIENT_TITLE: &str = "ACP Inspector";

/// Who this client says it is in every `initialize` request.
///
/// Public for the same reason as [`client_capabilities`]: a surface that shows
/// the claim must read the value the wire gets rather than restating it.
#[must_use]
pub fn client_info() -> v1::Implementation {
    v1::Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")).title(CLIENT_TITLE)
}

/// What this client claims about *itself* by default (§7.3, §7.6, §7.8).
///
/// **The boolean config option is a data shape rather than a service.** It is
/// gated by a client capability, so an agent that never hears this must not send
/// one — and a surveyed agent honours that precisely, shipping an option as a
/// boolean to clients that claimed the capability and degrading it to a
/// two-value select for everyone else. Declining it did not spare this tool any
/// work; it changed what the subject offered, by a claim nobody had examined.
/// Claiming it is what makes the inspector observe an agent's real behaviour
/// instead of a lesser one shaped for it.
///
/// **Elicitation is a service, and it is claimed because this tool performs
/// it** (§7.8). Both modes: a form is built from the agent's own schema, and a
/// URL is shown in full and handed to the reader's own browser. What is claimed
/// is what will be honoured, so a mode outside these two is refused on the wire
/// rather than answered on the reader's behalf.
///
/// **Session compaction and notices are data shapes too** (schema 1.11): the
/// timeline draws both, merging a compaction's updates and summary chunks into
/// one entry, so they are claimed.
///
/// **`fs/*` and `terminal/*` are declined by default, and a launch may claim
/// them** (F3, [`ClientServices`]): a claimed call is answered by the reader or
/// by a rules file, never performed — there is still no filesystem mediation
/// and no terminal (F16). A call that arrives unclaimed is recorded and answered
/// method-not-found (§8). The rule is unchanged: **this client claims exactly
/// what it will honour.**
///
/// This is the default claim; what a connection actually sent is
/// [`Inspector::client_claim`](crate::Inspector::client_claim).
#[must_use]
pub fn client_capabilities() -> v1::ClientCapabilities {
    ClientServices::default().capabilities()
}
/// The stores the typed layer writes and the screens read.
///
/// Held by the inspector and shared into each connection's client, because they
/// outlive any one connection: what an agent claimed and what it said stay
/// readable after it is gone, which is the rule the trace and the diagnostic log
/// already follow.
#[derive(Clone, Default)]
pub(crate) struct Stores {
    pub(crate) agent: Field<Option<v1::InitializeResponse>>,
    pub(crate) session: Field<Option<v1::SessionId>>,
    pub(crate) listing: Field<Option<SessionListing>>,
    pub(crate) auth: Field<AuthState>,
    pub(crate) turn: Field<TurnState>,
    /// What the live session is configured as (§7.6): the modes and the config
    /// options the agent published for it, and whatever it has said about them
    /// since. The session's own, unlike everything above it that is the
    /// connection's — which is why opening another session replaces it whole.
    pub(crate) settings: Field<SessionSettings>,
    /// What became of driving each of the agent's advertisements on this
    /// connection (§7.7): not driven, answered, or refused with the agent's own
    /// error. The connection's rather than the live session's — a capability is
    /// claimed once per connection and is true of every session on it, so
    /// opening another session leaves this alone.
    pub(crate) driven: Field<DrivenRecord>,
    pub(crate) timeline: Timeline,
    /// The permission requests the agent is still waiting on (§7.2). Screens
    /// *render* them off the timeline, where the turn stopped for each of them;
    /// this is the list, for the things that are true of all of them at once —
    /// who a cancel owes an answer to, what a turn's end gives up on, and
    /// whether the agent is waiting on anybody at all.
    pub(crate) permissions: Pending<PermissionRequest>,
    /// The elicitations it is still waiting on (§7.8) — the same list for the
    /// same three questions, kept apart because the answers are different
    /// sentences and one of them outlives the turn it arrived in.
    pub(crate) elicitations: Pending<ElicitationRequest>,
    /// The URL elicitations nobody has said are finished (§7.8): what an
    /// `elicitation/complete` is matched against, and what decides whether an
    /// `elicitationId` was reused while still outstanding.
    pub(crate) outstanding: Outstanding,
    /// The client-service requests the agent is waiting on (F3): the third
    /// list, for the same three questions.
    pub(crate) services: Pending<ServiceRequest>,
    /// What the next connection claims and how it answers services: the
    /// launch's choice, read once by each new client.
    pub(crate) launch: Arc<Mutex<ClientServices>>,
    /// What this connection's `initialize` claimed, once it went out.
    pub(crate) claim: Field<Option<ClientClaim>>,
    /// How permission requests are answered in the open session (F11).
    pub(crate) policy: Field<PermissionPolicy>,
    /// Which connection is the newest, so a late teardown of an older one
    /// leaves the newer one's policy alone.
    pub(crate) generation: Arc<AtomicU64>,
}

/// One connection's client: the conversation, and what it writes into.
///
/// A handle, not the client: cloning shares the pending calls and the stores,
/// so the task reading the agent and the task driving it are one client.
#[derive(Clone)]
pub(crate) struct Client {
    rpc: Rpc,
    stores: Stores,
    /// The id of the answer the open turn is waiting for.
    ///
    /// This is what makes the turn's state stream-driven rather than
    /// call-bracketed (§11 seam 2): the reader recognizes the turn's answer
    /// when it crosses, and settles the state from the frame. Whoever is
    /// awaiting the call is told afterwards, and could be nobody.
    awaiting: Arc<Mutex<Option<i64>>>,
    /// The `session/cancel` that cancelled the open turn, held until the turn
    /// resolves — and `None` whenever no rule is watching (§15 q7).
    ///
    /// **It is the arming of the rule and the evidence for it at once.** A
    /// frame here means this client sent a cancel for a turn that was still
    /// running, which is the only condition under which the specification's
    /// MUST applies; and it is the frame the annotation is read beside, because
    /// a notification is the one thing sent here that nothing will ever refer
    /// back to by id.
    cancelled: Arc<Mutex<Option<Frame>>>,
    /// The `session/load` this client is waiting on, and whether anything has
    /// been replayed into it — `None` whenever no load is in flight (§15 q7).
    ///
    /// The second rule's arming, and the same shape as the first's: a value
    /// here means this client asked an agent to load a session, which is the
    /// only condition under which the ordering MUST applies.
    loading: Arc<Mutex<Option<Loading>>>,
    /// The session setup call on the wire, and which of the three shapes its
    /// answer will arrive in — `None` whenever this client is not opening a
    /// session (§7.6).
    ///
    /// **Why the settings are read here rather than where the call is
    /// awaited.** An agent is free to announce a mode change in the breath
    /// after it answers, and the reader has that notification in hand before
    /// whoever asked for the session has woken up — so a caller writing the
    /// answer's settings afterwards would paint over an announcement that came
    /// later on the wire. Registered before the frame goes out, for the reason
    /// a turn registers its id before the prompt does: the answer is allowed to
    /// arrive before anyone here is looking for it.
    opening: Arc<Mutex<Option<Opening>>>,
    /// The `session/set_mode` on the wire, and what has crossed since it went
    /// out — `None` whenever this client is not setting a mode (§7.6).
    ///
    /// **Read where the frame is read, for [`opening`](Self::opening)'s
    /// reason.** An agent is free to announce the mode change in the breath
    /// before or after it answers, and the one task reading the connection has
    /// both in wire order; a caller writing the answer's outcome after it woke
    /// up would say *acknowledged and not restated* over an announcement that
    /// restated it.
    changing_mode: Arc<Mutex<Option<ChangingMode>>>,
    /// The `session/set_config_option` on the wire — `None` whenever this
    /// client is not setting one (§7.6).
    ///
    /// **Read where the frame is read**, for the reason
    /// [`changing_mode`](Self::changing_mode) is: the answer carries the whole
    /// option set, and an agent that restates its options in the breath before
    /// answering has said something the reader has already taken. Writing the
    /// answer from whoever awaited the call would put the two frames in the
    /// order the caller woke up in rather than the order they crossed.
    setting_option: Arc<Mutex<Option<SettingOption>>>,
    /// Which Advertisements each call on the wire is driving — empty whenever
    /// this client is driving none (§7.7).
    ///
    /// **The sixth id-keyed registration, and the one that keeps ACP out of the
    /// envelope.** `rpc` knows JSON-RPC and not ACP and must stay that way, so
    /// the map from an id to the capabilities its answer is about lives here,
    /// with the layer that knows what a method means. Registered *before* the
    /// frame goes out, the rule five things here already follow: the answer is
    /// allowed to arrive before anyone is looking for it.
    ///
    /// **A list rather than one capability**, because one call can drive two: a
    /// `session/load` carrying `additionalDirectories` drives the method it is
    /// and the field it carries, and both are answered by the same frame (§7.7).
    ///
    /// **Emptied by the teardown**, which is what stops a dying connection's
    /// late answer landing in the record — the mechanism the five slots above
    /// already rely on, rather than a connection ordinal on one store and not
    /// the others.
    driving: Arc<Mutex<HashMap<i64, Vec<AgentCapability>>>>,
    /// The first thing the agent said in each session on this connection.
    ///
    /// **The record that there was a conversation to replay**, which is the
    /// other half of what the load rule is decided from — and one frame per
    /// session rather than all of them, because the claim it supports is that
    /// the session was not empty, and one message is the whole of that. Sessions
    /// rather than the trace's whole record: a session id is one agent's, and the
    /// next connection's identical id is a different session.
    ///
    /// Only sessions this client opened are kept, so an agent naming arbitrary
    /// ids cannot pin a frame for each.
    conversed: Arc<Mutex<HashMap<v1::SessionId, Frame>>>,
    /// The session the reader last saw an open succeed for. Set where the
    /// answer is read, so updates that follow it are attributed before the
    /// caller has woken to make it live.
    opened_last: Arc<Mutex<Option<v1::SessionId>>>,
    /// What this connection claims and how it answers client services, as the
    /// launch chose them when it connected.
    services: Arc<ClientServices>,
    /// This connection's place in [`Stores::generation`].
    generation: u64,
}
impl Client {
    pub(crate) fn new(outgoing: FrameSink, stores: Stores) -> Self {
        let services = Arc::new(locked(&stores.launch).clone());
        let generation = stores.generation.fetch_add(1, Ordering::SeqCst) + 1;
        // A new connection has claimed nothing yet, and answers every
        // permission request by asking.
        stores.claim.set(None);
        stores.policy.set(PermissionPolicy::Ask);
        Self {
            rpc: Rpc::new(outgoing),
            stores,
            awaiting: Arc::default(),
            cancelled: Arc::default(),
            loading: Arc::default(),
            opening: Arc::default(),
            changing_mode: Arc::default(),
            setting_option: Arc::default(),
            driving: Arc::default(),
            conversed: Arc::default(),
            opened_last: Arc::default(),
            services,
            generation,
        }
    }

    pub(crate) fn sink(&self) -> &FrameSink {
        self.rpc.sink()
    }

    /// The JSON-RPC conversation itself, for what is sent beside the typed
    /// layer rather than through it (F2's hand-sent frames).
    pub(crate) fn rpc(&self) -> &Rpc {
        &self.rpc
    }

    /// The calls this client is still waiting on, oldest first.
    pub(crate) fn pending_calls(&self) -> Vec<PendingCall> {
        self.rpc.pending()
    }

    /// Asks the agent to give up a call still waiting for its answer, with
    /// `$/cancel_request`. The call stays pending until the agent answers it.
    pub(crate) async fn cancel_call(&self, id: i64) -> Result<(), CallError> {
        if !self.rpc.is_pending(id) {
            return Err(CallError::NotWaiting);
        }
        self.rpc
            .notify(
                v1::PROTOCOL_LEVEL_METHOD_NAMES.cancel_request,
                encode(&v1::CancelRequestNotification::new(v1::RequestId::Number(
                    id,
                ))),
            )
            .await
            .map(|_| ())
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: &impl Serialize,
    ) -> Result<T, CallError> {
        decode(self.rpc.call(method, encode(params)).await?)
    }

    fn session(&self) -> Result<v1::SessionId, CallError> {
        self.stores.session.get().ok_or(CallError::NoSession)
    }
}

/// A request's parameters as JSON.
///
/// Infallible in practice and treated as such: these are the schema crate's own
/// types, and a type that cannot serialize itself is a bug in this program
/// rather than something an agent did — the failure the inspector must never
/// invent is one it blames on the agent.
pub(crate) fn encode(params: &impl Serialize) -> Value {
    serde_json::to_value(params).unwrap_or(Value::Null)
}

/// An answer, read as v1.
///
/// A result that will not decode is not an error the agent reported — it is
/// this layer failing to recognize something — so it is named as such and the
/// frame stays in the trace to be read by a human (§8).
fn decode<T: serde::de::DeserializeOwned>(result: Value) -> Result<T, CallError> {
    crate::decode::from_value(result).map_err(|problem| CallError::Undecodable(problem.to_string()))
}
