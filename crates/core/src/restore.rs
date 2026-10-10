//! Opening a session that already exists (`docs/architecture.md` §7.5):
//! `session/load` and `session/resume`, modelled as one operation with a
//! property.
//!
//! The requests carry the same inputs (resume omits an empty MCP list). The one
//! observable difference: load MUST replay the conversation as `session/update`
//! notifications before answering, and resume MUST NOT replay it
//! ([`replays`](Restore::replays)). Where both are advertised, load is
//! preferred ([`Restore::preferred`]).

use agent_client_protocol_schema::v1;

/// How an existing session is opened on this agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Restore {
    /// `session/load`: the conversation replays into the timeline as
    /// `session/update` notifications, and the call does not answer until it
    /// has.
    Load,
    /// `session/resume`: the session is restored and nothing is replayed, so an
    /// empty timeline is correct.
    Resume,
}

impl Restore {
    /// How this agent would open an existing session: load where advertised,
    /// else resume; `None` where neither is advertised.
    ///
    /// Gated on the advertisement only (§7.5). Load is the boolean
    /// `agentCapabilities.loadSession`; resume is the presence of
    /// `sessionCapabilities.resume`.
    pub fn preferred(capabilities: &v1::AgentCapabilities) -> Option<Self> {
        if capabilities.load_session {
            Some(Self::Load)
        } else if capabilities.session_capabilities.resume.is_some() {
            Some(Self::Resume)
        } else {
            None
        }
    }

    /// Whether opening the session replays the conversation into the timeline
    /// before the call answers (the subject of a conformance rule, §15 q7).
    pub fn replays(self) -> bool {
        match self {
            Self::Load => true,
            Self::Resume => false,
        }
    }

    /// The method this sends, by its name on the wire.
    pub fn method(self) -> &'static str {
        match self {
            Self::Load => v1::AGENT_METHOD_NAMES.session_load,
            Self::Resume => v1::AGENT_METHOD_NAMES.session_resume,
        }
    }
}
