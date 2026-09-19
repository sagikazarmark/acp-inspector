//! What was driven on this connection, and what came back
//! (`docs/architecture.md` §7.7).
//!
//! **The capability display could say what an agent claimed and could not say
//! what happened when anybody took a claim up.** An agent that advertises a
//! method and refuses every call to it reads, on the screen built to audit its
//! claims, exactly like an agent nobody ever pressed the button on. This is the
//! record that tells the two apart: one outcome per Agent Capability, for the
//! connection that is live.
//!
//! **It is passive.** Nothing here sends anything; it records what the
//! affordances already on screen caused, so every frame in the trace is still
//! one the user asked for. The listing the tool refreshes on its own initiative
//! after a close or a delete is the case that proves the rule and writes
//! nothing.
//!
//! **Three outcomes and nothing else**: not driven, answered, refused — the
//! last carrying the agent's own error, because a client paraphrasing a refusal
//! is a client standing between the reader and the wire. A connection that died
//! under a call in flight lands in **refused**, which is where [`AuthState`] and
//! the two Session Settings setters already put it; the split
//! [`Ending`](crate::Ending) makes exists because a MUST forced it, and nothing
//! here is forced.
//!
//! **Latest wins, and a refusal is not sticky.** The next drive of the same
//! Advertisement replaces the last outcome, because a later drive is the next
//! ask — the rule a `session/set_config_option` answer already follows.
//!
//! **A field rather than a log**, for the reason the turn and the auth state
//! are: the question a row asks is what became of this capability *now*, and the
//! record of how it got there is the trace. And never derived from the trace,
//! which is a bounded ring that drops its oldest frames — a record scanned out
//! of it would report *not driven* about an agent that was driven, on exactly
//! the long connection where somebody wants to read it.
//!
//! [`AuthState`]: crate::AuthState

use std::collections::HashMap;

use agent_client_protocol_schema::v1;

use crate::call::CallError;

/// One thing the agent claimed about itself in its `initialize` result
/// (`CONTEXT.md`, *Agent Capability*), named so a record can be keyed on it.
///
/// **This is core's copy of the capability set**, and the capability display
/// keeps its own (`crates/app/src/agent.rs`'s `advertised`). The two are separate
/// on purpose so that each fails on its own, with the schema asked directly
/// whether either has gone stale (`crates/core/tests/capability.rs`). What this must
/// not become is a *third* list, which is why a row asks the record by naming a
/// variant rather than by matching a string: a key matched loosely would render
/// a capability upstream adds as *unknown* instead of breaking a build, which is
/// the exact failure that tripwire exists to prevent.
///
/// The six the session lifecycle is made of (§7.5), logout, and image/audio prompts.
/// Embedded text context is driven; the two MCP transports remain deferred (§1.1), and
/// `authenticate` is recorded in [`AuthState`](crate::AuthState), which has held
/// exactly these three outcomes per connection since the MVP.
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
    ///
    /// **The one key here that is not about a session at all.** The request
    /// carries no session id and the method name is the bare `logout`, so where
    /// the six above are the connection's *claims* about its sessions, this is
    /// the connection's own — which is what makes it the auth block's claim and
    /// not `sessionCapabilities`' (§7.7).
    Logout,
    /// Image content in `session/prompt`, claimed by promptCapabilities.image.
    Image,
    /// Audio content in `session/prompt`, claimed by promptCapabilities.audio.
    Audio,
    /// Embedded content in `session/prompt`, claimed by promptCapabilities.embeddedContext.
    EmbeddedContext,
}

/// What became of driving an Agent Capability on this connection (§7.7).
///
/// Three outcomes, because a call nobody answered is one of the three rather
/// than a fourth: a connection that went away holding a call refused it, the
/// way it refuses a `session/set_mode` it will never answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Driven {
    /// Nothing has driven this Advertisement on this connection.
    ///
    /// Named the way [`AuthState::Unasked`](crate::AuthState::Unasked) is,
    /// because it is the same state about a different claim: nobody has put the
    /// question to this agent yet.
    #[default]
    Unasked,
    /// It was driven, and the agent answered.
    ///
    /// Read off the answer's *shape* and not off a decoding of it: an answer
    /// this layer could not read as v1 is still the agent answering, and the
    /// frame is in the trace, complete (§8).
    Answered,
    /// It was driven and there was no answer: the agent refused, or the
    /// connection went away holding the call.
    ///
    /// Carries the agent's own error where the agent gave one, whole.
    Refused(CallError),
}

