//! Where the turn stands (`CONTEXT.md`, *Turn*; `docs/architecture.md` §11
//! seam 2).
//!
//! **A field, not a bracket.** The obvious way to write this is "the prompt
//! call is in progress, therefore the turn is running" — and it is the wrong
//! spine. v1 happens to end a turn with the prompt's response; v2 adds a
//! `state_update` notification that says so directly, and an agent can already
//! interleave updates and responses in orders a bracket cannot describe. So the
//! state is a value that several things write, the response being one of them,
//! and the layer that writes it is the one *reading the stream* — not the one
//! awaiting the call.
//!
//! What the caller of `prompt` gets back is a convenience. What the screen
//! renders is this.

use crate::call::CallError;

use agent_client_protocol_schema::v1;

/// The state of the turn on the active session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnState {
    /// Nothing has been prompted in the live session: it has just been opened,
    /// or the session that was being prompted has been switched away from
    /// (§7.5) and its turn is no longer this state's to describe.
    #[default]
    Idle,
    /// A prompt is on the wire and the turn has not resolved.
    InFlight,
    /// A `session/cancel` has been sent and the turn has still not resolved.
    ///
    /// Its own state rather than a flag on [`InFlight`](Self::InFlight),
    /// because this is the window in which a conformance violation is visible:
    /// the spec requires the prompt to resolve with
    /// [`StopReason::Cancelled`](v1::StopReason::Cancelled) even so (§7.1), and
    /// an agent that never resolves it leaves the turn sitting here saying
    /// exactly what it is doing.
    Cancelling,
    /// The turn ended and the agent said why — including `cancelled`, which is
    /// what a cancel is supposed to produce.
    Ended(v1::StopReason),
    /// The turn ended without a stop reason: the agent answered the prompt with
    /// an error, or the connection went away while the turn was open. Not a
    /// stop reason of our own invention, because the inspector reporting a
    /// reason the agent never gave is the one thing it may not do.
    Failed(CallError),
}

impl TurnState {
    /// Whether the turn is still open — prompted and not resolved, whether or
    /// not a cancel has been asked for.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::InFlight | Self::Cancelling)
    }
}

/// What a turn came to, kept beside the turn it was.
///
/// **This is a record and [`TurnState`] is a position.** The state says where
/// the *live* turn stands and is replaced the moment the next prompt goes out;
/// this is what a turn that is over came to, and it is kept for every turn the
/// session had. The two never say the same thing about the same turn: a turn
/// with an outcome is not the live one any more, and the live one has no
/// outcome yet — which is why this is a second type rather than a second copy
/// of the first. `Idle` and `InFlight` are not outcomes and have no spelling
/// here.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnOutcome {
    /// The agent ended the turn and said why.
    Ended(v1::StopReason),
    /// The turn ended without a stop reason: the agent answered the prompt with
    /// an error, or the connection went away while it was open. Never a reason
    /// of the inspector's own invention.
    Failed(CallError),
}

impl TurnOutcome {
    /// The outcome a settled state carries, or `None` for a state that is not
    /// an ending at all.
    ///
    /// Read from the state rather than assembled beside it, so the record and
    /// the position cannot disagree about how a turn finished.
    pub(crate) fn of(state: &TurnState) -> Option<Self> {
        match state {
            TurnState::Ended(reason) => Some(Self::Ended(*reason)),
            TurnState::Failed(error) => Some(Self::Failed(error.clone())),
            _ => None,
        }
    }
}
