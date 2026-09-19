//! What the desktop shell is a rendering of (`docs/architecture.md` §5): one
//! connection at a time, the trace of its frames, the log of its diagnostics, a
//! status saying which of those is happening — and, above them, what the typed
//! layer made of it all: the agent's own account of itself, the session, the
//! timeline, and the turn.
//!
//! **The shell renders this and calls back into it, and does nothing else.**
//! Every behaviour the screens promise — a spawn form that starts an agent,
//! frames appearing as they arrive, stderr streaming, a failed spawn putting its
//! evidence in front of you, a connection that visibly ends, a turn that streams
//! and stops for a reason, an affordance that exists only because the agent
//! advertised it, an agent that will not go on until somebody logs in — is a
//! transition here, asserted in `crates/core/tests/inspector.rs`,
//! `crates/core/tests/typed.rs`, `crates/core/tests/capability.rs` and
//! `crates/core/tests/misbehaviour.rs` without a window (§12). That is the rule of
//! placement made concrete: if it cannot be tested without a window, it does not
//! belong in the window.
//!
//! The driven methods are the conformant path and only that (§7.1). They are
//! `async` and they answer with what the agent said, but the *screen* never
//! waits on them: everything they produce is in a store before they return, put
//! there by the one task reading the connection.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use agent_client_protocol_schema::v1;
use tokio_util::sync::CancellationToken;

use crate::auth::AuthState;
use crate::call::CallError;
use crate::client::{Client, Stores, drives};
use crate::command::AgentCommand;
use crate::connection::{
    ConnectionFactory, Diagnostic, DiagnosticKind, DiagnosticStream, FrameStream,
};
use crate::driven::{AgentCapability, DrivenRecord};
use crate::elicitation::ElicitationRequest;
use crate::listing::{Cursor, SessionListing};
use crate::permission::PermissionRequest;
use crate::restore::Restore;
use crate::roots::{self, Roots};
use crate::session_settings::SessionSettings;
use crate::store::{Changes, Field, Log};
use crate::timeline::Timeline;
use crate::trace::Trace;
use crate::turn::TurnState;

/// Where the connection to the agent stands (§9, the visible status).
///
/// It says *what happened to the connection*, never why: the reason is what the
/// diagnostic channel carries, line by line and timestamped, and a status that
/// tried to carry it too would be a worse copy of the console. What it does
/// distinguish is who ended the connection, because that is the difference
/// between news and an acknowledgement — and news is what the shell puts the
/// console in front of you for.
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
    /// Together with [`FailedToStart`](Self::FailedToStart) this is the pair
    /// the shell surfaces the diagnostic channel on (§9): both mean the agent
    /// is gone and nobody asked it to go, which is exactly when the reason is
    /// worth reading. An agent that dies three lines into its own startup is the
    /// common shape of "it didn't start" — `npx` resolving nothing, a bad
    /// argument, a missing key — and it reaches the user the same way a failed
    /// spawn does, rather than as a window that quietly went idle.
    Lost,
}

/// Everything the transport said that was not a frame, oldest first
/// (`CONTEXT.md`, *Diagnostic channel*).
///
/// The Console's Diagnostics tab is a rendering of this, and so is the evidence
/// behind a failed spawn: one channel, so an agent's own diagnostics and the
/// inspector's remarks about the connection sit in one timeline instead of two.
#[derive(Clone, Default)]
pub struct DiagnosticLog {
    entries: Log<Diagnostic>,
}

/// The inspector: an agent connection and the record of it.
///
/// A handle, not the state — cloning shares one inspector, which is what lets
/// the shell hand it to every callback that needs it without threading a
/// reference through the tree.
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
        self.0.release(ConnectionStatus::Disconnected);
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
}