/// What has been driven on this connection, and what came back (§7.7).
///
/// A value, not a handle: the store is a field the typed layer writes
/// ([`Inspector::driven`](crate::Inspector::driven)), and this is what one read
/// of it says.
///
/// **Per connection.** It is cleared when another agent is connected to —
/// nothing the last agent did is this one's answer to anything — and kept when
/// the agent dies, because the panel outlives the agent it describes and
/// blanking the record at the moment the run ended would erase the evidence just
/// as somebody turned to read it. A session switch does not touch it: what it is
/// about is the connection's claims.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DrivenRecord {
    /// Only the capabilities something has driven. An absent key is
    /// [`Driven::Unasked`], which is why nothing ever writes that variant: *not
    /// driven* is the absence of a record rather than a record of an absence.
    outcomes: HashMap<AgentCapability, Driven>,
}

impl AgentCapability {
    /// Every capability this record can be keyed on, in the order the display
    /// draws them.
    pub const ALL: [Self; 10] = [
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
    ];

    /// What the protocol calls it — the method it gates where it gates one, the
    /// field name where it gates a field.
    ///
    /// Named rather than paraphrased, like every other protocol word this crate
    /// hands out: the reader of an inspector is reading the protocol.
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
        }
    }

    /// Whether this agent claimed it.
    ///
    /// **The two shapes v1 states a claim in, kept apart** (§7.5): a boolean on
    /// `agentCapabilities` for load, the presence of an object under
    /// `sessionCapabilities` for the five session claims and under `auth` for
    /// `logout`. Which is which is read here rather than guessed anywhere else.
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
        }
    }
}

impl DrivenRecord {
    /// What became of driving this capability on this connection.
    #[must_use]
    pub fn of(&self, capability: AgentCapability) -> Driven {
        self.outcomes.get(&capability).cloned().unwrap_or_default()
    }

    /// Whether anything has been driven on this connection at all.
    ///
    /// Which is what a fresh connection says, and it is an answer rather than a
    /// silence: this agent has been asked nothing yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }

    /// The agent answered a call that drove this capability (§7.7).
    ///
    /// Answers with whether anything changed.
    pub(crate) fn answered(&mut self, capability: AgentCapability) -> bool {
        self.record(capability, Driven::Answered)
    }

    /// The call that drove this capability got no answer, and this is what was
    /// said about it (§7.7).
    ///
    /// The agent's own refusal, or the news that there was nobody to ask.
    ///
    /// Answers with whether anything changed.
    pub(crate) fn refused(&mut self, capability: AgentCapability, error: CallError) -> bool {
        self.record(capability, Driven::Refused(error))
    }

    /// Holds the latest outcome, and only the latest.
    ///
    /// **The next drive replaces the last one, refusal or not.** A refusal here
    /// is what became of an ask, and a later drive is the next ask — so a
    /// capability that has since been driven successfully stops reporting a
    /// refusal the user has already fixed.
    fn record(&mut self, capability: AgentCapability, outcome: Driven) -> bool {
        if self.outcomes.get(&capability) == Some(&outcome) {
            return false;
        }
        self.outcomes.insert(capability, outcome);
        true
    }
}

/// A record stated whole, rather than driven into being.
///
/// Constructible for [`ConfigRefusal`](crate::ConfigRefusal)'s reason: this is
/// what a record *is* rather than a handle on something core owns, so the window
/// that renders one and the tests that state one expected read it the same way.
/// The desktop crate has no runtime to drive an agent with (§5, §12), and what
/// its rows draw is asserted against a record it can hold in its hand.
impl FromIterator<(AgentCapability, Driven)> for DrivenRecord {
    fn from_iter<T: IntoIterator<Item = (AgentCapability, Driven)>>(outcomes: T) -> Self {
        Self {
            outcomes: outcomes.into_iter().collect(),
        }
    }
}
