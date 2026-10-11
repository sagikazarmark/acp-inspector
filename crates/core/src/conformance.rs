//! Conformance annotations: where the ACP specification says **MUST** and the
//! captured frames can decide whether it was met. See `docs/architecture.md`
//! §15 q7.
//!
//! An annotation is admitted only where a MUST meets a violation decidable from
//! frames alone, so there is no rule engine, severity, score or configuration:
//! each rule is a named variant. Behaviour the specification leaves undefined
//! draws nothing, so the absence of an annotation is not approval.
//!
//! Annotations land on the timeline ([`EntryKind::Annotation`]) beside the
//! frames they are about.
//!
//! [`EntryKind::Annotation`]: crate::EntryKind::Annotation

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::turn::TurnState;

/// One place the specification states a MUST and the frames say it was not met.
///
/// A surface that renders [`requires`](Self::requires) and
/// [`observed`](Self::observed) needs no change when a variant is added.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Annotation {
    /// A turn that was cancelled did not resolve with
    /// [`Cancelled`](v1::StopReason::Cancelled).
    ///
    /// The schema requires it once the client sends `session/cancel`, "even if
    /// the cancellation caused exceptions in underlying operations". Decided
    /// from the cancel this client sent for an open turn and whatever answered
    /// the prompt.
    Cancellation(Ending),
    /// A `session/load` answered before the conversation had been replayed.
    ///
    /// The agent "MUST replay the entire conversation" as `session/update`
    /// notifications and only then "MUST respond to the original
    /// `session/load` request" (§7.5). Only the ordering boundary is decidable;
    /// [`Replay`] says which was crossed.
    Replay(Replay),
    /// A `session/set_config_option` that succeeded answered with something that
    /// was not the complete set of configuration options.
    ///
    /// Decided narrowly: the answer to a successful set for an option id lacks
    /// that id, so it cannot be the full set. A response omitting some other
    /// option draws nothing, because a shrinking set is indistinguishable from a
    /// delta in the trace. [`OptionSet`] says which shortfall was found.
    OptionSet(OptionSet),
    /// Two URL elicitations outstanding at once under one `elicitationId`
    /// (§7.8).
    ///
    /// An agent "MUST keep each `elicitationId` unique among outstanding URL
    /// elicitations". Decided from the create that minted the id, still
    /// outstanding, and the one that reused it. A completed or abandoned id is
    /// free again. [`Reuse`] says how it was reused.
    ElicitationId(Reuse),
}

/// How an `elicitationId` was reused.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reuse {
    /// A URL elicitation arrived carrying an id another one is still
    /// outstanding under.
    Outstanding(v1::ElicitationId),
}

/// How a cancelled turn ended, when it did not end the one way it had to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ending {
    /// The agent resolved the prompt, with some other stop reason.
    Stopped(v1::StopReason),
    /// The agent answered the prompt with an error or a result v1 could not
    /// read, rather than a stop reason.
    Failed(CallError),
    /// The connection ended with the prompt unanswered.
    ///
    /// Decidable only once the connection ends; until then the turn stays
    /// [`Cancelling`](crate::TurnState::Cancelling).
    Unresolved,
}

/// What a `session/load` did with the conversation, when what it did was not
/// what the ordering requires.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Replay {
    /// No `session/update` for the loaded session crossed between the request
    /// and its answer, while this connection had already heard the agent talk
    /// in that session.
    ///
    /// A session never heard from draws nothing: replaying nothing into an
    /// empty session is a complete replay.
    Nothing,
}

/// What a `session/set_config_option` answered with, when the answer cannot
/// have been the complete option set.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionSet {
    /// The option that was just set is not in the set the answer carried.
    Missing(v1::SessionConfigId),
}

