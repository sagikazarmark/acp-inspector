//! The headless core of the ACP Inspector: everything the tool does that is not
//! rendering (`docs/architecture.md` §5).
//!
//! Three layers. The **frame layer** (§14 step 1) is the bottom: point
//! [`StdioSpawn`] at an agent command and it spawns the agent, exchanges whole
//! JSON-RPC frames with it over stdio, and reports everything else it sees
//! through a diagnostic channel; [`Trace`] wraps it and records every frame,
//! both directions, before anything above the seam can see it. The **typed
//! layer** (§14 step 3) sits on top: it drives the conformant path —
//! `initialize`, `session/new`, `session/prompt`, `session/cancel`, and the
//! ones the agent's own claims gate: `session/list`, `authenticate`, the
//! `logout` that gives back what an `authenticate` was accepted with, the two
//! that open a session it already has — `session/load` and `session/resume`,
//! which are one operation with a [`Restore`] property — and the two that end
//! one, `session/close` and `session/delete`, which are two operations and stay
//! two. It reads what comes back *as v1*, filling a [`Timeline`] whose entities
//! are id-keyed, a [`TurnState`] the stream drives, an [`AuthState`] that
//! turns a `-32000 auth_required` into something a screen can say, and the
//! [`SessionSettings`] a session setup response publishes — the live session's
//! modes and its config options, which the agent's own announcements keep
//! current and which the two setters drive. `session/set_mode` answers nothing,
//! so what the agent said about the ask ([`ModeChange`]) sits beside a mode this
//! crate never moves on its own; `session/set_config_option` answers with the
//! whole option set, so the store is rebuilt from it and only a refusal
//! ([`ConfigRefusal`]) is left to report. Above all of them it keeps a
//! [`DrivenRecord`]: what became of driving each [`AgentCapability`] on this
//! connection — not driven, answered, or refused with the agent's own error —
//! passively, because what fills it is the affordances the user already pressed.
//! It services one thing the agent asks of it, which is the one an ACP turn is
//! made of: a
//! `session/request_permission` becomes a [`PermissionRequest`] on the timeline
//! where the turn stopped for it, carrying its own resolver, and the turn goes
//! on when somebody answers it. The [`Inspector`] is what the desktop shell
//! renders (§14 step 2): one connection at a time and every store above it,
//! subscribed to rather than polled, and nothing that needs a window to be
//! true.
//!
//! The trace is also what leaves the program: [`Trace::export`] takes it as an
//! [`Export`] — JSONL, one record per frame, behind a self-describing header —
//! which is the seed of the event log the wider ecosystem is waiting on, and a
//! pinned contract because of it (§10, `docs/trace-export.md`).
//!
//! Two things here are neither a layer nor a store, and both are files of the
//! inspector's own (§9): [`RecentCommands`] is what the spawn form remembers
//! between runs (§15 q6) — the last few invocations whose agent answered
//! `initialize` — and
//! [`Settings`] is what the window remembers about how it should look: an
//! [`Appearance`], and an [`Indentation`] that says whether a payload is drawn as
//! it crossed or spaced out to be read — the *window's* settings, and never the
//! live session's, which are [`SessionSettings`] and belong to an agent rather
//! than to a file. Indenting is a reading aid and stops there: it adds whitespace
//! to text already recorded, and the trace, the export and the clipboard all
//! carry the bytes that crossed (§8).
//! Both are in core because what is stored, what an unknown value falls back to
//! and what a broken file means are decisions with tests rather than a window.
//!
//! Where the specification says **MUST** and the frames already captured can
//! say whether it was met, it says so: an [`Annotation`] lands on the timeline
//! beside the traffic that decides it (§15 q7). That is a boundary and not a
//! feature — an annotation is admitted only where a MUST meets a decidable
//! violation, so there is no rule engine, no severity, no verdict, and nothing
//! is said at all about behaviour the specification leaves undefined. Three
//! rules are admitted today: a cancelled turn that did not resolve `cancelled`,
//! a `session/load` that answered without replaying the conversation, and a
//! `session/set_config_option` that succeeded and answered with an option set
//! the id just written is not in.
//!
//! The order of those layers is the design. The trace sits *below* the types,
//! so what it records is the wire rather than a decoding of it, and the typed
//! layer only ever decorates it: traffic the inspector does not recognize
//! becomes a first-class [`Unrecognized`] entry carrying its own JSON, because
//! misbehaviour is the subject matter and not the error case (§8).
//!
//! ```no_run
//! # use acp_inspector_core::{AgentCommand, Inspector, TurnState};
//! # async fn drive() {
//! let command = AgentCommand { command: "my-agent".into(), ..AgentCommand::default() };
//! let inspector = Inspector::new();
//!
//! inspector.connect(&command.factory());
//! let agent = inspector.initialize().await.expect("it started");
//! println!("{:?} offers {:?}", agent.agent_info, agent.agent_capabilities);
//!
//! let cwd = command.session_cwd().expect("a working directory to name");
//! inspector.new_session(cwd, None).await.expect("a session");
//! let stop = inspector.prompt("hello").await.expect("a turn");
//!
//! assert_eq!(inspector.turn(), TurnState::Ended(stop));
//! for entry in inspector.timeline().entries() {
//!     println!("{:?}", entry.kind);
//! }
//! # }
//! ```
//!
//! Its only protocol dependency is the ACP schema crate, whose [`v1`] module is
//! re-exported here so that a surface above core reads the same types core
//! decodes. No UI types, no Dioxus, no dependency on the host repo's
//! `dioxus-agent-ui*` crates and not the ACP SDK: the app is additive, one
//! crate with a feature per renderer, which is what keeps a web surface a
//! second feature rather than a redesign (§1, §3, §5, ADR 0011).

