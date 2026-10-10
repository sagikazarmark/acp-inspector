//! What the screens read: the stores, each with a stream of its changes,
//! and what the agent advertised.

use agent_client_protocol_schema::v1;

use crate::auth::AuthState;
use crate::driven::{AgentCapability, DrivenRecord};
use crate::elicitation::ElicitationRequest;
use crate::listing::SessionListing;
use crate::permission::PermissionRequest;
use crate::restore::Restore;
use crate::session_settings::SessionSettings;
use crate::store::Changes;
use crate::timeline::Timeline;
use crate::trace::Trace;
use crate::turn::TurnState;

use super::{ConnectionStatus, DiagnosticLog, Inspector};

impl Inspector {
    pub fn status(&self) -> ConnectionStatus {
        self.0.status.get()
    }

    /// Watches the connection status. Subscribing before connecting is the
    /// point: the shell renders a status it was told about, never one it polled
    /// for.
    pub fn status_changes(&self) -> Changes<ConnectionStatus> {
        self.0.status.changes()
    }

    /// Every frame on every connection this inspector has had (§6.2).
    pub fn trace(&self) -> &Trace {
        &self.0.trace
    }

    pub fn diagnostics(&self) -> &DiagnosticLog {
        &self.0.diagnostics
    }

    /// What the agent said about itself when it was initialized: its info, its
    /// capabilities, its auth methods (§9, the capability display).
    ///
    /// `None` until `initialize` has been answered — and capability-gated
    /// affordances are gated on what is advertised here, not on whether the
    /// thing they would show has any content yet.
    pub fn agent(&self) -> Option<v1::InitializeResponse> {
        self.0.stores.agent.get()
    }

    pub fn agent_changes(&self) -> Changes<Option<v1::InitializeResponse>> {
        self.0.stores.agent.changes()
    }

    /// The session turns happen in, once `session/new` has answered.
    pub fn session(&self) -> Option<v1::SessionId> {
        self.0.stores.session.get()
    }

    pub fn session_changes(&self) -> Changes<Option<v1::SessionId>> {
        self.0.stores.session.changes()
    }

    /// What the live session is configured as, and what it could be configured
    /// as: its modes and its config options (§7.6).
    ///
    /// **Filled from the session setup response**, and kept up to date by the
    /// `current_mode_update` and `config_option_update` notifications the agent
    /// sends afterwards. It is the *live* session's, so opening another
    /// replaces it whole and ending one empties it — there is no merge and no
    /// carry-over, which is the rule the timeline already follows.
    ///
    /// **Empty is an answer**, and the one the affordances gate on: modes and
    /// config options are advertised by the presence of their fields on that
    /// response, so a [`SessionSettings`] with nothing in it is the agent
    /// saying it published nothing rather than this store saying it does not
    /// know yet.
    pub fn session_settings(&self) -> SessionSettings {
        self.0.stores.settings.get()
    }

    pub fn session_settings_changes(&self) -> Changes<SessionSettings> {
        self.0.stores.settings.changes()
    }

    /// Whether the agent advertised `session/list` (§7.1, §9).
    ///
    /// **The gate the listing affordance sits behind, and it is gated on the
    /// advertisement rather than on the content**: an agent that advertises
    /// listing and has nothing open still offers the button, because the button
    /// is about what the agent said it can do and the emptiness is about this
    /// moment. `false` until `initialize` has answered — an agent that has
    /// claimed nothing has not claimed this.
    pub fn lists_sessions(&self) -> bool {
        self.advertised(AgentCapability::List)
    }

    /// How this agent would open a session it already has, or `None` where it
    /// advertised no way to (§7.5, §9).
    ///
    /// **The gate the open affordance sits behind, and the preference with
    /// it**: where an agent advertises both, load is preferred, because it is
    /// the one that can rebuild the conversation. Where it advertises only
    /// resume the answer is [`Resume`](crate::Restore::Resume), which is a
    /// screen's cue to say plainly that this agent cannot show previous
    /// messages — a resumed session's empty timeline is correct, and without
    /// the callout it reads as a fault in the inspector.
    ///
    /// Gated on the advertisement and on nothing else, like every affordance
    /// here: an agent that advertises a method it never implemented is offered
    /// it anyway, and what it says when it is driven is the finding.
    pub fn advertised_restore(&self) -> Option<Restore> {
        self.agent()
            .as_ref()
            .and_then(|agent| Restore::preferred(&agent.agent_capabilities))
    }

