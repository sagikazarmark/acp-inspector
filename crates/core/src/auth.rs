//! Where authentication stands (`docs/architecture.md` §7.1, §1.1).
//!
//! Display only: the inspector shows the advertised methods, sends
//! `authenticate` when the user picks one, and surfaces `-32000 auth_required`
//! as a state. It never runs a login itself; agents run their own.
//!
//! This is also the driven record for `authenticate` (§7.7), so
//! [`DrivenRecord`](crate::DrivenRecord) has no entry for it;
//! [`driven`](AuthState::driven) maps between the two.

use agent_client_protocol_schema::v1;

use crate::call::CallError;
use crate::driven::Driven;

/// What the connection has been told about logging in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthState {
    /// Nothing on this connection has asked for a login, and nobody has offered
    /// one.
    ///
    /// A successful `logout` also returns here; it does not raise the login
    /// screen, since the agent has not yet asked for one.
    #[default]
    Unasked,
    /// The agent answered something with `-32000 auth_required`, carrying the
    /// agent's own error. Which call was refused is not recorded: it is a fact
    /// about the connection.
    Required(v1::Error),
    /// `authenticate` was sent with this method and the agent accepted it.
    Authenticated(v1::AuthMethodId),
    /// `authenticate` was sent with this method and there was no acceptance:
    /// the agent refused it, or the connection went away holding it.
    Refused {
        method: v1::AuthMethodId,
        error: CallError,
    },
}

impl AuthState {
    /// Whether the agent has said it will not proceed without a login — the
    /// login-needed screen's condition.
    pub fn needs_login(&self) -> bool {
        match self {
            Self::Required(_) => true,
            // An `authenticate` refused with `auth_required` is still the agent
            // asking for a login.
            Self::Refused {
                error: CallError::Rejected(refusal),
                ..
            } => refusal.code == v1::ErrorCode::AuthRequired,
            _ => false,
        }
    }

    /// What became of driving `authenticate` on this connection (§7.7).
    ///
    /// A connection lost mid-call counts as refused. [`Required`](Self::Required)
    /// is not driven: nobody has sent `authenticate` yet.
    #[must_use]
    pub fn driven(&self) -> Driven {
        // Exhaustive on purpose: `#[non_exhaustive]` does not bind this crate, so
        // a new state must be mapped here explicitly.
        match self {
            Self::Unasked | Self::Required(_) => Driven::Unasked,
            Self::Authenticated(_) => Driven::Answered,
            Self::Refused { error, .. } => Driven::Refused(error.clone()),
        }
    }
}
