//! What a driven method answers with when there is no answer: one flat enum
//! shared by the envelope (`rpc`) and the typed layer (`client`).

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
    /// This prompt entry point handles text, images, audio, embedded text and PDFs only.
    UnsupportedPromptContent,
    /// The bounded outgoing queue cannot accept a prompt immediately.
    OutgoingBusy,
    /// The session's absolute working directory could not be determined: the
    /// form's path is relative and the inspector's own working directory is
    /// unknown (e.g. deleted).
    NoWorkingDirectory,
    /// Inspector-supplied MCP input is incomplete; no Session call was made.
    InvalidMcp(String),
    /// A path the request would carry (the working directory or a root) is
    /// not valid UTF-8. ACP carries paths as JSON strings, so nothing was
    /// sent rather than a request with its parameters missing.
    PathNotUtf8,
    /// The blocking request was not waiting for this answer, so nothing was
    /// sent: it was already answered, or the turn it blocked ended. Each id is
    /// answered at most once.
    NotWaiting,
    /// The agent answered with an error, carried whole (including
    /// `-32000 auth_required`).
    Rejected(v1::Error),
    /// The agent answered with a result v1 could not read; this is the
    /// decoder's message. The frame is in the trace verbatim.
    Undecodable(String),
    /// The agent's answer was longer than the transport carries, so it was
    /// dropped; `length` is its size in bytes. A placeholder is in the trace.
    AnswerDropped { length: usize },
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
                "this composer sends only text, images, audio, embedded text and PDF resources",
            ),
            Self::NoWorkingDirectory => {
                f.write_str("the session's working directory could not be named")
            }
            Self::InvalidMcp(problem) => f.write_str(problem),
            Self::PathNotUtf8 => f.write_str(
                "a directory the session names is not valid UTF-8, and ACP carries paths as text",
            ),
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
            Self::AnswerDropped { length } => write!(
                f,
                "the agent's answer was {length} bytes, too long to carry, and was dropped"
            ),
        }
    }
}

impl std::error::Error for CallError {}