    /// Whether the agent advertised `session/close` (§7.5, §9).
    ///
    /// **The gate the close affordance sits behind, and it gates on the
    /// advertisement and on nothing else** — never on whether the inspector
    /// thinks the close would succeed. Closing the live session, or one that is
    /// closed already, is undefined rather than forbidden, and an inspector that
    /// withheld the button would be deciding what may be observed (§7.5).
    /// `false` until `initialize` has answered.
    pub fn closes_sessions(&self) -> bool {
        self.advertised(AgentCapability::Close)
    }

    /// Whether the agent advertised `session/delete` (§7.5, §9).
    ///
    /// [`closes_sessions`](Self::closes_sessions)'s rule, for the other call:
    /// the claim was made in the same shape — the presence of an object under
    /// `sessionCapabilities` — and it is the whole of what the affordance is
    /// gated on.
    pub fn deletes_sessions(&self) -> bool {
        self.advertised(AgentCapability::Delete)
    }

    /// Whether the agent advertised `additionalDirectories` (§7.7, §9).
    ///
    /// **The gate the roots control sits behind**, beside the `session/new`
    /// button and on each listing row — and the whole of what it is gated on,
    /// like every affordance here. What it does *not* gate is the reopen's own
    /// default: a session is handed back the roots the listing reported whatever
    /// the agent advertised ([`restore_session`](Self::restore_session)), because
    /// that is fidelity to the session being reopened rather than a claim about
    /// a capability. `false` until `initialize` has answered.
    ///
    /// The claim is made in the shape the five session claims are made in — the
    /// presence of an object under `sessionCapabilities` — for a capability that
    /// gates a *field on a request* rather than a method, which is why it is the
    /// one advertisement here with no call of its own.
    pub fn opens_with_roots(&self) -> bool {
        self.advertised(AgentCapability::AdditionalDirectories)
    }

    /// Whether the agent advertised `logout` (§7.1, §7.7, §9).
    ///
    /// **The gate the logout affordance sits behind**, and it gates on the
    /// advertisement and on nothing else — never on what this tool believes the
    /// agent's state makes sensible. Logging out with nobody logged in is
    /// undefined rather than forbidden, so the button is offered there too:
    /// withholding it would be the inspector deciding what may be observed
    /// (§7.7). `false` until `initialize` has answered.
    ///
    /// The claim is made in the shape the five session claims are made in — the
    /// presence of an object — under `auth` rather than under
    /// `sessionCapabilities`, because the call is the connection's and not a
    /// session's.
    pub fn logs_out(&self) -> bool {
        self.advertised(AgentCapability::Logout)
    }

    /// Whether this agent claimed a capability at all.
    ///
    /// **The one place the two shapes v1 states a claim in are read**
    /// ([`AgentCapability::advertised`]), so the gates below and the record's
    /// own key cannot come to disagree about what an agent said. `false` until
    /// `initialize` has answered: an agent that has claimed nothing has not
    /// claimed this.
    fn advertised(&self, capability: AgentCapability) -> bool {
        self.agent()
            .is_some_and(|agent| capability.advertised(&agent.agent_capabilities))
    }

    /// What the agent answered the last time it was asked for its sessions, or
    /// `None` if it has not been asked on this connection.
    pub fn listing(&self) -> Option<SessionListing> {
        self.0.stores.listing.get()
    }

    pub fn listing_changes(&self) -> Changes<Option<SessionListing>> {
        self.0.stores.listing.changes()
    }

    /// What has been driven on this connection, and what came back (§7.7).
    ///
    /// **The fourth fact a capability row carries**: not driven, driven and
    /// answered, or driven and refused with the agent's own error beside it —
    /// the latest outcome and only the latest, because a later drive is the next
    /// ask.
    ///
    /// **Passive, and computed rather than scanned.** Nothing here sends
    /// anything: what fills it is the affordances a user already pressed, so
    /// every frame in the trace is still one they caused. And it is a store the
    /// typed layer writes rather than a reading of the trace, which is a bounded
    /// ring that drops its oldest frames — a record derived from one would
    /// report *not driven* about an agent that was driven, on exactly the long
    /// connection where somebody wants to read it.
    ///
    /// **Empty is an answer**: this agent has been asked nothing yet. It is
    /// cleared when another agent is connected to, kept when the agent dies —
    /// the panel outlives the agent it describes — and untouched by a session
    /// switch, because what it is about is the connection's claims.
    ///
    /// `authenticate` has no entry here. [`auth`](Self::auth) is already this
    /// record for it, per connection and with the agent's own refusal in it, and
    /// two stores holding the same three things would be two things to disagree.
    pub fn driven(&self) -> DrivenRecord {
        self.0.stores.driven.get()
    }