/// The connection the inspector currently has, and the two things that end it.
struct Live {
    /// The typed conversation with this agent. It holds the frame sink, and a
    /// connection lives as long as a client end of it does — so holding this is
    /// what keeps the agent running until the inspector says otherwise, and
    /// dropping it is half of the teardown.
    client: Client,
    /// Stops the task holding the other two ends, so that dropping this whole
    /// value really is dropping the connection.
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
    /// **One connection at a time** (`CONTEXT.md`, *Connection*): whatever was
    /// connected is disconnected first, so the trace never has two agents
    /// writing into it at once.
    ///
    /// Never fails, for the reason the factory never does (§6.1): an agent that
    /// will not start is evidence, not an error return. The evidence lands in
    /// the diagnostic log and the status turns to
    /// [`FailedToStart`](ConnectionStatus::FailedToStart) — in that order, so
    /// that a console opened on the status is opened on the output.
    ///
    /// Spawns its pump onto the ambient async runtime, like the factory it
    /// drives, so this is called from within one.
    pub fn connect(&self, factory: &impl ConnectionFactory) {
        // Read before the teardown below clears it: what the timeline is a view
        // of, if it is a view of anything.
        let leaving = self.0.stores.session.get().is_some();
        self.disconnect();

        let connection = self.0.trace.tap(factory.connect());
        let (outgoing, incoming, diagnostics) = connection.into_parts();
        let shutdown = CancellationToken::new();
        let client = Client::new(outgoing, self.0.stores.clone());

        // A new agent has claimed nothing yet, and the last one's session is
        // not this one's.
        self.0.stores.agent.set(None);
        self.0.stores.session.set(None);
        self.0.stores.listing.set(None);
        self.0.stores.auth.set(AuthState::Unasked);
        self.0.stores.turn.set(TurnState::Idle);
        // Settings belong to a session, and this connection has none: what the
        // last agent published for the session it opened is not this one's
        // answer about anything (§7.6).
        self.0.stores.settings.set(SessionSettings::default());
        // And nothing has been driven on a connection that has just started
        // (§7.7). Cleared here and nowhere else: what an agent did when it was
        // driven stays readable after it has gone — the panel outlives the agent
        // it describes — so the record ends when the *next* agent claims
        // something, which is the moment it would start being about the wrong
        // one.
        self.0.stores.driven.set(DrivenRecord::default());
        // Nothing is pending on a new agent: the last one's requests were given
        // up on when it went away, which is the teardown's job and not this
        // one's.
        // **The view is emptied when the session it is a view of is left
        // behind.** A `session/new` on the same agent already did this
        // (`Client::new_session`), and a relaunch — which is a new agent, a new
        // connection and a new session — did not: the last agent's rows stayed
        // above the next one's, undifferentiated, under a composer that sends
        // to the new one. That is the larger of the two discontinuities keeping
        // the rows the smaller one clears, and the store's own rule is that it
        // is the view of the one live session (`Timeline::discard`).
        //
        // Guarded on there having *been* a session, so nothing is emptied that
        // was not a view of one: an agent that failed its handshake, or talked
        // before a session existed (§8), leaves rows a relaunch after it keeps.
        // The optimism above does not reach this — the line before it ended the
        // previous connection, so by the time a mistyped command gets here the
        // session those rows were a view of is over either way.
        //
        // Nothing that was evidence is lost: every frame behind every entry is
        // in the trace, which is untouched by any of this and marks where one
        // connection's traffic ends and the next's begins (§6.2).
        if leaving {
            self.0.stores.timeline.discard();
        } else {
            self.0.stores.timeline.reopen();
        }

        // Optimistic, and corrected within a moment if the agent was never
        // there: what a factory returns is a *started* connection, and the only
        // report of a start that failed comes up the diagnostic channel behind
        // it.
        self.0.status.set(ConnectionStatus::Connected);
        *self.0.connection() = Some(Live {
            client: client.clone(),
            shutdown: shutdown.clone(),
        });

        tokio::spawn(read(
            Arc::clone(&self.0.0),
            client,
            incoming,
            diagnostics,
            shutdown,
        ));
    }

