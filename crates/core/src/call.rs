//! What a driven method answers with when there is no answer.
//!
//! Its own module because it belongs to neither of the two layers that use it:
//! the envelope (`rpc`) knows how a call fails on the wire, the typed layer
//! (`client`) knows how one fails against ACP, and the screen that reports it
//! has one question — did the thing I asked for happen? One flat enum is what
//! that question deserves; four error types would only make every caller match
//! on all of them.

use agent_client_protocol_schema::v1;

/// Why a driven method has no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallError {
    /// There is no connection, or it ended before the answer arrived.
    Disconnected,
    /// The call needs a session and there is none: nothing has answered
    /// `session/new` yet on this connection.
    NoSession,
    /// Image content was supplied without an Agent image advertisement.
    ImageNotAdvertised,
    /// Audio content was supplied without an Agent audio advertisement.
    AudioNotAdvertised,
    /// Embedded content was supplied without the Agent advertisement.
    EmbeddedContextNotAdvertised,
    /// The complete outgoing prompt Frame exceeds the host's 10 MiB budget.
    PromptTooLarge,
    /// This prompt entry point handles text, images, audio and embedded text only.
    UnsupportedPromptContent,
    /// The bounded outgoing queue cannot accept a prompt immediately.
    OutgoingBusy,
    /// The session's working directory could not be named. ACP wants an
    /// absolute path (§7.1) and the form is a place where people type `.` or
    /// nothing at all, so the answer has to be made out of where the inspector
    /// itself is — and a process whose own working directory has been deleted
    /// underneath it cannot say. Naming a plausible directory instead is the
    /// one thing this tool may not do.
    NoWorkingDirectory,
    /// The blocking request was not waiting for this answer, so it was not
    /// sent: it has one already, or the turn it blocked ended without it. The
    /// agent asked once and is told once — a second response to an id already
    /// answered is a frame JSON-RPC has no shape for — and an answer chosen as
    /// the turn was cancelled out from under it is the usual way to arrive
    /// here.
    NotWaiting,
    /// The agent answered, and the answer was an error. Carried whole — the
    /// `-32000 auth_required` state the spec asks the UI to surface (§7.1) is
    /// this arm with a code in it, not a special case here.
    Rejected(v1::Error),
    /// The agent answered with a result v1 could not read. The frame is in the
    /// trace, verbatim, like every other (§8); this is what the decoder said
    /// about it.
    Undecodable(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => f.write_str("the connection to the agent is gone"),
            Self::NoSession => f.write_str("there is no session yet"),
            Self::OutgoingBusy => {
                f.write_str("the outgoing queue is full; the draft can be retried")
            }
            Self::ImageNotAdvertised => f.write_str("the agent did not advertise image prompts"),
            Self::AudioNotAdvertised => f.write_str("the agent did not advertise audio prompts"),
            Self::EmbeddedContextNotAdvertised => {
                f.write_str("the agent did not advertise embedded context prompts")
            }
            Self::PromptTooLarge => f.write_str("the complete prompt Frame exceeds 10 MiB"),
            Self::UnsupportedPromptContent => f.write_str(
                "this composer sends only text, images, audio and embedded text resources",
            ),
            Self::NoWorkingDirectory => {
                f.write_str("the session's working directory could not be named")
            }
            Self::NotWaiting => {
                f.write_str("the agent is no longer waiting for an answer to that request")
            }
            Self::Rejected(error) => write!(
                f,
                "the agent refused: {} ({})",
                error.message,
                i32::from(error.code)
            ),
            Self::Undecodable(problem) => {
                write!(f, "the agent's answer could not be read as v1: {problem}")
            }
        }
    }
}

impl std::error::Error for CallError {}