mod appearance;
mod auth;
mod blocking;
mod call;
mod client;
mod command;
mod conformance;
mod connection;
mod decode;
mod driven;
mod elicitation;
mod export;
mod frame;
mod indentation;
mod inspector;
mod kept;
mod listing;
mod mcp;
pub use mcp::{McpDraft, McpEnv, McpFinding, McpServerDraft, McpTransport};
mod permission;
mod recent;
mod restore;
mod roots;
mod rpc;
mod session_settings;
mod settings;
mod stdio;
mod store;
mod timeline;
mod trace;
mod turn;

/// The protocol, as this crate speaks it.
///
/// Re-exported rather than restated: the typed layer decodes *as v1* (§11 seam
/// 1), so the version is part of what core's API says, and the day a `v2`
/// module joins it here is the day the seam is used. [`ProtocolVersion`] sits
/// beside it rather than inside it, in the schema crate and here, because which
/// dialect is being spoken is not a statement in any one of them.
pub use agent_client_protocol_schema::{ProtocolVersion, v1};

pub use appearance::Appearance;
pub use auth::AuthState;
pub use blocking::RequestId;
pub use call::CallError;
pub use client::{client_capabilities, client_info};
pub use command::AgentCommand;
pub use conformance::{Annotation, Ending, OptionSet, Replay, Reuse};
pub use connection::{
    Connection, ConnectionFactory, Diagnostic, DiagnosticKind, DiagnosticStream, FrameSink,
    FrameStream, SendError, TransportEnds, connection,
};
pub use driven::{AgentCapability, Driven, DrivenRecord};
pub use elicitation::{ElicitationAnswer, ElicitationRequest, ElicitationState};
pub use export::Export;
pub use frame::{Direction, Frame, FrameKind, FrameSummary};
pub use indentation::Indentation;
pub use inspector::{ConnectionStatus, DiagnosticLog, Inspector};
pub use listing::{Cursor, SessionListing};
pub use permission::{PermissionRequest, PermissionState};
pub use recent::RecentCommands;
pub use restore::Restore;
pub use roots::Roots;
pub use session_settings::{ConfigRefusal, ModeChange, SessionSettings};
pub use settings::Settings;
pub use stdio::StdioSpawn;
pub use store::Changes;
pub use timeline::{EntryId, EntryKind, Recorded, Timeline, TimelineEntry, Turn, Unrecognized};
pub use trace::{Trace, TracedFrame};
pub use turn::{TurnOutcome, TurnState};
