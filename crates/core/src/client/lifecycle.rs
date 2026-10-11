//! The session lifecycle: `initialize`, opening, reopening, leaving, closing
//! and listing sessions, and authentication. See `docs/architecture.md` §7.5.

use std::path::PathBuf;

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::auth::AuthState;
use crate::call::CallError;
use crate::driven::AgentCapability;
use crate::listing::{Cursor, SessionListing};
use crate::permission::PermissionPolicy;
use crate::restore::Restore;
use crate::roots::{self, Roots};
use crate::services::ClientClaim;
use crate::session_settings::SessionSettings;
use crate::store::locked;
use crate::turn::TurnState;

use super::driving::{drives, drives_setup};
use super::settings::Setup;
use super::{Client, Loading, client_info, decode, encode};

/// Which blocking requests are answered `cancelled` (§7.2, §7.8).
#[derive(Clone, Copy)]
pub(super) enum Debt {
    /// The turn's: every permission request, and every elicitation or service
    /// request that named a session. A request scoped to a JSON-RPC call
    /// outlives the turn.
    TheTurns,
    /// Everything, because the timeline that would answer them is going away.
    Everything,
}

impl Client {
    /// `initialize`: the launch's protocol version and client capabilities, and
    /// [`client_info`]. The claim that went out is kept in `Stores::claim`.
    pub(crate) async fn initialize(&self) -> Result<v1::InitializeResponse, CallError> {
        let claim = ClientClaim {
            capabilities: self.services.capabilities(),
            protocol_version: self.services.protocol(),
        };
        let request = v1::InitializeRequest::new(claim.protocol_version)
            .client_capabilities(claim.capabilities.clone())
            .client_info(client_info());
        self.stores.claim.set(Some(claim));

        let response: v1::InitializeResponse = self
            .call(v1::AGENT_METHOD_NAMES.initialize, &request)
            .await?;
        self.stores.agent.set(Some(response.clone()));
        Ok(response)
    }

    /// `session/new` with the given working directory, roots and MCP servers.
    ///
    /// With a session already live this is a switch (§7.5): the live session is
    /// left first, and the new one becomes live. The settings the answer
    /// publishes are written by the reader (`opened`). Roots resolve against
    /// `cwd`; an empty list sends no `additionalDirectories` field.
    pub(crate) async fn new_session(
        &self,
        cwd: PathBuf,
        roots: Option<&Roots>,
        mcp: Vec<v1::McpServer>,
    ) -> Result<v1::SessionId, CallError> {
        let switching = self.stores.session.get().is_some();
        if switching {
            self.leave().await;
        }

        let request = v1::NewSessionRequest::new(cwd.clone())
            .mcp_servers(mcp)
            .additional_directories(roots::creating(roots, &cwd));
        let params = encode(&request);
        // No capability gates `session/new`, so it drives only what the frame
        // carries. Prepared by hand rather than through `drive` because the
        // settings slot needs the id before the frame goes out.
        let driving = drives_setup(&params);
        let call = self
            .rpc
            .prepare(v1::AGENT_METHOD_NAMES.session_new, params)
            .inspect_err(|error| self.drive_failed(None, &driving, error))?;
        let sent = call.id();
        self.registers(sent, &driving);
        // No session id to watch: `session/new` mints it in the answer.
        self.opening(sent, Setup::New, None);

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            // No frame the agent could answer, so no reader event: record the
            // failure here, under the id guard.
            self.drive_failed(Some(sent), &driving, error);
        }
        let response: v1::NewSessionResponse = decode(outcome?)?;

