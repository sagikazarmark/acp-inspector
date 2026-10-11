//! The conformance rules decided from captured frames: a cancelled turn's
//! resolution and a `session/load`'s replay. See `docs/architecture.md` §15 q7.
//!
//! A rule is armed only when this client sent the request the MUST is about,
//! and annotates only a violation the frames decide.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::conformance::{Annotation, Ending, Replay};
use crate::frame::Frame;
use crate::store::locked;
use crate::turn::TurnState;

use super::Client;

/// A `session/load` in flight, and whether anything was replayed into it.
pub(super) struct Loading {
    /// The id its answer will arrive under.
    pub(super) call: i64,
    /// The session being loaded; only its updates count as replay.
    pub(super) session: v1::SessionId,
    /// The request frame, which the annotation is read beside.
    pub(super) request: Frame,
    /// Whether any update for that session has crossed since. Any update
    /// counts, since the wire cannot tell replay from live traffic; that can
    /// only make the rule quieter, never wrong.
    pub(super) replayed: bool,
}

impl Client {
    /// Annotates a cancelled turn that did not resolve the way the
    /// specification requires ([`Ending::of`]); says nothing when it did.
    pub(super) fn judge(&self, resolved: &TurnState, cancel: Frame, answer: Option<Frame>) {
        let Some(ending) = Ending::of(resolved) else {
            return;
        };

        self.stores.timeline.annotate(
            Annotation::Cancellation(ending),
            std::iter::once(cancel).chain(answer).collect(),
        );
    }

    /// Notes that the agent said something in a session: marks a load in
    /// flight for that session as replayed, and keeps the first frame heard in
    /// each known session as evidence it had a conversation.
    ///
    /// The session id is read off the envelope, so an update v1 cannot decode
    /// still counts (§8).
    pub(super) fn heard(&self, params: &Value, frame: &Frame) {
        let Some(session) = params
            .get("sessionId")
            .and_then(Value::as_str)
            .map(v1::SessionId::new)
        else {
            return;
        };

        if let Some(loading) = locked(&self.loading)
            .as_mut()
            .filter(|loading| loading.session == session)
        {
            loading.replayed = true;
        }
        if self.known_sessions().contains(&session) {
            locked(&self.conversed)
                .entry(session)
                .or_insert_with(|| frame.clone());
        }
    }

    /// Annotates a `session/load` that answered successfully without replaying
    /// anything, for a session this connection has heard the agent talk in.
    ///
    /// A refused load owed no replay, and a session never heard from may simply
    /// be empty, so neither is annotated.
    pub(super) fn loaded(&self, id: i64, outcome: &Result<Value, v1::Error>, answer: &Frame) {
        let Some(loading) = locked(&self.loading).take_if(|load| load.call == id) else {
            return;
        };

        if outcome.is_err() || loading.replayed {
            return;
        }
        let Some(said) = locked(&self.conversed).get(&loading.session).cloned() else {
            return;
        };

        // In wire order: the agent talking in the session, the load, and the
        // answer that came without a replay.
        self.stores.timeline.annotate(
            Annotation::Replay(Replay::Nothing),
            vec![said, loading.request, answer.clone()],
        );
    }
}