    /// Starts an agent and opens the session a turn can happen in: connect,
    /// `initialize`, `session/new` (§7.1).
    ///
    /// **The whole handshake, because it is one thing the user asked for.** The
    /// Launch button is a button, and three round trips in a fixed order is
    /// protocol choreography — which lives here, where it is tested without a
    /// window, rather than in an event handler where each surface would have to
    /// get it right again (§5).
    ///
    /// The three steps are still separately callable, and stay that way: the
    /// capability display and the auth flow drive `initialize` on their own
    /// terms, and an agent that has to be logged into before it will open a
    /// session is exactly the shape that needs them apart.
    ///
    /// Answers with the session, and with the first thing that went wrong —
    /// including an agent that never started, which reaches this as
    /// [`Disconnected`](CallError::Disconnected) the moment the transport
    /// reports it, never as a wait. The connection is left standing either way:
    /// a handshake that failed is when the trace and the console are worth the
    /// most.
    pub async fn start(&self, command: &AgentCommand) -> Result<v1::SessionId, CallError> {
        // Snapshot before initialize: edits during the handshake belong to the
        // next opening, and an invalid draft must not replace a live Connection.
        let draft = self.mcp_draft();
        draft.definitions()?;
        // Before the spawn, because it is the one step that can fail without
        // the agent having anything to do with it, and starting a process to
        // then tell the user we cannot name a directory for it would be a
        // subprocess spent on a decision already made.
        let cwd = command.session_cwd().ok_or(CallError::NoWorkingDirectory)?;

        self.connect(&command.factory());
        let agent = self.initialize().await?;
        let mcp = draft.definitions_for(&agent.agent_capabilities.mcp_capabilities)?;
        // **No roots**, because the form that launches an agent has no control
        // that supplies any (§7.7): the control sits beside the `session/new`
        // button, where there is an agent that has said whether it advertises
        // `additionalDirectories`, and a launch is the one session opened before
        // anything is known about that.
        self.client()?.new_session(cwd, None, mcp).await
    }

    /// Opens another session on the agent this command launched, without
    /// relaunching it (§7.5).
    ///
    /// **The affordance's whole call**, and the handshake's own second step
    /// with the invocation still in hand: a session created on demand is opened
    /// in the working directory that invocation names, resolved exactly as
    /// [`start`](Self::start) resolves it — which is a decision with an answer
    /// ([`AgentCommand::session_cwd`]) and a failure ([`NoWorkingDirectory`]),
    /// and therefore one core makes once rather than one each surface makes
    /// again in an event handler (§5).
    ///
    /// Everything the switch costs is [`new_session`](Self::new_session)'s,
    /// which this is.
    ///
    /// `roots` is what the control beside the affordance holds, or `None` where
    /// the agent advertised no `additionalDirectories` for one to sit behind
    /// (§7.7) — which asks for none, because a session nobody has opened yet has
    /// no roots of its own to default to.
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

    /// Runs `initialize`, and answers with what the agent claims about itself
    /// (§7.1).
    ///
    /// The claim lands in [`agent`](Self::agent) as well as coming back here,
    /// because it is the first display artifact — agent info, capabilities,
    /// auth methods — and the screen showing it should not have to have been
    /// the caller.
    pub async fn initialize(&self) -> Result<v1::InitializeResponse, CallError> {
        self.client()?.initialize().await
    }

    /// Creates the session every turn happens in: `session/new` with this cwd
    /// and the current inspector-supplied MCP definitions (§7.1).
    ///
    /// The cwd is the spawn form's — [`AgentCommand::session_cwd`] turns what
    /// was typed into the absolute path ACP requires.
    ///
    /// **Callable whenever a connection is live, not only as part of launching**
    /// (§7.5): [`start`](Self::start) is one caller of this and an affordance on
    /// screen is another, which is what lets an agent be asked for a second
    /// session without being relaunched. The session that answers becomes the
    /// live one, so the composer is ready to prompt into what was just created.
    ///
    /// **Creating one while a session is already live is a switch**, and the
    /// switch happens before the call goes out: an in-flight turn is cancelled,
    /// every waiting permission request is answered `cancelled` — the sequence
    /// a `session/cancel` already owes them (§7.2) — and the timeline is
    /// discarded and rebuilt for the session that opens. There is no merge and
    /// no second timeline: the inspector holds one live session, and this store
    /// is the view of it. **The trace is untouched**, so a switch costs the view
    /// and never the evidence — every frame the discarded session was made of
    /// is where it was.
    ///
    /// It does not wait for the cancelled turn to end, because an agent that
    /// never ends one is exactly the sort this tool is pointed at. What answers
    /// that turn afterwards answers the *caller* who asked for it and nothing
    /// on screen: [`turn`](Self::turn) is the live session's.
    ///
    /// Answers with the new session, or with why there is not one — including
    /// [`Disconnected`](CallError::Disconnected) for a connection that has gone
    /// away, because an affordance whose agent left has to say so rather than
    /// do nothing.
    ///
    /// **The roots it opens with are the caller's** (§7.7), resolved against
    /// this same `cwd` — the base the schema names for a relative path, and not
    /// the inspector's own working directory. `None` asks for none, which is
    /// what an agent that never advertised `additionalDirectories` is asked for:
    /// the control the list comes from is gated on that advertisement, and
    /// nothing else about the call is.
    ///
    /// **What became of that ask is in [`driven`](Self::driven) either way** —
    /// including a click that reached no agent at all, which is a refusal for
    /// the reason [`close_session`](Self::close_session)'s is. An empty ask is
    /// no ask: the field is omitted when the list is empty, so it drives
    /// nothing whether it crossed or not.
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

