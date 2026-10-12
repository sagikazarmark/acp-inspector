//! What the screens read: the stores, each with a stream of its changes,
//! and what the agent advertised.

use agent_client_protocol_schema::v1;

use crate::auth::AuthState;
use crate::driven::{AgentCapability, DrivenRecord};
use crate::elicitation::ElicitationRequest;
use crate::listing::SessionListing;
use crate::permission::PermissionRequest;
use crate::restore::Restore;
use crate::session_report::SessionReports;
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

    /// Watches the connection status.
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

    /// What the agent said about itself in `initialize`: info, capabilities,
    /// auth methods. `None` until `initialize` has been answered.
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

    /// The live session's modes and config options (§7.6).
    ///
    /// Filled from the session setup response and updated by
    /// `current_mode_update` and `config_option_update`. Replaced whole when
    /// another session opens, emptied when one ends. Empty means the agent
    /// published nothing.
    pub fn session_settings(&self) -> SessionSettings {
        self.0.stores.settings.get()
    }

    pub fn session_settings_changes(&self) -> Changes<SessionSettings> {
        self.0.stores.settings.changes()
    }

    /// What the agent has said about each session's title and last activity
    /// in `session_info_update`, and its context and cost in `usage_update`
    /// (§7.5), keyed by session.
    ///
    /// Per connection: emptied when another agent connects, and a session's
    /// own report is dropped as a call reopening it goes out. A session the
    /// agent never sent an update about has none; its listing row is the
    /// other place it is described.
    pub fn session_reports(&self) -> SessionReports {
        self.0.stores.reports.get()
    }

    pub fn session_reports_changes(&self) -> Changes<SessionReports> {
        self.0.stores.reports.changes()
    }

    /// Whether the agent advertised `session/list`. `false` until
    /// `initialize` has answered.
    pub fn lists_sessions(&self) -> bool {
        self.advertised(AgentCapability::List)
    }

    /// How this agent would reopen a session, or `None` where it advertised
    /// no way to (§7.5).
    ///
    /// Load is preferred where both are advertised, because only load rebuilds
    /// the conversation; [`Resume`](crate::Restore::Resume) is the cue to say
    /// previous messages cannot be shown.
    pub fn advertised_restore(&self) -> Option<Restore> {
        self.agent()
            .as_ref()
            .and_then(|agent| Restore::preferred(&agent.agent_capabilities))
    }

    /// Whether the agent advertised `session/close`. `false` until
    /// `initialize` has answered.
    pub fn closes_sessions(&self) -> bool {
        self.advertised(AgentCapability::Close)
    }

    /// Whether the agent advertised `session/delete`. `false` until
    /// `initialize` has answered.
    pub fn deletes_sessions(&self) -> bool {
        self.advertised(AgentCapability::Delete)
    }

    /// Whether the agent advertised `additionalDirectories`, which gates the
    /// roots control (§7.7). It does not gate a reopen's default roots.
    /// `false` until `initialize` has answered.
    pub fn opens_with_roots(&self) -> bool {
        self.advertised(AgentCapability::AdditionalDirectories)
    }

    /// Whether the agent advertised `logout` (under `auth`). `false` until
    /// `initialize` has answered.
    pub fn logs_out(&self) -> bool {
        self.advertised(AgentCapability::Logout)
    }

    /// Whether this agent claimed a capability, read through
    /// [`AgentCapability::advertised`] so the gates and the driven record agree.
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
    /// Per capability: not driven, answered, or refused with the agent's error
    /// — the latest outcome only. Filled by the typed layer (not derived from
    /// the bounded trace). Cleared when another agent connects, kept when the
    /// agent dies, untouched by a session switch. `authenticate` is recorded in
    /// [`auth`](Self::auth) instead.
    pub fn driven(&self) -> DrivenRecord {
        self.0.stores.driven.get()
    }

    pub fn driven_changes(&self) -> Changes<DrivenRecord> {
        self.0.stores.driven.changes()
    }

    /// Where authentication stands: what the agent asked for, and what came of
    /// asking it.
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
    /// They render on the timeline; this list answers whether the agent is
    /// waiting on somebody. Answer one with [`PermissionRequest::select`].
    pub fn pending_permissions(&self) -> Vec<PermissionRequest> {
        self.0.stores.permissions.waiting()
    }

    /// The elicitations the agent is still waiting on (§7.8).
    ///
    /// Answer one with [`ElicitationRequest::accept`], [`decline`] or
    /// [`cancel`]. One scoped to a JSON-RPC call rather than a session can be
    /// waiting with no session open.
    ///
    /// [`decline`]: ElicitationRequest::decline
    /// [`cancel`]: ElicitationRequest::cancel
    pub fn pending_elicitations(&self) -> Vec<ElicitationRequest> {
        self.0.stores.elicitations.waiting()
    }

    /// The client-service requests the agent is still waiting on
    /// (`docs/client-services.md`). Answer one with
    /// [`ServiceRequest::respond`] or [`ServiceRequest::refuse`].
    ///
    /// [`ServiceRequest::respond`]: crate::ServiceRequest::respond
    /// [`ServiceRequest::refuse`]: crate::ServiceRequest::refuse
    pub fn pending_services(&self) -> Vec<crate::ServiceRequest> {
        self.0.stores.services.waiting()
    }

    /// Chooses what the *next* connection claims in `initialize` and how it
    /// answers client services. The live connection keeps what it was launched
    /// with; kept across disconnect and reconnect, like the MCP draft.
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

    /// How `session/request_permission` is answered in the open session.
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
