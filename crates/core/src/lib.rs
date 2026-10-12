//! The headless core of the ACP Inspector: everything the tool does that is not
//! rendering. See `docs/architecture.md` §5.
//!
//! Three layers:
//!
//! - The **frame layer**: [`StdioSpawn`] spawns an agent and exchanges whole
//!   JSON-RPC frames with it over stdio, reporting everything else through a
//!   diagnostic channel. [`Trace`] wraps it and records every frame, both
//!   directions, before anything above can see it.
//! - The **typed layer** drives the conformant ACP client path (`initialize`,
//!   sessions, prompts, the setters, auth) and reads what comes back as v1 into
//!   stores: a [`Timeline`], a [`TurnState`], an [`AuthState`], the live
//!   [`SessionSettings`] (with [`ModeChange`] and [`ConfigRefusal`] beside them),
//!   and a [`DrivenRecord`] of what became of each [`AgentCapability`] that was
//!   driven. Requests the agent blocks on — such as a [`PermissionRequest`] —
//!   land on the timeline carrying their own resolver.
//! - The [`Inspector`] is what the desktop shell renders: one connection at a
//!   time and every store above it, subscribed to rather than polled.
//!
//! The trace is raw-first: the typed layer only decorates it, and traffic it
//! does not recognize becomes an [`Unrecognized`] entry carrying its own JSON
//! (§8). [`Trace::export`] writes it as an [`Export`] (JSONL, see
//! `docs/trace-export.md`). Where a spec MUST is decidable from captured frames,
//! an [`Annotation`] lands on the timeline beside the evidence (§15 q7).
//!
//! [`RecentCommands`] and [`Settings`] (an [`Appearance`] and an
//! [`Indentation`]) are the inspector's own small state files. [`Restore`] names
//! the two ways to reopen a session (`session/load`, `session/resume`).
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
//! re-exported so surfaces above core read the same types core decodes.

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
mod import;
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
mod script;
mod service;
mod services;
mod session_report;
mod session_settings;
mod settings;
mod stdio;
mod store;
mod timeline;
mod trace;
mod turn;
mod unwind;

/// The protocol, as this crate speaks it. The typed layer decodes as v1
/// (§11 seam 1); [`ProtocolVersion`] is shared across versions.
pub use agent_client_protocol_schema::{MaybeUndefined, ProtocolVersion, v1};

pub use appearance::Appearance;
pub use auth::AuthState;
pub use blocking::RequestId;
pub use call::CallError;
pub use client::{client_capabilities, client_info};
pub use command::{AgentCommand, MINIMAL_ENV};
pub use conformance::{Annotation, Ending, OptionSet, Replay, Reuse};
pub use connection::{
    Connection, ConnectionFactory, Diagnostic, DiagnosticKind, DiagnosticStream, FrameSink,
    FrameStream, SendError, Stop, TransportEnds, connection,
};
pub use driven::{AgentCapability, Driven, DrivenRecord};
pub use elicitation::{ElicitationAnswer, ElicitationRequest, ElicitationState};
pub use export::Export;
pub use frame::{Direction, Frame, FrameKind, FrameSummary, Omission};
pub use import::{ImportError, Imported};
pub use indentation::Indentation;
pub use inspector::{ConnectionStatus, DiagnosticLog, HandSent, Inspector};
pub use listing::{Cursor, SessionListing};
pub use permission::{PermissionPolicy, PermissionRequest, PermissionState};
pub use recent::RecentCommands;
pub use restore::Restore;
pub use roots::Roots;
pub use rpc::PendingCall;
pub use script::{Rule, ScriptError, Scripted, ServiceScript, Unmatched, glob};
pub use service::{
    AnsweredBy, ServiceAnswer, ServiceReply, ServiceRequest, ServiceState, read_json, skeleton,
};
pub use services::{ClientClaim, ClientServices};
pub use session_report::{SessionReport, SessionReports, Stated, StatedBy};
pub use session_settings::{ConfigRefusal, ModeChange, SessionSettings};
pub use settings::Settings;
pub use stdio::StdioSpawn;
pub use store::Changes;
pub use timeline::{
    EntryId, EntryKind, Recorded, Retention, Timeline, TimelineEntry, Turn, Unrecognized,
};
pub use trace::{Counterpart, Trace, TracedFrame};
pub use turn::{TurnOutcome, TurnState};