        // Discarded after the answer, so the old session's last words land in
        // the view being left. Only on a switch: a relaunch empties the view in
        // `Inspector::connect`.
        if switching {
            self.stores.timeline.discard();
        }
        // View and turn first, session last, so a screen woken by the session
        // changing reads state that already belongs to it.
        self.stores.turn.set(TurnState::Idle);
        self.stores.session.set(Some(response.session_id.clone()));
        Ok(response.session_id)
    }

    /// `session/load` or `session/resume`, as `how` says: opens an existing
    /// session and makes it the live one (§7.5).
    ///
    /// The view is discarded before the request, because a load replays ahead
    /// of its answer; resume does the same so a resume that wrongly replays is
    /// still shown. A failure leaves the live session live, with whatever was
    /// replayed still on the timeline. A connection already gone fails before
    /// anything is given up.
    ///
    /// The request carries the session's own `cwd`. Roots are the caller's,
    /// defaulting to the ones the listing reported, and cross whether or not
    /// `additionalDirectories` was advertised. The outcome is recorded against
    /// the advertisement `how` selects ([`drives`]).
    pub(crate) async fn restore(
        &self,
        session: &v1::SessionInfo,
        how: Restore,
        roots: Option<&Roots>,
        mcp: Vec<v1::McpServer>,
    ) -> Result<v1::SessionId, CallError> {
        let id = session.session_id.clone();
        let asked = roots::reopening(roots, session);
        let params = match how {
            Restore::Load => encode(
                &v1::LoadSessionRequest::new(id.clone(), session.cwd.clone())
                    .mcp_servers(mcp)
                    .additional_directories(asked.clone()),
            ),
            Restore::Resume => encode(
                &v1::ResumeSessionRequest::new(id.clone(), session.cwd.clone())
                    .mcp_servers(mcp)
                    .additional_directories(asked),
            ),
        };

        // The call's own advertisement, plus whatever its frame carries.
        let mut driving = vec![drives(how)];
        driving.extend(drives_setup(&params));

        // Prepared first: a connection that has gone away fails here, before
        // the switch spends the view.
        let call = self
            .rpc
            .prepare(how.method(), params)
            .inspect_err(|error| self.drive_failed(None, &driving, error))?;
        let sent = call.id();
        // Registered before `leave` awaits anything: a teardown during the
        // switch must find and refuse this entry, rather than the caller writing
        // a refusal into the next connection's record.
        self.registers(sent, &driving);

        if self.stores.session.get().is_some() {
            self.leave().await;
            self.stores.timeline.discard();
        }

        // Armed before the frame goes out, since the replay may arrive first.
        if how.replays() {
            *locked(&self.loading) = Some(Loading {
                call: call.id(),
                session: id.clone(),
                request: call.frame().clone(),
                replayed: false,
            });
        }
        // Likewise the settings, with the session id that tells this session's
        // replay from the session being left.
        self.opening(call.id(), Setup::Restore(how), Some(id.clone()));

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            // No frame the agent could answer, so no reader event: record the
            // failure here, under the id guard.
            self.drive_failed(Some(sent), &driving, error);
        }
        outcome?;

        self.stores.turn.set(TurnState::Idle);
        self.stores.session.set(Some(id.clone()));
        Ok(id)
    }

    /// `session/close`. A running turn is not cancelled first: what the agent
    /// does with it is the observation.
    pub(crate) async fn close_session(&self, session: &v1::SessionId) -> Result<(), CallError> {
        self.end_session(
            v1::AGENT_METHOD_NAMES.session_close,
            encode(&v1::CloseSessionRequest::new(session.clone())),
            session,
            AgentCapability::Close,
        )
        .await
    }

    /// `session/delete`. The listing is asked for again rather than edited.
    pub(crate) async fn delete_session(&self, session: &v1::SessionId) -> Result<(), CallError> {
        self.end_session(
            v1::AGENT_METHOD_NAMES.session_delete,
            encode(&v1::DeleteSessionRequest::new(session.clone())),
            session,
            AgentCapability::Delete,
        )
        .await
    }

    /// Sends a close or delete. If the agent accepted it for the live session,
    /// there is no live session any more; a refusal leaves it live. Either way
    /// the listing is refreshed.
    pub(super) async fn end_session(
        &self,
        method: &str,
        params: Value,
        session: &v1::SessionId,
        driving: AgentCapability,
    ) -> Result<(), CallError> {
        let outcome = self.drive(method, params, Some(driving)).await;

        if outcome.is_ok() && self.stores.session.get().as_ref() == Some(session) {
            self.ended().await;
        }
        self.refresh_listing().await;
        outcome.map(|_| ())
    }

    /// Lists sessions again from the start, if the agent advertised
    /// `session/list`, so the screen shows what the agent now claims to hold.
    ///
    /// The one call the inspector makes on its own initiative, so it is gated
    /// on the advertisement and records nothing in the driven record. Its
    /// failure does not change the caller's answer.
    pub(super) async fn refresh_listing(&self) {
        let lists = self
            .stores
            .agent
            .get()
            .is_some_and(|agent| agent.agent_capabilities.session_capabilities.list.is_some());

        if lists {
            let _ = self.list(None, None).await;
        }
    }

    /// The live session was closed or deleted, and there is no other.
    ///
    /// Like [`leave`](Self::leave) without sending `session/cancel`, which
    /// would pre-empt what the close was sent to observe. Waiting requests are
    /// still answered `cancelled`, after the agent's answer. The view and the
    /// settings go with the session; the trace keeps every frame.
    pub(super) async fn ended(&self) {
        self.answer_cancelled(Debt::Everything).await;
        self.forget();
        self.stores.policy.set(PermissionPolicy::Ask);
        self.stores.timeline.discard();
        self.stores.settings.set(SessionSettings::default());
        self.stores.session.set(None);
    }

    /// Stops tracking the live session: no answer is awaited and no rule is
    /// armed for it, and no turn is running. A late answer still reaches its
    /// caller, but no store.
    pub(super) fn forget(&self) {
        *locked(&self.awaiting) = None;
        *locked(&self.cancelled) = None;
        *locked(&self.loading) = None;
        // A setter's answer for the old session must not land on the new
        // session's settings.
        *locked(&self.changing_mode) = None;
        *locked(&self.setting_option) = None;
        self.stores.turn.set(TurnState::Idle);
    }

    /// Walks away from the live session (§7.5).
    ///
    /// A running turn is sent `session/cancel`, and every waiting request is
    /// answered `cancelled` whether or not a turn was running (§7.2). It does
    /// not wait for the turn to end: an agent that never resolves a cancelled
    /// prompt must not hang the switch. The cancellation rule is disarmed,
    /// since nothing will read the frames that would decide it.
    pub(super) async fn leave(&self) {
        if self.stores.turn.get().is_running() {
            let _ = self.cancel().await;
        }
        self.answer_cancelled(Debt::Everything).await;
        self.stores.policy.set(PermissionPolicy::Ask);
        // The turn is over here whatever the agent does next, so a failed
        // switch does not leave the composer waiting.
        self.forget();
    }

    /// Answers waiting requests `cancelled` (§7.2, §7.8); `debt` says which.
    pub(super) async fn answer_cancelled(&self, debt: Debt) {
        for request in self.stores.permissions.take() {
            // One answered a moment ago refuses this second answer itself.
            let _ = request.cancelled().await;
        }
        let elicitations = match debt {
            Debt::Everything => self.stores.elicitations.take(),
            Debt::TheTurns => self
                .stores
                .elicitations
                .take_unless(|request| request.session_id().is_none()),
        };
        for request in elicitations {
            let _ = request.cancel().await;
        }
        let services = match debt {
            Debt::Everything => self.stores.services.take(),
            Debt::TheTurns => self
                .stores
                .services
                .take_unless(|request| request.session_id().is_none()),
        };
        for request in services {
            let _ = request.cancelled().await;
        }
    }

    /// `session/list` from `from`, a cursor a previous page carried, or from
    /// the start. A continuation extends the held listing; a fresh listing
    /// replaces it. Recorded as driving `session/list` (§7.7).
    pub(crate) async fn list_sessions(
        &self,
        from: Option<Cursor>,
    ) -> Result<SessionListing, CallError> {
        self.list(from, Some(AgentCapability::List)).await
    }

    /// The listing itself; `driving` is `None` for the refresh nobody asked
    /// for.
    pub(super) async fn list(
        &self,
        from: Option<Cursor>,
        driving: Option<AgentCapability>,
    ) -> Result<SessionListing, CallError> {
        let request = v1::ListSessionsRequest::new().cursor(from.as_ref().map(Cursor::token));

        let answer = self
            .drive(
                v1::AGENT_METHOD_NAMES.session_list,
                encode(&request),
                driving,
            )
            .await?;
        let response: v1::ListSessionsResponse = decode(answer)?;
        let page = SessionListing {
            sessions: response.sessions,
            next_page: response.next_cursor.map(Cursor::new),
        };

        // Folded under the field's lock so two listings in flight cannot lose
        // a page between them.
        let listed = self.stores.listing.change(|held| {
            let mut folded = held.take().filter(|_| from.is_some()).unwrap_or_default();
            folded.extend(page);
            *held = Some(folded);
        });
        Ok(listed.unwrap_or_default())
    }

    /// `authenticate` with the chosen method. Records the outcome in the auth
    /// store; it cannot log anyone in itself (§1.1).
    pub(crate) async fn authenticate(&self, method: &v1::AuthMethodId) -> Result<(), CallError> {
        let request = v1::AuthenticateRequest::new(method.clone());

        let outcome: Result<v1::AuthenticateResponse, CallError> = self
            .call(v1::AGENT_METHOD_NAMES.authenticate, &request)
            .await;

        self.stores.auth.set(match &outcome {
            Ok(_) => AuthState::Authenticated(method.clone()),
            Err(error) => AuthState::Refused {
                method: method.clone(),
                error: error.clone(),
            },
        });
        outcome.map(|_| ())
    }

    /// `logout`. Connection-scoped; the live session is left alone.
    ///
    /// On success the auth state returns to [`AuthState::Unasked`], without
    /// raising the login screen: the next `auth_required` does that
    /// (`note_login`). A refusal leaves the state as it was.
    pub(crate) async fn logout(&self) -> Result<(), CallError> {
        let request = v1::LogoutRequest::new();

        let answer = self
            .drive(
                v1::AGENT_METHOD_NAMES.logout,
                encode(&request),
                Some(AgentCapability::Logout),
            )
            .await?;
        // Only an answer that decodes as v1 is acted on. The driven record was
        // already settled off the answer's shape.
        decode::<v1::LogoutResponse>(answer)?;

        self.stores.auth.set(AuthState::Unasked);
        Ok(())
    }

    /// Reads `auth_required` out of any refusal, whichever call provoked it:
    /// it says the agent will not proceed until somebody logs in (§7.1).
    pub(super) fn note_login(&self, outcome: &Result<Value, v1::Error>) {
        if let Err(error) = outcome
            && error.code == v1::ErrorCode::AuthRequired
        {
            self.stores.auth.set(AuthState::Required(error.clone()));
        }
    }
}
