//! Opening a session that already exists (`docs/architecture.md` §7.5):
//! `session/load` and `session/resume`, which are **one operation with a
//! property** rather than two.
//!
//! Their requests and their responses are field-for-field identical, and the
//! sole observable difference between them is an ordering the specification
//! states twice: load MUST replay the entire conversation as `session/update`
//! notifications and MUST NOT answer until every entry has been streamed, while
//! resume MUST NOT replay it at all. That difference is the whole of what a
//! client gets to choose between, because it is what decides whether a client
//! can rebuild the session it is opening — so it is modelled as a property of
//! the call ([`replays`](Restore::replays)) and never as two unrelated
//! affordances.
//!
//! **Where both are advertised, load is preferred** ([`Restore::preferred`]).
//! That is the policy a shipped ACP client already uses, and it anticipates the
//! protocol's own direction, in which load is removed and resume grows a replay
//! cursor. Where only resume is advertised the answer is still an answer — this
//! agent can reopen the session and cannot show what was said in it, which is a
//! fact a screen has to state rather than leave a reader to conclude from an
//! empty timeline.

use agent_client_protocol_schema::v1;

/// How an existing session is opened on this agent.
///
/// The two calls ACP v1 gives for it, told apart by the one thing that is
/// observably different about them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Restore {
    /// `session/load`: the conversation replays into the timeline as
    /// `session/update` notifications, and the call does not answer until it
    /// has.
    Load,
    /// `session/resume`: the session is restored and nothing is replayed, so a
    /// resumed session's empty timeline is *correct* rather than a failure.
    Resume,
}

impl Restore {
    /// How this agent would open an existing session, or `None` where it
    /// advertised no way to.
    ///
    /// **Load where it is offered**, resume where it is the only offer. Gated
    /// on the advertisement and on nothing else (§7.5): an agent that claims a
    /// method it has not implemented is offered the affordance anyway, because
    /// what it does when it is driven is the thing a user came here to find
    /// out.
    ///
    /// The two claims are read from the two different shapes v1 states them in
    /// — a boolean on `agentCapabilities` for load, the presence of an object
    /// under `sessionCapabilities` for resume — which the capability display
    /// keeps apart for the same reason (§9).
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
    /// before the call answers.
    ///
    /// The property the screen is built around, and the thing the second
    /// conformance rule is about (§15 q7).
    pub fn replays(self) -> bool {
        match self {
            Self::Load => true,
            Self::Resume => false,
        }
    }

    /// The method this sends, by its name on the wire.
    ///
    /// Named rather than paraphrased, like every other affordance here: the
    /// reader of an inspector is reading the protocol, and a button that said
    /// "reopen" would be a translation they did not ask for.
    pub fn method(self) -> &'static str {
        match self {
            Self::Load => v1::AGENT_METHOD_NAMES.session_load,
            Self::Resume => v1::AGENT_METHOD_NAMES.session_resume,
        }
    }
}