    pub fn driven_changes(&self) -> Changes<DrivenRecord> {
        self.0.stores.driven.changes()
    }

    /// Where authentication stands (§7.1): what the agent asked for, and what
    /// came of asking it.
    pub fn auth(&self) -> AuthState {
        self.0.stores.auth.get()
    }

    pub fn auth_changes(&self) -> Changes<AuthState> {
        self.0.stores.auth.changes()
    }

    /// What the agent has said during the session, decoded where the typed
    /// layer recognizes it and kept raw where it does not (§8).
    pub fn timeline(&self) -> &Timeline {
        &self.0.stores.timeline
    }

    /// The permission requests the agent is still waiting on (§7.2).
    ///
    /// **Where they *render* is the timeline**, at the point the turn stopped
    /// for each of them — this is the same requests, asked as one question: is
    /// the agent waiting on somebody? A screen derives "the agent is waiting on
    /// you" from this rather than from a turn state, because a turn blocked on
    /// a request is still the turn that was running (§11 seam 2).
    ///
    /// Answering one is [`PermissionRequest::select`]: the request carries its
    /// own resolver, so a caller holding one out of this list can answer it
    /// without knowing anything else.
    pub fn pending_permissions(&self) -> Vec<PermissionRequest> {
        self.0.stores.permissions.waiting()
    }

    /// The elicitations the agent is still waiting on (§7.8).
    ///
    /// The other half of "the agent is waiting on you", and a screen saying that
    /// sentence reads both: which kind stopped the tool is not a distinction the
    /// reader's next action turns on.
    ///
    /// Answering one is [`ElicitationRequest::accept`], [`decline`] or
    /// [`cancel`] — three answers rather than an option id, and every one of
    /// them a claim about what the reader did.
    ///
    /// A list that outlives no live session: an elicitation scoped to a
    /// JSON-RPC call was never about a conversation, and can be waiting here
    /// with nothing open.
    ///
    /// [`decline`]: ElicitationRequest::decline
    /// [`cancel`]: ElicitationRequest::cancel
    pub fn pending_elicitations(&self) -> Vec<ElicitationRequest> {
        self.0.stores.elicitations.waiting()
    }

    /// The client-service requests the agent is still waiting on (F3): the
    /// third half of "the agent is waiting on you".
    ///
    /// Answering one is [`ServiceRequest::respond`] or
    /// [`ServiceRequest::refuse`].
    ///
    /// [`ServiceRequest::respond`]: crate::ServiceRequest::respond
    /// [`ServiceRequest::refuse`]: crate::ServiceRequest::refuse
    pub fn pending_services(&self) -> Vec<crate::ServiceRequest> {
        self.0.stores.services.waiting()
    }

    /// Chooses what the *next* connection claims in `initialize` and how it
    /// answers client services (F3, F13). The live connection keeps what it
    /// was launched with; kept across disconnect and reconnect, like the MCP
    /// draft.
    pub fn set_client_services(&self, services: crate::ClientServices) {
        *crate::store::locked(&self.0.stores.launch) = services;
    }

    /// What the next connection will claim and how it will answer.
    pub fn client_services(&self) -> crate::ClientServices {
        crate::store::locked(&self.0.stores.launch).clone()
    }

    /// What this connection's `initialize` claimed, once it has gone out.
    pub fn client_claim(&self) -> Option<crate::ClientClaim> {
        self.0.stores.claim.get()
    }

    pub fn client_claim_changes(&self) -> Changes<Option<crate::ClientClaim>> {
        self.0.stores.claim.changes()
    }

    /// How `session/request_permission` is answered in the open session
    /// (F11).
    pub fn permission_policy(&self) -> crate::PermissionPolicy {
        self.0.stores.policy.get()
    }

    /// Sets how permission requests that arrive from now on are answered. Goes
    /// back to [`Ask`](crate::PermissionPolicy::Ask) when the session changes
    /// or the agent disconnects.
    pub fn set_permission_policy(&self, policy: crate::PermissionPolicy) {
        self.0.stores.policy.set(policy);
    }

    pub fn permission_policy_changes(&self) -> Changes<crate::PermissionPolicy> {
        self.0.stores.policy.changes()
    }

    /// Where the turn stands — a field the stream drives, never a bracket
    /// around the prompt call (§11 seam 2).
    pub fn turn(&self) -> TurnState {
        self.0.stores.turn.get()
    }

    pub fn turn_changes(&self) -> Changes<TurnState> {
        self.0.stores.turn.changes()
    }
}