    /// Opens a session the agent already has, and makes it the live one:
    /// `session/load` or `session/resume` (§7.5).
    ///
    /// **One operation with a property**, which is what
    /// [`Restore::replays`](crate::Restore::replays) is: the two calls are
    /// carry the same inputs (resume omits an empty MCP list) and differ in an ordering the
    /// specification states — load replays the conversation as `session/update`
    /// notifications before it answers, resume does not replay it at all. Which
    /// one this agent offers is [`advertised_restore`](Self::advertised_restore),
    /// and it is the caller's to hand back here: core sends what it was asked
    /// to send, the way [`list_sessions`](Self::list_sessions) does, because an
    /// agent's answer to a method it never advertised is evidence about the
    /// agent (§8).
    ///
    /// `session` is the listing's own description of it — the id, the working
    /// directory the session was opened in, and the roots it reported — so
    /// opening one is handing back what the agent said about it.
    ///
    /// **`roots` is the reported part the user may change** (§7.5, §7.7).
    /// MCP definitions come from the inspector's current draft, never the listing.
    /// `None` asks for the roots the listing reported, which is what a reopen
    /// does by default and what it does whatever the agent advertised: handing
    /// an agent its own words back is fidelity to the session being reopened
    /// rather than a claim about a capability. A control supplies them where
    /// the agent advertised `additionalDirectories`, prefilled with those same
    /// reported roots, and a list the user changed is sent as they changed it —
    /// the schema permits one that differs "as long as the request `cwd`
    /// matches the session's `cwd`", and what the agent answers is reported
    /// whatever it is. Relative paths are resolved against the session's own
    /// `cwd` either way, because a malformed request would put this client's
    /// violation into every annotation drawn against that agent.
    ///
    /// **Opening a session is a switch** and costs what
    /// [`new_session`](Self::new_session) costs: the in-flight turn cancelled,
    /// every waiting permission request answered `cancelled`, the timeline
    /// discarded and rebuilt, the trace untouched. One thing is ordered
    /// differently and it is the point of the call — the timeline is discarded
    /// *before* the ask, because a load replays ahead of its answer and a view
    /// emptied afterwards would be emptied of the replay.
    ///
    /// Answers with the session that is now live, or with why there is not one
    /// — an agent that refused, or one that has gone away. A refusal leaves the
    /// session it could not replace live, with whatever was replayed before it
    /// still readable.
    ///
    /// **What became of it is in [`driven`](Self::driven) either way**,
    /// including this failure and for the reason [`authenticate`](Self::authenticate)
    /// records its own: the record outlives the connection that filled it (§5),
    /// so a row that stayed blank because the click reached nothing would be
    /// reporting *not driven* about an attempt that was made (§7.7).
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
    /// **Gated on the advertisement and on nothing else** — the gate is
    /// [`closes_sessions`](Self::closes_sessions), and it is on the affordance
    /// rather than in here, exactly as [`list_sessions`](Self::list_sessions)'s
    /// is. Core sends what it was asked to send.
    ///
    /// Answers with the agent's answer, whatever it was — and **what became of
    /// it is in [`driven`](Self::driven) either way** (§7.7), including a click
    /// that reached no agent at all: the record outlives the connection that
    /// filled it (§5), so a row that stayed blank because there was nobody to
    /// ask would be reporting *not driven* about an attempt that was made.
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
    /// Gated on [`deletes_sessions`](Self::deletes_sessions) at the affordance,
    /// like every other call here. **Whether the session then leaves the
    /// listing is the agent's answer**, so the listing is asked for again
    /// rather than edited: agents differ on it, and what this tool reports has
    /// to be what the agent said rather than what deletion is supposed to mean.
    ///
    /// What became of it is in [`driven`](Self::driven) either way, which is
    /// [`close_session`](Self::close_session)'s rule for the other call.
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
    /// **Nothing is applied optimistically.** The mode
    /// [`session_settings`](Self::session_settings) reports is whatever the
    /// agent last stated, before this call and after it: `session/set_mode`
    /// answers `{}` and no rule obliges the agent to restate anything, so a
    /// success it never follows up on lands beside the mode as a
    /// [`ModeChange::Acknowledged`](crate::ModeChange::Acknowledged) rather
    /// than as a value the inspector invented. A `current_mode_update` settles
    /// it, and a refusal costs the attempt and nothing else.
    ///
    /// **The gate is the advertisement and it is on the affordance**, like
    /// every other call here: a surface offers this where the agent published
    /// modes, and core sends what it was asked to send — including into a live
    /// turn, which the specification leaves undefined and this tool therefore
    /// observes rather than prevents (§7.6).
    ///
    /// Answers with the agent's answer, whatever it was. **What a screen shows
    /// is in the store either way**, including this failure: the settings
    /// outlive the connection that filled them (§5), so a surface is still
    /// describing the session after its agent has gone — and a control pressed
    /// there has to read as an attempt that failed rather than as a button with
    /// nothing behind it. The same rule [`prompt`](Self::prompt) and
    /// [`authenticate`](Self::authenticate) follow, for the same reason.
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
    /// **The agent's word, taken entirely.** The answer to this call is the
    /// **complete replacement option set**, so
    /// [`session_settings`](Self::session_settings) is rebuilt from the list
    /// the agent returned and from nothing else: an agent that answers a single
    /// write with an option set differing in more than the option that was
    /// written has that whole set reflected. Nothing here merges an answer into
    /// what was held, patches the option that was set, or assumes it was the
    /// only one that moved.
    ///
    /// **The value says what shape it is, and both shapes are this method's.**
    /// A select's write carries a bare value id
    /// ([`v1::SessionConfigOptionValue::ValueId`]) and a boolean's carries the
    /// boolean under a `type` discriminator
    /// ([`v1::SessionConfigOptionValue::Boolean`]) — the discriminator
    /// describes the shape of a *value*, not the kind of an option, so it is
    /// the value that decides it and not the caller. A boolean option is on the
    /// wire at all only because this client advertised
    /// `clientCapabilities.session.configOptions.boolean` (§7.3): a conformant
    /// agent must not publish one to a client that did not.
    ///
    /// **The gate is the advertisement and it is on the affordance**, and a set
    /// into a live turn is permitted — [`set_mode`](Self::set_mode) argues
    /// both. A refusal costs the attempt and nothing else, whatever the kind of
    /// the option: the options stay where the agent's last statement left them,
    /// and what it said lands beside them as a
    /// [`ConfigRefusal`](crate::ConfigRefusal), including the failure of a set
    /// nobody was left to answer.
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
    /// `from` is a [`Cursor`] a previous [`SessionListing`] carried, or `None`
    /// to ask from the start — and a cursor is a token that can only be handed
    /// back, never read ([`Cursor`] says why). Answers with the listing
    /// as it now stands, which is also what [`listing`](Self::listing) holds:
    /// a continuation is folded into what it continues, a fresh ask replaces it.
    ///
    /// **Handing the token back is the whole of the paging** the spec left out
    /// (§7.1: "paging ignored ... cursor treated as opaque if one appears").
    /// Nothing here counts pages, remembers how many there were, prefetches, or
    /// looks inside a cursor: an agent that says there is more can be asked for
    /// it, and that is the end of what this knows about paging.
    ///
    /// **The gate is [`lists_sessions`](Self::lists_sessions), and it is on the
    /// affordance rather than in here.** A screen offers this when the agent
    /// advertised it; core does not refuse to send what it was asked to send,
    /// because an agent's answer to a method it never advertised is evidence
    /// about the agent and refusing to ask for it would be the inspector
    /// deciding what may be observed (§8).
    ///
    /// **A listing asked for here is one somebody asked for**, and it is on the
    /// record as such (§7.7). The listing this client refreshes on its own
    /// initiative after a close or a delete goes out through the same call and
    /// records nothing, because a row marked by the tool's own housekeeping
    /// would be one the user never drove and could not clear by listing again
    /// themselves.
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
    /// **Display-only** (§1.1): this is the call and the state it leaves
    /// behind, in [`auth`](Self::auth). The inspector runs no login — the flows
    /// real agents use are the agent's own, in the user's own terminal — so
    /// what a screen does with `-32000 auth_required` is say so and name the
    /// methods, which is what [`AuthState`] and [`agent`](Self::agent) are
    /// between them.
    ///
    /// **The outcome is in the store either way**, including this one: the
    /// stores outlive the connection that filled them (§5), so an agent that
    /// went away still has a panel on screen — and a login attempted at one
    /// that is no longer there has to read as an attempt that failed rather
    /// than as a button with nothing behind it. The same rule
    /// [`prompt`](Self::prompt) follows, for the same reason.
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

