//! The typed ACP client: the driven methods on the way out, and one place that
//! reads everything on the way in. See `docs/architecture.md` §5 and §7.
//!
//! The trace has every frame before this layer sees it, so the client only
//! decorates: a frame it cannot recognize becomes an unrecognized timeline
//! entry carrying its own JSON (§8), never an error or a lost frame.
//!
//! Everything is decoded as v1, from the schema crate's `v1` module, so a v2
//! decoder can sit beside this one (§11).
//!
//! The client claims only what it honours (`client_capabilities`). A request
//! for a service it did not claim is recorded and answered method-not-found;
//! claimed client services are answered by the reader or a rules file
//! (`docs/client-services.md`).

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
use crate::session_report::SessionReports;
use crate::session_settings::SessionSettings;
use crate::store::{Field, locked};
use crate::timeline::Timeline;
use crate::turn::TurnState;

pub(crate) use driving::drives;
use rules::Loading;
use settings::{ChangingMode, Opening, SettingOption};

const CLIENT_NAME: &str = "acp-inspector";
const CLIENT_TITLE: &str = "ACP Inspector";

/// Who this client says it is in every `initialize` request.
///
/// Public so a surface showing the claim reads the value the wire gets.
#[must_use]
pub fn client_info() -> v1::Implementation {
    v1::Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")).title(CLIENT_TITLE)
}

/// The client capabilities claimed by default.
///
/// Claims boolean config options, session compaction and notices (data shapes
/// the timeline renders) and both elicitation modes (§7.8). `fs/*` and
/// `terminal/*` are declined unless a launch claims them through
/// [`ClientServices`]. The claim a connection actually sent is
/// [`Inspector::client_claim`](crate::Inspector::client_claim).
#[must_use]
pub fn client_capabilities() -> v1::ClientCapabilities {
    ClientServices::default().capabilities()
}

/// The stores the typed layer writes and the screens read.
///
/// Held by the inspector and shared into each connection's client, so what an
/// agent claimed and said stays readable after its connection is gone.
#[derive(Clone, Default)]
pub(crate) struct Stores {
    pub(crate) agent: Field<Option<v1::InitializeResponse>>,
    pub(crate) session: Field<Option<v1::SessionId>>,
    pub(crate) listing: Field<Option<SessionListing>>,
    pub(crate) auth: Field<AuthState>,
    pub(crate) turn: Field<TurnState>,
    /// The live session's modes and config options (§7.6). Replaced whole when
    /// another session opens.
    pub(crate) settings: Field<SessionSettings>,
    /// What the agent has said about each session's title, activity and
    /// usage (§7.5). Per connection, keyed by session.
    pub(crate) reports: Field<SessionReports>,
    /// What became of driving each advertisement on this connection (§7.7).
    /// Per connection, so opening another session leaves it alone.
    pub(crate) driven: Field<DrivenRecord>,
    pub(crate) timeline: Timeline,
    /// Permission requests the agent is still waiting on. The timeline renders
    /// them; this list answers what a cancel or a turn's end owes.
    pub(crate) permissions: Pending<PermissionRequest>,
    /// Elicitations the agent is still waiting on. Kept apart from permissions
    /// because a request-scoped elicitation outlives its turn (§7.8).
    pub(crate) elicitations: Pending<ElicitationRequest>,
    /// URL elicitations not yet completed: what `elicitation/complete` is
    /// matched against, and what detects a reused `elicitationId`.
    pub(crate) outstanding: Outstanding,
    /// Client-service requests the agent is still waiting on.
    pub(crate) services: Pending<ServiceRequest>,
    /// What the next connection claims and how it answers services, read once
    /// by each new client.
    pub(crate) launch: Arc<Mutex<ClientServices>>,
    /// What this connection's `initialize` claimed, once it went out.
    pub(crate) claim: Field<Option<ClientClaim>>,
    /// How permission requests are answered in the open session.
    pub(crate) policy: Field<PermissionPolicy>,
    /// Which connection is the newest, so a late teardown of an older one
    /// leaves the newer one's policy alone.
    pub(crate) generation: Arc<AtomicU64>,
}

/// One connection's client: the conversation, and what it writes into.
///
/// A handle: clones share the pending calls and the stores, so the task reading
/// the agent and the task driving it are one client.
///
/// Every id-keyed slot below is registered before its frame is sent, because
/// the answer may arrive before the caller is looking for it. Answers are
/// handled by the reader in wire order, never by whoever awaited the call, so a
/// notification that crossed after an answer is never overwritten by it.
#[derive(Clone)]
pub(crate) struct Client {
    rpc: Rpc,
    stores: Stores,
    /// The id of the answer the open turn is waiting for. The reader settles
    /// the turn when that answer crosses (§11 seam 2).
    awaiting: Arc<Mutex<Option<i64>>>,
    /// The `session/cancel` sent for the still-running turn, kept until the
    /// turn resolves. Arms the cancellation rule and is its evidence (§15 q7).
    cancelled: Arc<Mutex<Option<Frame>>>,
    /// The `session/load` in flight and whether anything was replayed into it.
    /// Arms the replay rule (§15 q7).
    loading: Arc<Mutex<Option<Loading>>>,
    /// The session setup call in flight, whose answer publishes the settings.
    opening: Arc<Mutex<Option<Opening>>>,
    /// The `session/set_mode` in flight, and whether the agent has restated a
    /// mode since.
    changing_mode: Arc<Mutex<Option<ChangingMode>>>,
    /// The `session/set_config_option` in flight.
    setting_option: Arc<Mutex<Option<SettingOption>>>,
    /// The advertisements each call in flight drives (§7.7). One call can drive
    /// several. Kept here rather than in `rpc`, which knows nothing of ACP.
    /// Emptied by the teardown so a dead connection's late answer is not
    /// recorded.
    driving: Arc<Mutex<HashMap<i64, Vec<AgentCapability>>>>,
    /// The first frame the agent sent in each session this client opened: the
    /// evidence that a load had a conversation to replay. Limited to known
    /// sessions so an agent cannot pin a frame per invented id.
    conversed: Arc<Mutex<HashMap<v1::SessionId, Frame>>>,
    /// The session the reader last saw an open succeed for, so updates that
    /// follow are attributed before the caller makes it live.
    opened_last: Arc<Mutex<Option<v1::SessionId>>>,
    /// What this connection claims and how it answers client services.
    services: Arc<ClientServices>,
    /// This connection's place in [`Stores::generation`].
    generation: u64,
}

impl Client {
    pub(crate) fn new(outgoing: FrameSink, stores: Stores) -> Self {
        let services = Arc::new(locked(&stores.launch).clone());
        let generation = stores.generation.fetch_add(1, Ordering::SeqCst) + 1;
        // A new connection has claimed nothing yet, and asks about every
        // permission request.
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

    /// The JSON-RPC conversation, for frames sent by hand beside the typed
    /// layer.
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
/// The schema crate's types always serialize; a failure would be a bug here,
/// and must not be reported as something the agent did.
pub(crate) fn encode(params: &impl Serialize) -> Value {
    serde_json::to_value(params).unwrap_or(Value::Null)
}

/// An answer, read as v1. A result that does not decode is this layer failing
/// to recognize it, not an error the agent reported.
fn decode<T: serde::de::DeserializeOwned>(result: Value) -> Result<T, CallError> {
    crate::decode::from_value(result).map_err(|problem| CallError::Undecodable(problem.to_string()))
}
