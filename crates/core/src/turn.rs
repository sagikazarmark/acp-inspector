//! Where the turn stands (`CONTEXT.md`, *Turn*; `docs/architecture.md` §11
//! seam 2).
//!
//! A field written by the task reading the stream, not a bracket around the
//! prompt call: v2 can end a turn with a notification, and agents can
//! interleave updates and responses in orders a bracket cannot describe.

use crate::call::CallError;

use agent_client_protocol_schema::v1;

/// The state of the turn on the active session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnState {
    /// Nothing has been prompted in the live session: it was just opened, or
    /// the previous session was switched away from (§7.5).
    #[default]
    Idle,
    /// A prompt is on the wire and the turn has not resolved.
    InFlight,
    /// A `session/cancel` has been sent and the turn has still not resolved.
    ///
    /// The spec still requires the prompt to resolve with
    /// [`StopReason::Cancelled`](v1::StopReason::Cancelled) (§7.1); an agent that
    /// never does stays here.
    Cancelling,
    /// The turn ended and the agent said why — including `cancelled`, which is
    /// what a cancel is supposed to produce.
    Ended(v1::StopReason),
    /// The turn ended without a stop reason: the agent answered the prompt with
    /// an error, or the connection went away while the turn was open. The
    /// inspector never invents a stop reason.
    Failed(CallError),
}

impl TurnState {
    /// Whether the turn is still open — prompted and not resolved, whether or
    /// not a cancel has been asked for.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::InFlight | Self::Cancelling)
    }
}

/// What a finished turn came to, kept for every turn of the session.
///
/// [`TurnState`] is the live turn's position and is replaced by the next
/// prompt; this is the record of a turn that is over.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnOutcome {
    /// The agent ended the turn and said why.
    Ended(v1::StopReason),
    /// The turn ended without a stop reason: the agent answered the prompt with
    /// an error, or the connection went away while it was open.
    Failed(CallError),
}

impl TurnOutcome {
    /// The outcome a settled state carries, or `None` for a state that is not
    /// an ending. Derived from the state so the two cannot disagree.
    pub(crate) fn of(state: &TurnState) -> Option<Self> {
        match state {
            TurnState::Ended(reason) => Some(Self::Ended(*reason)),
            TurnState::Failed(error) => Some(Self::Failed(error.clone())),
            _ => None,
        }
    }
}