    /// Sends `logout`: the agent is asked to give back what it accepted (§7.1,
    /// §7.7).
    ///
    /// **Connection-scoped.** No session id crosses, and the live session is
    /// left strictly alone — nothing is cancelled and nothing is closed ahead of
    /// it, because what the agent does to a session it has logged out of is what
    /// there is to find out and pre-empting it would answer that on the agent's
    /// behalf.
    ///
    /// **The gate is [`logs_out`](Self::logs_out), and it is on the affordance**
    /// like every other one here — never on what this tool believes the agent's
    /// state makes sensible. A logout with nobody logged in is a corner the
    /// specification leaves entirely undefined, which is the reason to drive it
    /// rather than a reason to withhold the button.
    ///
    /// **A success returns [`auth`](Self::auth) to [`AuthState::Unasked`]** —
    /// the agent has retracted what it accepted — and leaves the login screen
    /// down, because the agent has asked for no login and this tool reports what
    /// the agent said rather than what the schema expects it to say next.
    ///
    /// **What became of it is in [`driven`](Self::driven) either way**,
    /// including this failure: the panel outlives the agent it describes (§5),
    /// so a logout asked of one that is no longer there has to read as an
    /// attempt that failed rather than as a button that did nothing.
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
    /// Answers with the agent's stop reason, which is also in
    /// [`turn`](Self::turn) by the time this returns — and gets there whether
    /// or not anyone is awaiting this call. So does a failure, including this
    /// one: a turn asked for on an agent that is no longer there never reaches
    /// a client, and a composer whose button left no trace anywhere would be
    /// the screen keeping a secret the store is there to tell.
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
    /// The stores keep what they hold. Stopping an agent is not a reason to
    /// lose the record of what it did — that is what an inspector is for — and
    /// clearing the trace is a thing the user asks for, alongside the export
    /// that makes it a decision (§10, §15 q3).
    ///
    /// A connection that already ended by itself leaves nothing to disconnect,
    /// and this says nothing over the status that reported it: the news that an
    /// agent was lost has to survive a click.
    pub fn disconnect(&self) {
        self.0.release(ConnectionStatus::Disconnected);
    }

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

