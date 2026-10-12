//! What the desktop shell renders: one connection at a time, its trace, its
//! diagnostics and status, and the typed layer's stores (agent, session,
//! timeline, turn). See `docs/architecture.md` §5.
//!
//! The shell renders this and calls back into it and does nothing else, so
//! every screen behaviour is a transition here that tests can assert without a
//! window. The driven methods are `async`, but everything they produce is in a
//! store before they return, put there by the one task reading the connection.

mod read;
mod reader;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1;
use tokio_util::sync::CancellationToken;

use crate::auth::AuthState;
use crate::call::CallError;
use crate::client::{Client, Stores, drives};
use crate::command::AgentCommand;
use crate::connection::{ConnectionFactory, FrameSink};
use crate::driven::{AgentCapability, DrivenRecord};
use crate::listing::{Cursor, SessionListing};
use crate::restore::Restore;
use crate::roots::{self, Roots};
use crate::rpc::PendingCall;
use crate::session_report::SessionReports;
use crate::session_settings::SessionSettings;
use crate::store::Field;
use crate::store::locked;
use crate::trace::Trace;
use crate::turn::TurnState;

pub use reader::DiagnosticLog;
use reader::{read, watched};

/// Where the connection to the agent stands.
///
/// It says what happened to the connection, not why — the diagnostic log
/// carries the reason. It does distinguish who ended the connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionStatus {
    /// No agent, and nobody expected one: before the first launch, and after
    /// the inspector stopped one.
    #[default]
    Disconnected,
    /// An agent is running and the pipe to it is open.
    Connected,
    /// The agent never started.
    FailedToStart,
    /// The agent started and then went away without being asked to — it exited
    /// on its own, or the pipe to it broke.
    ///
    /// Like [`FailedToStart`](Self::FailedToStart), this is when the shell
    /// surfaces the diagnostic log.
    Lost,
}

/// What became of a frame sent by hand ([`Inspector::send_raw`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandSent {
    /// The id a request went out under; `None` for a notification.
    pub id: Option<i64>,
    /// Where the trace put the frame, which is what its answer is paired with.
    pub captured: Option<u64>,
}

/// The inspector: an agent connection and the record of it.
///
/// A handle: clones share one inspector.
#[derive(Clone, Default)]
pub struct Inspector(Arc<Owner>);

/// Only public handles keep the owner alive. The reader holds the stores, but
/// must not keep its own Connection alive after the window lets go.
#[derive(Default)]
struct Owner(Arc<State>);

impl std::ops::Deref for Owner {
    type Target = State;

    fn deref(&self) -> &State {
        &self.0
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.0.release(ConnectionStatus::Disconnected, false);
        for stopping in std::mem::take(&mut *locked(&self.0.stopping)) {
            stopping.kill();
        }
    }
}

#[derive(Default)]
struct State {
    mcp: Mutex<crate::McpDraft>,
    trace: Trace,
    diagnostics: DiagnosticLog,
    status: Field<ConnectionStatus>,
    stores: Stores,
    connection: Mutex<Option<Live>>,
    /// Connections released gracefully whose agent may still be stopping, so
    /// an immediate disconnect or teardown can still kill it.
    stopping: Mutex<Vec<FrameSink>>,
}

/// The connection the inspector currently has, and the two things that end it.
struct Live {
    /// Holds the frame sink: the agent runs as long as this does, so dropping
    /// it is half of the teardown.
    client: Client,
    /// Stops the reader task holding the other two ends.
    shutdown: CancellationToken,
}

impl Inspector {
    /// In-memory input only. Editing affects later opens, never the live Session.
    /// Kept over disconnect/reconnect, never included in recent commands.
    pub fn set_mcp_draft(&self, draft: crate::McpDraft) {
        *self.0.mcp.lock().unwrap_or_else(|e| e.into_inner()) = draft;
    }

