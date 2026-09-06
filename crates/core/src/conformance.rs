//! Conformance annotations (`docs/architecture.md` §15 q7): the places the ACP
//! specification says **MUST** and the frames the trace already holds can say
//! whether it was met.
//!
//! **The admission rule is the whole design, and it is a boundary rather than a
//! feature.** An annotation is produced only where the specification states a
//! MUST *and* the violation is decidable from captured frames. Nothing that
//! would need a model of what the agent is doing qualifies. That is what keeps
//! this an inspector rather than a grader, and it is why there is no rule
//! engine here, no severity, no aggregate verdict, no score and no
//! configuration — the rules are *named*, one variant each, and the list grows
//! only when the specification supplies another qualifying MUST.
//!
//! **Silence is not approval.** Behaviour the specification leaves undefined
//! draws no annotation at all, which is deliberate and stated so that the
//! absence of a complaint is not read as one — an agent doing something
//! surprising in an undefined corner is what a user came here to find, and the
//! inspector shows it without an opinion.
//!
//! Where they *go* is the timeline ([`EntryKind::Annotation`]), carrying the
//! frames they are about: an annotation is read inline, beside the traffic it
//! describes, and never as a modal, a toast or a lane of its own.
//!
//! [`EntryKind::Annotation`]: crate::EntryKind::Annotation

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::turn::TurnState;

/// One place the specification states a MUST and the frames say it was not met.
///
/// Non-exhaustive because the list grows — and it grows only when the
/// specification supplies another qualifying MUST. A surface that renders
/// [`requires`](Self::requires) and [`observed`](Self::observed) needs no
/// change when it does, which is what the second rule cost to add, and the
/// third after it: a variant here, and nothing anywhere else.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Annotation {
    /// A turn that was cancelled did not resolve with
    /// [`Cancelled`](v1::StopReason::Cancelled).
    ///
    /// The MUST is the schema's own, on `StopReason::Cancelled`: it is required
    /// when the client sends `session/cancel`, *even if the cancellation caused
    /// exceptions in underlying operations*. Everything it takes to decide is
    /// on the wire — a `session/cancel` this client sent for an open turn, and
    /// whatever answered that turn's `session/prompt`.
    Cancellation(Ending),
    /// A `session/load` answered before the conversation had been replayed.
    ///
    /// Two MUSTs, one ordering: the agent *"MUST replay the entire conversation
    /// to the Client in the form of `session/update` notifications"*, and
    /// *"when all the conversation entries have been streamed to the Client,
    /// the Agent MUST respond to the original `session/load` request"*. It is
    /// the difference between load and resume — the two calls are otherwise
    /// field-for-field identical (§7.5) — and it is what decides whether a
    /// client can rebuild a session at all.
    ///
    /// What is decidable from frames is the boundary and not the whole: nobody
    /// here can know what "the entire conversation" was. [`Replay`] says which
    /// boundary was crossed.
    Replay(Replay),
    /// A `session/set_config_option` that succeeded answered with something that
    /// was not the complete set of configuration options.
    ///
    /// The MUST is the specification's on the response: it carries *"the full
    /// set of configuration options"*. What makes a shortfall decidable is
    /// narrow and airtight — a set that succeeded for an option id, answered
    /// with a list that does not contain that id. The option provably exists,
    /// because this client had just changed it and the agent said so, and a
    /// list without it cannot be the complete set. Two frames decide it and no
    /// model of agent state is required.
    ///
    /// **The looser formulation is not this rule** (§15 q7): a response omitting
    /// some *other* option the agent advertised earlier draws nothing, because
    /// an agent whose option set legitimately shrinks is indistinguishable, from
    /// the trace alone, from one sending a delta. [`OptionSet`] says which
    /// shortfall was found.
    OptionSet(OptionSet),
    /// Two URL elicitations outstanding at once under one `elicitationId`
    /// (§7.8).
    ///
    /// The MUST is the specification's on URL mode: an agent *"MUST keep each
    /// `elicitationId` unique among outstanding URL elicitations"*, and a client
    /// *"MUST treat it as opaque"* — so the id is the only thing that says which
    /// elicitation an `elicitation/complete` is about, and two live ones sharing
    /// it make that question unanswerable.
    ///
    /// Decided from two frames and nothing else: the `elicitation/create` that
    /// minted the id, still outstanding, and the one that reused it.
    /// *Outstanding* is the rule's own word and this client keeps it literally —
    /// an id whose elicitation was completed, or abandoned when the connection
    /// went, has left the set and is free again. [`Reuse`] says how it was
    /// reused.
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
    /// The agent answered the prompt with something that was not a stop reason
    /// at all: an error, or a result v1 could not read. Still the same MUST
    /// unmet — the specification asks for one answer and this was not it.
    Failed(CallError),
    /// Nothing ever resolved it: the connection ended with the prompt
    /// unanswered.
    ///
    /// **Decidable only when the connection ends**, and that is the whole of
    /// the patience this rule has. Until then the trace holds no evidence that
    /// the answer is not still coming, and the turn sitting in
    /// [`Cancelling`](crate::TurnState::Cancelling) is what says so on screen.
    /// Once the connection is gone the trace holds every frame it will ever
    /// hold, and the answer is not among them — whoever ended it.
    Unresolved,
}