    /// Where the turn stands — a field the stream drives, never a bracket
    /// around the prompt call (§11 seam 2).
    pub fn turn(&self) -> TurnState {
        self.0.stores.turn.get()
    }

    pub fn turn_changes(&self) -> Changes<TurnState> {
        self.0.stores.turn.changes()
    }

    /// Says in the record that `additionalDirectories` was asked for at an agent
    /// that was not there (§7.7).
    ///
    /// **An empty ask is no ask**, which is the frame-reading rule where there
    /// is no frame to read: the field is omitted when the list is empty, so a
    /// call that would have carried nothing drove nothing whether it crossed or
    /// not. Where the list was not empty this is the same refusal the call's own
    /// Advertisement gets beside it — the attempt was made, and a row left
    /// reading *not driven* would be the panel keeping a secret this record
    /// exists to tell.
    fn roots_refused(&self, asked: &[PathBuf], error: &CallError) {
        if !asked.is_empty() {
            self.drive_refused(AgentCapability::AdditionalDirectories, error);
        }
    }

    /// Says in the record that an Advertisement was driven at an agent that was
    /// not there — which is a refusal and not a fourth outcome (§7.7).
    ///
    /// **The caller's half of the write path**, and the one place the affordances
    /// that have one state it: a call that never became a frame has no reader
    /// event to write it, so whoever asked does — the shape
    /// [`authenticate`](Self::authenticate) and the two setters already have.
    /// A failure the client itself met is the client's to record, because by
    /// then there is an id to guard the write with.
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

impl DiagnosticLog {
    pub fn entries(&self) -> Vec<Diagnostic> {
        self.entries.entries()
    }