    pub fn mcp_draft(&self) -> crate::McpDraft {
        self.0.mcp.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts an agent and begins recording it.
    ///
    /// One connection at a time: whatever was connected is disconnected first.
    /// Never fails: an agent that will not start lands in the diagnostic log,
    /// then the status turns to [`FailedToStart`](ConnectionStatus::FailedToStart)
    /// — in that order, so the console has the evidence when the status points
    /// at it. Must be called from within a tokio runtime.
    pub fn connect(&self, factory: &impl ConnectionFactory) {
        // Read before the teardown below clears it.
        let leaving = self.0.stores.session.get().is_some();
        self.disconnect_gracefully();

        let connection = self.0.trace.tap(factory.connect());
        let (outgoing, incoming, diagnostics) = connection.into_parts();
        let shutdown = CancellationToken::new();
        let client = Client::new(outgoing, self.0.stores.clone());

        self.0.stores.agent.set(None);
        self.0.stores.session.set(None);
        self.0.stores.listing.set(None);
        self.0.stores.auth.set(AuthState::Unasked);
        self.0.stores.turn.set(TurnState::Idle);
        self.0.stores.settings.set(SessionSettings::default());
        self.0.stores.reports.set(SessionReports::default());
        // Cleared only here: the driven record stays readable after its agent
        // has gone, until the next agent starts.
        self.0.stores.driven.set(DrivenRecord::default());
        // A relaunch leaves the session, so the timeline (its view) is emptied.
        // Rows from an agent that never opened a session are kept. The trace
        // keeps every frame either way.
        if leaving {
            self.0.stores.timeline.discard();
        } else {
            self.0.stores.timeline.reopen();
        }

        // Optimistic: a failed start is reported moments later on the
        // diagnostic channel, which corrects the status.
        self.0.status.set(ConnectionStatus::Connected);
        *self.0.connection() = Some(Live {
            client: client.clone(),
            shutdown: shutdown.clone(),
        });

        let state = Arc::clone(&self.0.0);
        let reader = read(
            Arc::clone(&state),
            client,
            incoming,
            diagnostics,
            shutdown.clone(),
        );
        tokio::spawn(watched(state, shutdown, reader));
    }

    /// Starts an agent and opens a session: connect, `initialize`,
    /// `session/new` (§7.1).
    ///
    /// Answers with the session or the first error — an agent that never
    /// started surfaces as [`Disconnected`](CallError::Disconnected), never as
    /// a wait. The connection is left standing either way, so the trace and
    /// console stay readable after a failed handshake.
    pub async fn start(&self, command: &AgentCommand) -> Result<v1::SessionId, CallError> {
        // Snapshot before initialize: edits during the handshake belong to the
        // next opening, and an invalid draft must not replace a live Connection.
        let draft = self.mcp_draft();
        draft.definitions()?;
        // Before the spawn, so a missing directory does not cost a subprocess.
        let cwd = command.session_cwd().ok_or(CallError::NoWorkingDirectory)?;

        self.connect(&command.factory());
        let agent = self.initialize().await?;
        let mcp = draft.definitions_for(&agent.agent_capabilities.mcp_capabilities)?;
        // No roots: nothing is known yet about `additionalDirectories` support.
        self.client()?.new_session(cwd, None, mcp).await
    }

    /// Opens another session on the running agent, in the working directory
    /// `command` names ([`AgentCommand::session_cwd`]), without relaunching.
    ///
    /// Otherwise [`new_session`](Self::new_session). Fails with
    /// [`NoWorkingDirectory`] when no directory can be resolved. `roots: None`
    /// asks for none.
    ///
    /// [`AgentCommand::session_cwd`]: crate::AgentCommand::session_cwd
    /// [`NoWorkingDirectory`]: CallError::NoWorkingDirectory
    pub async fn open_session(
        &self,
        command: &AgentCommand,
        roots: Option<Roots>,
    ) -> Result<v1::SessionId, CallError> {
        let cwd = command.session_cwd().ok_or(CallError::NoWorkingDirectory)?;

        self.new_session(cwd, roots).await
    }

    /// Runs `initialize`, and answers with what the agent claims about itself.
    /// The claim is also stored in [`agent`](Self::agent).
    pub async fn initialize(&self) -> Result<v1::InitializeResponse, CallError> {
        self.client()?.initialize().await
    }

    /// `session/new` with this cwd, `roots`, and the current MCP draft; the
    /// session that answers becomes the live one (§7.5).
    ///
    /// With a session already live this is a switch: the in-flight turn is
    /// cancelled (not awaited), waiting permission requests are answered
    /// `cancelled`, and the timeline is discarded. The trace is untouched.
    /// Relative roots resolve against `cwd`; an empty list sends no field.
    /// The outcome lands in [`driven`](Self::driven) even when there is no
    /// connection.
    ///
    /// [`AgentCommand::session_cwd`]: crate::AgentCommand::session_cwd
    pub async fn new_session(
        &self,
        cwd: impl Into<PathBuf>,
        roots: Option<Roots>,
    ) -> Result<v1::SessionId, CallError> {
        let cwd = cwd.into();
        let mcp = self.mcp_draft().definitions_for(
            &self
                .agent()
                .map(|agent| agent.agent_capabilities.mcp_capabilities)
                .unwrap_or_default(),
        )?;
        match self.client() {
            Ok(client) => client.new_session(cwd, roots.as_ref(), mcp).await,
            Err(error) => {
                self.roots_refused(&roots::creating(roots.as_ref(), &cwd), &error);
                self.mcp_refused(&mcp, &error);
                Err(error)
            }
        }
    }

    /// Reopens a listed session with `session/load` or `session/resume`, as
    /// `how` says, and makes it the live one (§7.5).
    ///
    /// Sends what it is asked whatever the agent advertised
    /// ([`advertised_restore`](Self::advertised_restore) is the caller's gate).
    /// `roots: None` sends the roots the listing reported; relative paths
    /// resolve against the session's own `cwd`. MCP definitions come from the
    /// current draft.
    ///
    /// A switch, like [`new_session`](Self::new_session), except the timeline
    /// is discarded before the call because a load replays ahead of its answer.
    /// A refusal leaves the previous session live. The outcome lands in
    /// [`driven`](Self::driven) even when there is no connection.
    pub async fn restore_session(
        &self,
        session: &v1::SessionInfo,
        how: Restore,
        roots: Option<Roots>,
    ) -> Result<v1::SessionId, CallError> {
        let mcp = self.mcp_draft().definitions_for(
            &self
                .agent()
                .map(|agent| agent.agent_capabilities.mcp_capabilities)
                .unwrap_or_default(),
        )?;
        match self.client() {
            Ok(client) => client.restore(session, how, roots.as_ref(), mcp).await,
            Err(error) => {
                self.drive_refused(drives(how), &error);
                self.roots_refused(&roots::reopening(roots.as_ref(), session), &error);
                self.mcp_refused(&mcp, &error);
                Err(error)
            }
        }
    }

    /// Closes a session the agent holds: `session/close` (§7.5).
    ///
    /// Not gated here; [`closes_sessions`](Self::closes_sessions) is the
    /// caller's gate. The outcome lands in [`driven`](Self::driven) even when
    /// there is no connection.
    pub async fn close_session(&self, session: &v1::SessionId) -> Result<(), CallError> {
        match self.client() {
            Ok(client) => client.close_session(session).await,
            Err(error) => {
                self.drive_refused(AgentCapability::Close, &error);
                Err(error)
            }
        }
    }

    fn mcp_refused(&self, definitions: &[v1::McpServer], error: &CallError) {
        for definition in definitions {
            match definition {
                v1::McpServer::Http(_) => self.drive_refused(AgentCapability::McpHttp, error),
                v1::McpServer::Sse(_) => self.drive_refused(AgentCapability::McpSse, error),
                _ => {}
            }
        }
    }

    /// Deletes a session the agent holds: `session/delete` (§7.5).
    ///
    /// The listing is asked for again afterwards rather than edited. Not gated
    /// here ([`deletes_sessions`](Self::deletes_sessions) is the caller's
    /// gate); the outcome lands in [`driven`](Self::driven) either way.
    pub async fn delete_session(&self, session: &v1::SessionId) -> Result<(), CallError> {
        match self.client() {
            Ok(client) => client.delete_session(session).await,
            Err(error) => {
                self.drive_refused(AgentCapability::Delete, &error);
                Err(error)
            }
        }
    }

    /// Puts the live session in a mode: `session/set_mode` (§7.6).
    ///
    /// Nothing is applied optimistically: the mode in
    /// [`session_settings`](Self::session_settings) stays the agent's last
    /// statement, and an unrestated success is recorded as
    /// [`ModeChange::Acknowledged`](crate::ModeChange::Acknowledged). Sent even
    /// during a turn. A failure, including no connection, is recorded in the
    /// settings store.
    pub async fn set_mode(&self, mode: &v1::SessionModeId) -> Result<(), CallError> {
        match self.client() {
            Ok(client) => client.set_mode(mode).await,
            Err(error) => {
                self.0
                    .stores
                    .settings
                    .update(|settings| settings.mode_refused(mode.clone(), error.clone()));
                Err(error)
            }
        }
    }

    /// Sets one of the live session's config options:
    /// `session/set_config_option` (§7.6).
    ///
    /// The answer is the complete replacement option set, and
    /// [`session_settings`](Self::session_settings) is rebuilt from it alone.
    /// The value's variant decides the wire shape:
    /// [`v1::SessionConfigOptionValue::ValueId`] for a select,
    /// [`v1::SessionConfigOptionValue::Boolean`] for a boolean. A refusal,
    /// including no connection, is recorded as a
    /// [`ConfigRefusal`](crate::ConfigRefusal) and leaves the options as they
    /// were.
    pub async fn set_config_option(
        &self,
        option: &v1::SessionConfigId,
        value: &v1::SessionConfigOptionValue,
    ) -> Result<(), CallError> {
        match self.client() {
            Ok(client) => client.set_config_option(option, value).await,
            Err(error) => {
                self.0.stores.settings.update(|settings| {
                    settings.option_refused(option.clone(), value.clone(), error.clone())
                });
                Err(error)
            }
        }
    }

    /// Asks the agent what sessions it has: `session/list` (§7.1).
    ///
    /// `from` is a [`Cursor`] from a previous [`SessionListing`], or `None` to
    /// start over. Answers with the listing as it now stands (also in
    /// [`listing`](Self::listing)): a continuation is appended, a fresh ask
    /// replaces it. Not gated here ([`lists_sessions`](Self::lists_sessions)
    /// is the caller's gate); the ask is recorded in [`driven`](Self::driven).
    pub async fn list_sessions(&self, from: Option<Cursor>) -> Result<SessionListing, CallError> {
        match self.client() {
            Ok(client) => client.list_sessions(from).await,
            Err(error) => {
                self.drive_refused(AgentCapability::List, &error);
                Err(error)
            }
        }
    }

    /// Sends `authenticate` with one of the advertised methods (§7.1).
    ///
    /// Display-only: no login flow runs here. The outcome lands in
    /// [`auth`](Self::auth), including a failure for lack of a connection.
    pub async fn authenticate(&self, method: &v1::AuthMethodId) -> Result<(), CallError> {
        match self.client() {
            Ok(client) => client.authenticate(method).await,
            Err(error) => {
                self.0.stores.auth.set(AuthState::Refused {
                    method: method.clone(),
                    error: error.clone(),
                });
                Err(error)
            }
        }
    }

    /// Sends `logout` (§7.7).
    ///
    /// Connection-scoped; the live session is left alone. A success returns
    /// [`auth`](Self::auth) to [`AuthState::Unasked`]. Not gated here
    /// ([`logs_out`](Self::logs_out) is the caller's gate); the outcome lands
    /// in [`driven`](Self::driven) either way.
    pub async fn logout(&self) -> Result<(), CallError> {
        match self.client() {
            Ok(client) => client.logout().await,
            Err(error) => {
                self.drive_refused(AgentCapability::Logout, &error);
                Err(error)
            }
        }
    }

    /// Starts a turn: `session/prompt` with one text block (§7.1).
    ///
    /// Answers with the stop reason, which is also in [`turn`](Self::turn) by
    /// the time this returns; so is any failure.
    pub async fn prompt(&self, text: &str) -> Result<v1::StopReason, CallError> {
        self.prompt_content(vec![v1::ContentBlock::from(text)])
            .await
    }

    /// Starts a Turn with ordered text/image/audio/embedded-text/PDF blocks. Each attachment kind
    /// requires its Agent advertisement; the outgoing Frame is bounded.
    pub async fn prompt_content(
        &self,
        content: Vec<v1::ContentBlock>,
    ) -> Result<v1::StopReason, CallError> {
        self.submit_prompt(content)?
            .await
            .map_err(|_| CallError::Disconnected)?
    }

    /// Admit a prompt synchronously: the current Connection and Session are
    /// captured and the actual Frame validated before the draft may be cleared.
    /// Waiting for the Agent's outcome is separate from local admission.
    pub fn submit_prompt(
        &self,
        content: Vec<v1::ContentBlock>,
    ) -> Result<tokio::task::JoinHandle<Result<v1::StopReason, CallError>>, CallError> {
        match self.client() {
            Ok(client) => client.submit_prompt(content),
            Err(error) => {
                self.0.stores.turn.set(TurnState::Failed(error.clone()));
                Err(error)
            }
        }
    }

    /// Asks the agent to stop: `session/cancel` (§7.1).
    ///
    /// Returns as soon as the notification is on the wire — the turn is not
    /// over until the agent ends it, and a conformant agent ends it with
    /// [`StopReason::Cancelled`](v1::StopReason::Cancelled).
    pub async fn cancel(&self) -> Result<(), CallError> {
        self.client()?.cancel().await
    }

    /// Ends the connection: the agent stops, and the status says so.
    ///
    /// The stores keep what they hold. A connection that already ended keeps
    /// the status that reported it (e.g. `Lost`).
    ///
    /// Immediate: the agent's process group is killed before this returns,
    /// with no further executor ticks — the path for window and runtime
    /// teardown. It also ends any graceful stop still in progress.
    pub fn disconnect(&self) {
        self.0.release(ConnectionStatus::Disconnected, false);
        for stopping in std::mem::take(&mut *locked(&self.0.stopping)) {
            stopping.kill();
        }
    }

    /// Ends the connection like [`disconnect`](Self::disconnect), but asks the
    /// agent to stop rather than killing it: stdin is closed, then after a
    /// short grace its process group gets SIGTERM, then SIGKILL. Which step
    /// ended it is recorded as a [`DiagnosticKind::Stopped`](crate::DiagnosticKind::Stopped)
    /// line.
    ///
    /// For an explicit disconnect; a reconnect does the same to the old agent.
    pub fn disconnect_gracefully(&self) {
        self.0.release(ConnectionStatus::Disconnected, true);
    }

    /// Whether an agent released by [`disconnect_gracefully`](Self::disconnect_gracefully)
    /// (or a reconnect) is still being stopped.
    pub fn is_stopping(&self) -> bool {
        !locked(&self.0.stopping).is_empty()
    }

    /// Waits until no graceful stop is in progress, which is what a relaunch
    /// waits for so the old agent is gone before the next one starts. Bounded
    /// by the stop's own escalation to SIGKILL.
    pub async fn stopped(&self) {
        while self.is_stopping() {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// The calls this client sent on the live connection that have no answer
    /// yet, oldest first. Empty when there is no connection.
    pub fn pending_calls(&self) -> Vec<PendingCall> {
        self.client()
            .map(|client| client.pending_calls())
            .unwrap_or_default()
    }

    /// Sends `$/cancel_request` for a call still waiting on its answer. The
    /// call stays pending until the agent answers it, as the protocol asks it
    /// to (a result, or error -32800).
    pub async fn cancel_call(&self, id: i64) -> Result<(), CallError> {
        self.client()?.cancel_call(id).await
    }

    /// Sends a request or notification a person composed: `method`, and
    /// `params` exactly as written (only made one line), beside the typed layer.
    ///
    /// A request takes the next id from the shared sequence, is listed in
    /// [`pending_calls`](Self::pending_calls) until answered, and can be
    /// [cancelled](Self::cancel_call). The frame is marked
    /// [hand-sent](crate::Frame::is_hand_sent), and its answer drives none of
    /// the stores; read it off the trace with
    /// [`Trace::counterpart`](crate::Trace::counterpart) of
    /// [`HandSent::captured`].
    ///
    /// Returns once the frame is handed to the transport.
    pub async fn send_raw(
        &self,
        method: &str,
        params: Option<&serde_json::value::RawValue>,
        notification: bool,
    ) -> Result<HandSent, CallError> {
        let client = self.client()?;
        let rpc = client.rpc();
        if notification {
            let captured = rpc.notify_by_hand(method, params).await?;
            return Ok(HandSent { id: None, captured });
        }
        let call = rpc.prepare_by_hand(method, params)?;
        let id = call.id();
        let captured = rpc.send_unawaited(call).await?;
        Ok(HandSent {
            id: Some(id),
            captured,
        })
    }

    /// Records `additionalDirectories` as refused when roots were asked of an
    /// agent that was not there. An empty list sends no field, so drives nothing.
    fn roots_refused(&self, asked: &[PathBuf], error: &CallError) {
        if !asked.is_empty() {
            self.drive_refused(AgentCapability::AdditionalDirectories, error);
        }
    }

    /// Records an Advertisement as refused when it was driven at an agent that
    /// was not there. With no frame there is no reader event, so the caller
    /// writes the record (§7.7).
    fn drive_refused(&self, capability: AgentCapability, error: &CallError) {
        self.0
            .stores
            .driven
            .update(|record| record.refused(capability, error.clone()));
    }

    /// The typed conversation with the connected agent, or the news that there
    /// is none.
    fn client(&self) -> Result<Client, CallError> {
        self.0
            .connection()
            .as_ref()
            .map(|live| live.client.clone())
            .ok_or(CallError::Disconnected)
    }
}
