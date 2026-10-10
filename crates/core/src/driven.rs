//! What was driven on this connection, and what came back
//! (`docs/architecture.md` §7.7).
//!
//! One outcome per Agent Capability for the live connection — not driven,
//! answered, or refused with the agent's own error — so an agent that refuses
//! every call to a claimed method is told apart from one nobody tried. A
//! connection that died under a call counts as refused.
//!
//! Passive: it records what the user's own actions caused, and the listing the
//! tool refreshes by itself writes nothing. The latest drive of a capability
//! replaces its previous outcome.
//!
//! Kept as a field rather than derived from the trace, because the trace is a
//! ring that drops its oldest frames.

use std::collections::HashMap;

use agent_client_protocol_schema::v1;

use crate::call::CallError;

/// One thing the agent claimed about itself in its `initialize` result
/// (`CONTEXT.md`, *Agent Capability*), named so a record can be keyed on it.
///
/// The capability display keeps its own list (`crates/app/src/agent.rs`); both
/// are checked against the schema in `crates/core/tests/capability.rs`. Rows look
/// records up by variant, never by string, so a new upstream capability breaks
/// the build instead of rendering as unknown.
///
/// `authenticate` is not here; its outcome is [`AuthState`](crate::AuthState).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentCapability {
    /// `session/load`, claimed by the top-level `loadSession` boolean.
    Load,
    /// `session/list`, claimed by the presence of `sessionCapabilities.list`.
    List,
    /// `session/resume`, claimed by the presence of `sessionCapabilities.resume`.
    Resume,
    /// `session/close`, claimed by the presence of `sessionCapabilities.close`.
    Close,
    /// `session/delete`, claimed by the presence of `sessionCapabilities.delete`.
    Delete,
    /// The `additionalDirectories` a session may be opened with, claimed by the
    /// presence of the capability object of the same name.
    AdditionalDirectories,
    /// `logout`, claimed by the presence of `agentCapabilities.auth.logout`.
    /// Connection-scoped: the request carries no session id.
    Logout,
    /// Image content in `session/prompt`, claimed by promptCapabilities.image.
    Image,
    /// Audio content in `session/prompt`, claimed by promptCapabilities.audio.
    Audio,
    /// Embedded content in `session/prompt`, claimed by promptCapabilities.embeddedContext.
    EmbeddedContext,
    /// HTTP definitions supplied in a Session-opening call, not MCP connectivity.
    McpHttp,
    /// SSE definitions supplied in a Session-opening call, not MCP connectivity.
    McpSse,
}

/// What became of driving an Agent Capability on this connection (§7.7).
/// A connection that went away holding the call counts as
/// [`Refused`](Self::Refused).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Driven {
    /// Nothing has driven this Advertisement on this connection.
    #[default]
    Unasked,
    /// It was driven, and the agent answered — even with a result v1 could not
    /// decode.
    Answered,
    /// It was driven and there was no answer: the agent refused (its error is
    /// carried whole), or the connection went away holding the call.
    Refused(CallError),
}

/// What has been driven on this connection, and what came back (§7.7), as one
/// read of [`Inspector::driven`](crate::Inspector::driven).
///
/// Cleared when another agent connects; kept after the agent exits and across
/// session switches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DrivenRecord {
    /// Only driven capabilities; an absent key is [`Driven::Unasked`].
    outcomes: HashMap<AgentCapability, Driven>,
}

impl AgentCapability {
    /// Every capability this record can be keyed on, in the order the display
    /// draws them.
    pub const ALL: [Self; 12] = [
        Self::Load,
        Self::List,
        Self::Resume,
        Self::Close,
        Self::Delete,
        Self::AdditionalDirectories,
        Self::Logout,
        Self::Image,
        Self::Audio,
        Self::EmbeddedContext,
        Self::McpHttp,
        Self::McpSse,
    ];

    /// The protocol's name for it: the method it gates, or the field name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Load => v1::AGENT_METHOD_NAMES.session_load,
            Self::List => v1::AGENT_METHOD_NAMES.session_list,
            Self::Resume => v1::AGENT_METHOD_NAMES.session_resume,
            Self::Close => v1::AGENT_METHOD_NAMES.session_close,
            Self::Delete => v1::AGENT_METHOD_NAMES.session_delete,
            Self::AdditionalDirectories => "additionalDirectories",
            Self::Logout => v1::AGENT_METHOD_NAMES.logout,
            Self::Image => "image",
            Self::Audio => "audio",
            Self::EmbeddedContext => "embeddedContext",
            Self::McpHttp => "http",
            Self::McpSse => "sse",
        }
    }

    /// Whether this agent claimed it in its `initialize` capabilities, as a
    /// boolean or as the presence of an object, whichever v1 uses.
    #[must_use]
    pub fn advertised(self, capabilities: &v1::AgentCapabilities) -> bool {
        let sessions = &capabilities.session_capabilities;
        match self {
            Self::Load => capabilities.load_session,
            Self::List => sessions.list.is_some(),
            Self::Resume => sessions.resume.is_some(),
            Self::Close => sessions.close.is_some(),
            Self::Delete => sessions.delete.is_some(),
            Self::AdditionalDirectories => sessions.additional_directories.is_some(),
            Self::Logout => capabilities.auth.logout.is_some(),
            Self::Image => capabilities.prompt_capabilities.image,
            Self::Audio => capabilities.prompt_capabilities.audio,
            Self::EmbeddedContext => capabilities.prompt_capabilities.embedded_context,
            Self::McpHttp => capabilities.mcp_capabilities.http,
            Self::McpSse => capabilities.mcp_capabilities.sse,
        }
    }
}

impl DrivenRecord {
    /// What became of driving this capability on this connection.
    #[must_use]
    pub fn of(&self, capability: AgentCapability) -> Driven {
        self.outcomes.get(&capability).cloned().unwrap_or_default()
    }

    /// Whether nothing has been driven on this connection yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }

    /// The agent answered a call that drove this capability.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn answered(&mut self, capability: AgentCapability) -> bool {
        self.record(capability, Driven::Answered)
    }

    /// The call that drove this capability was refused, or nobody was there to
    /// answer it.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn refused(&mut self, capability: AgentCapability, error: CallError) -> bool {
        self.record(capability, Driven::Refused(error))
    }

    /// Holds the latest outcome only, so a later success replaces a refusal.
    fn record(&mut self, capability: AgentCapability, outcome: Driven) -> bool {
        if self.outcomes.get(&capability) == Some(&outcome) {
            return false;
        }
        self.outcomes.insert(capability, outcome);
        true
    }
}

/// A record stated whole, for the window's tests and previews, which have no
/// agent to drive.
impl FromIterator<(AgentCapability, Driven)> for DrivenRecord {
    fn from_iter<T: IntoIterator<Item = (AgentCapability, Driven)>>(outcomes: T) -> Self {
        Self {
            outcomes: outcomes.into_iter().collect(),
        }
    }
}