    /// The lines it holds and the count of the ones it does not, from one
    /// moment — [`Trace::snapshot`](crate::Trace::snapshot)'s reason, for the
    /// other log a window renders.
    ///
    /// The second number is `0` today and the accessor still hands it over:
    /// this log is unbounded for now (§15 q8 is the volume question, still
    /// open), and a console that keyed its lines by position alone would be a
    /// console that quietly starts mixing rows up on the day it is not. A line's
    /// identity is the ordinal it was captured under, whether or not anything
    /// has been dropped yet.
    pub fn snapshot(&self) -> (Vec<Diagnostic>, usize) {
        self.entries.snapshot()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Watches the log: how many lines there are now, and a wait for the next.
    pub fn changes(&self) -> Changes<usize> {
        self.entries.changes()
    }
}

impl State {
    /// Records one diagnostic, and lets it move the status.
    ///
    /// **The connection's whole lifecycle is on this channel** (§6.1): a start
    /// that failed, an agent that exited, a pipe that broke. Reading the status
    /// off the diagnostics rather than off "the streams went quiet" is what
    /// keeps the two agreeing — an agent that closes its stdout and keeps
    /// running has not disconnected, and the transport says so by not saying so.
    ///
    /// The log first, then the status, then the teardown, always: the shell
    /// opens the console *on the captured output* (§9), so the output has to be
    /// there by the time the status tells it to look — and the teardown is what
    /// wakes everyone waiting on an answer, so the status has to be there by
    /// the time they ask why.
    fn observe(&self, diagnostic: Diagnostic, shutdown: &CancellationToken) {
        let mut connection = self.connection();
        // A reader can wake after reconnect. Its evidence and teardown belong
        // only to the Connection whose token it holds.
        if shutdown.is_cancelled() {
            return;
        }
        let status = match diagnostic.kind {
            DiagnosticKind::SpawnFailed { .. } => Some(ConnectionStatus::FailedToStart),
            // The agent is gone either way; which way is what the console says.
            DiagnosticKind::AgentExited(_) | DiagnosticKind::TransportFailed(_) => {
                Some(ConnectionStatus::Lost)
            }
            // Everything the agent itself says, and the one hole the transport
            // admits to. Neither is news about the connection.
            _ => None,
        };

        self.diagnostics.entries.append(diagnostic);
        if let Some(status) = status {
            // Nothing is on the other end any more, so nothing here should be
            // holding a connection to it: "connected" and "holding a
            // connection" are the same fact, and this is where they stay one.
            self.release_locked(&mut connection, status);
        }
    }

