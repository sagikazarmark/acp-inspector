//! Where authentication stands (`docs/architecture.md` §7.1, §1.1).
//!
//! **Display only, on purpose.** The inspector shows what the agent advertised,
//! sends `authenticate` when the user picks one of the advertised methods, and
//! surfaces `-32000 auth_required` as a state a screen can render. What it does
//! not do is log anyone in: every practical login is an OAuth flow or a device
//! code the agent runs itself, and the client's part in it is a terminal the
//! inspector deliberately does not embed. So the honest end of this state is a
//! sentence telling the user to run the agent's own login where they run
//! everything else.
//!
//! **`logout` is the one call here that moves this state on its own** (§7.7).
//! It is the other half of the same display: the agent is asked to give back
//! what it accepted, and a success is read as exactly that and no further — back
//! to [`Unasked`](AuthState::Unasked), with no login screen raised on the
//! strength of what the schema predicts the *next* call will be told.
//!
//! **And it is the capability panel's record for `authenticate`** (§7.7), which
//! is why the fourth ring's [`DrivenRecord`](crate::DrivenRecord) has no entry
//! for that call: this has held not-driven, accepted-with-a-method and
//! refused-with-the-agent's-own-error per connection since the MVP, and two
//! stores holding the same three things would be two things to disagree about
//! one login. [`driven`](AuthState::driven) is the one mapping between them.
//!
//! A field rather than a log, and for the same reason the turn is
//! ([`TurnState`](crate::TurnState)): the question the screen asks is where
//! authentication stands *now*, and the record of how it got there is the trace.

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
    /// **Also where a successful `logout` puts it back** (§7.7): the agent has
    /// retracted what it accepted, and going on naming the method it accepted
    /// would be a screen stating something the agent has taken back. What it is
    /// not is a login being asked for — the schema predicts that new sessions
    /// will require authentication, and a prediction is not the agent asking, so
    /// [`needs_login`](Self::needs_login) stays false and the next refusal
    /// settles it honestly.
    #[default]
    Unasked,
    /// The agent answered something with `-32000 auth_required`.
    ///
    /// Carries the agent's own error, message and all: the screen names the
    /// advertised methods out of
    /// [`Inspector::agent`](crate::Inspector::agent) and says the rest in the
    /// agent's words, because a client paraphrasing a refusal is a client
    /// standing between the reader and the wire.
    ///
    /// Which call was refused is not part of it. `auth_required` is a fact
    /// about the connection — the agent wants a login before it will do
    /// anything — and an agent that answers it to `session/new` and then to
    /// `session/prompt` has said one thing twice.
    Required(v1::Error),
    /// `authenticate` was sent with this method and the agent accepted it.
    ///
    /// Which is all it means: the agent said yes to a method id. Whether the
    /// user is now able to open a session is the next call's answer, not this
    /// one's.
    Authenticated(v1::AuthMethodId),
    /// `authenticate` was sent with this method and there was no acceptance:
    /// the agent refused it, or the connection went away holding it.
    Refused {
        method: v1::AuthMethodId,
        error: CallError,
    },
}

impl AuthState {
    /// Whether the agent has said it will not proceed without a login.
    ///
    /// The login-needed screen's condition, and the one question the four
    /// states above answer differently from each other: an `authenticate` that
    /// was refused is a report about an attempt, while this is a report about
    /// the agent.
    pub fn needs_login(&self) -> bool {
        match self {
            Self::Required(_) => true,
            // An `authenticate` the agent refused *with* `auth_required` is
            // still the agent asking for a login; it has only said so about the
            // login itself.
            Self::Refused {
                error: CallError::Rejected(refusal),
                ..
            } => refusal.code == v1::ErrorCode::AuthRequired,
            _ => false,
        }
    }

    /// What became of driving `authenticate` on this connection (§7.7).
    ///
    /// **The capability row's fourth fact, out of the store that has always
    /// held it** — the one-store argument is above, on the module. What this
    /// adds is the mapping: the row asks this and the auth block beside it reads
    /// the same value, so the two are said twice on screen and decided once.
    ///
    /// **A connection that died under an `authenticate` in flight is a
    /// refusal**, because that is where this state already put it, which is the
    /// whole reason [`Driven`] folds a dead connection the same way.
    ///
    /// **[`Required`](Self::Required) is *not driven*, and that is not a
    /// rounding.** An agent that answered `-32000 auth_required` to something
    /// else has asked for a login; nobody has sent `authenticate` yet, so
    /// nothing has driven the advertisement. What the agent said is on screen
    /// directly above the row, in the agent's own words.
    #[must_use]
    pub fn driven(&self) -> Driven {
        // Exhaustive on purpose, `#[non_exhaustive]` notwithstanding: the
        // attribute binds other crates and not this one, so a state added here
        // is a state this has to be told what to say about, rather than one
        // quietly reported as *not driven*.
        match self {
            Self::Unasked | Self::Required(_) => Driven::Unasked,
            Self::Authenticated(_) => Driven::Answered,
            Self::Refused { error, .. } => Driven::Refused(error.clone()),
        }
    }
}