impl Ending {
    /// How a cancelled turn ended, or `None` where it resolved `cancelled` or
    /// has not ended.
    pub(crate) fn of(resolved: &TurnState) -> Option<Self> {
        match resolved {
            TurnState::Ended(v1::StopReason::Cancelled) => None,
            TurnState::Ended(reason) => Some(Self::Stopped(*reason)),
            // The one way "it never resolved" becomes decidable from frames.
            TurnState::Failed(CallError::Disconnected) => Some(Self::Unresolved),
            TurnState::Failed(error) => Some(Self::Failed(error.clone())),
            // Not a resolution yet; `TurnState` is non-exhaustive.
            _ => None,
        }
    }
}

impl OptionSet {
    /// What the answer to a set left out, or `None` where nothing is missing or
    /// the answer carried no `configOptions` at all.
    ///
    /// Read off the raw answer rather than the decoded one: the typed layer
    /// skips option entries it cannot read (§7.6), and an option present on the
    /// wire must not be reported missing. A `configOptions` that is not a list
    /// counts as one without the id.
    pub(crate) fn of(answered: &Value, option: &v1::SessionConfigId) -> Option<Self> {
        let listed = answered.get("configOptions")?;
        let asked = option.to_string();
        let carried = listed.as_array().is_some_and(|options| {
            options
                .iter()
                .any(|entry| entry.get("id").and_then(Value::as_str) == Some(asked.as_str()))
        });

        (!carried).then(|| Self::Missing(option.clone()))
    }
}

impl Annotation {
    /// What the specification requires: one sentence per rule.
    pub fn requires(&self) -> &'static str {
        match self {
            Self::Cancellation(_) => concat!(
                "A `session/prompt` the client cancelled must still resolve, ",
                "and must resolve with `stopReason: \"cancelled\"` — ",
                "even if the cancellation caused exceptions in whatever the agent was doing.",
            ),
            Self::Replay(_) => concat!(
                "A `session/load` must replay the entire conversation to the client as ",
                "`session/update` notifications, and must not answer until every entry ",
                "has been streamed — replaying before answering is the whole of what ",
                "makes load different from `session/resume`.",
            ),
            Self::OptionSet(_) => concat!(
                "A `session/set_config_option` must answer with the complete set of ",
                "configuration options — and a set that succeeded for an option is an ",
                "agent saying that option exists, so a list without it cannot be that set.",
            ),
            Self::ElicitationId(_) => concat!(
                "An agent must keep each `elicitationId` unique among outstanding URL ",
                "elicitations — the client is required to treat the id as opaque, so it is ",
                "the only thing an `elicitation/complete` names the interaction by.",
            ),
        }
    }

    /// What this agent did instead, quoting the agent's own values (such as the
    /// stop reason as it was spelled on the wire).
    pub fn observed(&self) -> String {
        match self {
            Self::Cancellation(Ending::Stopped(reason)) => {
                format!(
                    "The agent resolved it with `stopReason: \"{}\"`.",
                    spelt(reason)
                )
            }
            Self::Cancellation(Ending::Failed(error)) => {
                format!("The agent gave no stop reason at all: {error}.")
            }
            Self::Cancellation(Ending::Unresolved) => {
                "It never resolved: the connection ended with the prompt unanswered.".to_owned()
            }
            Self::Replay(Replay::Nothing) => concat!(
                "The agent answered it having replayed nothing: no `session/update` for ",
                "that session crossed between the request and the answer, and this ",
                "connection has already heard the agent talk in it.",
            )
            .to_owned(),
            Self::OptionSet(OptionSet::Missing(option)) => {
                format!(
                    "The agent accepted the set for `{option}` and answered with an option \
                     set that does not contain it."
                )
            }
            Self::ElicitationId(Reuse::Outstanding(id)) => {
                format!(
                    "The agent opened a second URL elicitation as `{id}` while the first one \
                     under that id was still outstanding."
                )
            }
        }
    }
}

/// A stop reason as the agent spelled it on the wire.
///
/// Read back from the serialization because [`v1::StopReason`] is
/// non-exhaustive: a reason added later still prints as itself.
fn spelt(reason: &v1::StopReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{reason:?}"))
}