    /// Lets go of the connection, if there is one, and says so — in the status,
    /// and to everyone waiting on an answer. A connection that is already gone
    /// leaves nothing to do and nothing to say over what said it went.
    ///
    /// Both ends of the teardown: the reader lets go of the stream and the
    /// channel, the [`Live`] lets go of the client that holds the sink, and the
    /// last of them going away is what tells the transport to end the agent
    /// (§6.1).
    ///
    /// **The status moves under the connection's own lock**, before anything is
    /// woken. That is what keeps the two agreeing from any angle: a caller told
    /// its call will not be answered can read a status that already says why,
    /// and a `disconnect` racing an agent's last breath either finds the
    /// connection gone — leaving the news standing, because it has to survive a
    /// click — or gets there first and this reports nothing over it.
    ///
    /// Everything still waiting on an answer is then told there will not be
    /// one. A call left pending across a teardown is the hang this design is
    /// against, and a turn is the one that would be visible: it would sit on
    /// the screen looking like it was still running.
    fn release(&self, status: ConnectionStatus) {
        self.release_locked(&mut self.connection(), status);
    }

    fn release_locked(&self, live: &mut Option<Live>, status: ConnectionStatus) {
        if let Some(connection) = live.take() {
            self.status.set(status);
            connection.client.disconnected();
            connection.shutdown.cancel();
        }
    }

    /// Poll only while this reader owns the stores. The lock is held for each
    /// synchronous poll, never over an await: reconnect cannot reset the stores
    /// halfway through an old Frame being recorded or decoded.
    async fn while_connected<F: std::future::Future>(
        &self,
        shutdown: &CancellationToken,
        future: F,
    ) -> Option<F::Output> {
        tokio::pin!(future);
        std::future::poll_fn(|cx| {
            let _connection = self.connection();
            if shutdown.is_cancelled() {
                return std::task::Poll::Ready(None);
            }
            future.as_mut().poll(cx).map(Some)
        })
        .await
    }

    fn connection(&self) -> MutexGuard<'_, Option<Live>> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Drains a connection into the stores until the inspector lets go of it.
///
/// One task for both ends rather than one each: a diagnostic is what moves the
/// status, and a frame is what the trace and the timeline are made of, so
/// keeping them in one place is what makes "the log has the evidence before the
/// status points at it" a fact about this function rather than a hope about
/// two.
///
/// **This is the only reader**, which is what makes the stores trustworthy: a
/// frame reaches the timeline, settles a turn and answers a call from here and
/// nowhere else, in the order the wire had them. Whoever awaits a driven method
/// is told after the stores already know.
///
/// It outlives the agent on purpose. An exited agent still has stderr to drain
/// and an exit status to report, and a diagnostic channel that stays open as
/// long as the client holds a sink is the transport keeping that promise —
/// which is why nothing here concludes anything from a stream going quiet.
async fn read(
    state: Arc<State>,
    client: Client,
    mut incoming: FrameStream,
    mut diagnostics: DiagnosticStream,
    shutdown: CancellationToken,
) {
    let mut frames = true;
    let mut lines = true;
    let mut exit = None;

    while frames || lines {
        tokio::select! {
            () = shutdown.cancelled() => return,
            frame = state.while_connected(&shutdown, incoming.recv()), if frames => match frame {
                // Reading *is* the recording: a frame is in the trace by the
                // time this has it (§6.2), so everything the typed layer does
                // with it next is decoration over a record already made.
                //
                // Under the token as well, because answering a request means
                // sending, and sending waits for room in a pipe an agent that
                // has stopped reading its stdin will never make. A teardown
                // that could not interrupt that would be a stop button that
                // does not stop the one kind of agent worth stopping.
                Some(Some(frame)) => tokio::select! {
                    biased;
                    () = shutdown.cancelled() => return,
                    _ = state.while_connected(&shutdown, client.receive(frame)) => {}
                },
                Some(None) => frames = false,
                None => return,
            },
            diagnostic = diagnostics.recv(), if lines && exit.is_none() => match diagnostic {
                Some(diagnostic) if matches!(diagnostic.kind, DiagnosticKind::AgentExited(_)) => {
                    // The transport finished producing Frames, but a separate
                    // channel means its exit can overtake the queued tail.
                    exit = Some(diagnostic);
                }
                Some(diagnostic) => state.observe(diagnostic, &shutdown),
                None => lines = false,
            },
        }
        if !frames && let Some(diagnostic) = exit.take() {
            state.observe(diagnostic, &shutdown);
            return;
        }
    }
}