/// What a `session/load` did with the conversation, when what it did was not
/// what the ordering requires.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Replay {
    /// It replayed nothing at all: not one `session/update` for the session it
    /// was loading crossed between the request and its answer, while the record
    /// already holds a conversation in that session.
    ///
    /// **Both halves are frames** (§15 q7's admission rule): the load and its
    /// answer with nothing between them, and — before either — the agent's own
    /// updates in that session, which is what says there was a conversation to
    /// replay. A session this connection has never heard a word in draws
    /// nothing, because a load that replayed nothing into an empty session
    /// replayed the entire conversation, and annotating there would be
    /// inventing a rule rather than reporting one.
    Nothing,
}

/// What a `session/set_config_option` answered with, when the answer cannot
/// have been the complete option set.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionSet {
    /// The option that was just set is not in the set the answer carried.
    ///
    /// Named rather than counted: the claim is about *that* option, and the id
    /// is the agent's own — the one this client asked it to change and it said
    /// it had.
    Missing(v1::SessionConfigId),
}

impl Ending {
    /// How a turn that was cancelled ended, or `None` where there is nothing to
    /// say — the specification got what it asked for, or the turn has not ended
    /// at all.
    ///
    /// Here rather than at the caller because it is the whole of what this rule
    /// reads a turn *for*: the client holds the cancel and the frames, and what
    /// a resolution amounts to is this file's question.
    pub(crate) fn of(resolved: &TurnState) -> Option<Self> {
        match resolved {
            // What the specification asks for. The rule's silence is the whole
            // of what happens about it.
            TurnState::Ended(v1::StopReason::Cancelled) => None,
            TurnState::Ended(reason) => Some(Self::Stopped(*reason)),
            // The connection ended with the prompt unanswered, which is the one
            // way "it never resolved" becomes decidable from captured frames.
            TurnState::Failed(CallError::Disconnected) => Some(Self::Unresolved),
            TurnState::Failed(error) => Some(Self::Failed(error.clone())),
            // Not a resolution at all: nothing to judge yet, and `TurnState` is
            // non-exhaustive.
            _ => None,
        }
    }
}

impl OptionSet {
    /// What the answer to a set left out, or `None` where there is nothing to
    /// say — which is nearly always.
    ///
    /// Here rather than at the caller for [`Ending::of`]'s reason: what an
    /// answer to a set amounts to is this file's question, and the client's
    /// half is the frames it is read beside.
    ///
    /// **Read off the frame rather than off the decoded answer**, the way the
    /// load rule reads a session id off the envelope (`Client::heard`): the
    /// typed layer skips option entries it cannot deserialize (§7.6), so an
    /// option the agent really sent in a shape v1 could not read is absent from
    /// the decoded list and present on the wire. A rule about what crossed must
    /// not turn on what this client recognized — annotating there would be a
    /// claim about the agent that the trace itself contradicts (§8).
    ///
    /// **And only where the answer carried a `configOptions` at all**, which is
    /// this rule's floor. An answer without one carries no option set to judge,
    /// and what the surface does with it is say the set did not happen (§7.6);
    /// claiming a shortfall in a list the agent never sent would be inventing a
    /// rule rather than reporting one. A `configOptions` that *is* there and is
    /// not a list of options is judged like any other list the id is not in —
    /// the id is provably not in it, no conformant agent could have sent it,
    /// and the typed layer reads it as no options at all, so the row leaves the
    /// surface and this is what says why.
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
    /// What the specification requires.
    ///
    /// One sentence per rule, not per way of breaking it: an annotation is a
    /// specification claim, and the claim does not change with the shape of the
    /// violation.
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

    /// What this agent did instead.
    ///
    /// The agent's own artifact wherever there is one — the stop reason it gave,
    /// spelled the way it spelled it — because a paraphrase would put the
    /// inspector between the reader and the wire.
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
/// Read back out of the serialization rather than matched arm by arm:
/// [`v1::StopReason`] is non-exhaustive, and a reason the schema crate grows
/// later should read as itself here rather than as a name this file guessed.
fn spelt(reason: &v1::StopReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{reason:?}"))
}
