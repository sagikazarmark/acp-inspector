//! The conformance rules decided from captured frames (§15 q7): a cancelled
//! turn's resolution and a `session/load`'s replay.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::conformance::{Annotation, Ending, Replay};
use crate::frame::Frame;
use crate::store::locked;
use crate::turn::TurnState;

use super::Client;

/// A `session/load` on the wire, and what has crossed since it went out.
pub(super) struct Loading {
    /// The id its answer will arrive under, which is what says the answer is
    /// this load's.
    pub(super) call: i64,
    /// The session it is loading. Only that session's updates are its replay —
    /// an agent talking about another one has replayed nothing here.
    pub(super) session: v1::SessionId,
    /// The request frame, kept because the annotation is read beside it.
    pub(super) request: Frame,
    /// Whether any `session/update` for that session has crossed since.
    ///
    /// Anything that crossed counts, whatever it was for: nothing on the wire
    /// tells a replayed message from one the agent would have sent anyway —
    /// the last words of a turn the switch just cancelled, say. Which can only
    /// make the rule quieter than it might have been, never wrong about an
    /// agent, and quieter is the right way for a rule decided from frames to
    /// fail.
    pub(super) replayed: bool,
}

impl Client {
    /// Says so when a turn that was cancelled did not end the one way the
    /// specification allows — and says nothing at all when it did (§15 q7).
    ///
    /// **Only reached with a cancel in hand**, which is the admission rule made
    /// mechanical: the frame is proof this client asked an agent to stop a turn
    /// that was running, and the resolution is what the trace saw of the
    /// answer. Nothing here models what the agent was doing, and nothing here
    /// counts, scores or aggregates what it finds.
    ///
    /// Reading the resolution is [`Ending::of`]'s, beside the annotation it
    /// makes; what is left here is the client's own half — the frames the claim
    /// is read beside, and the store it goes in.
    pub(super) fn judge(&self, resolved: &TurnState, cancel: Frame, answer: Option<Frame>) {
        let Some(ending) = Ending::of(resolved) else {
            return;
        };

        self.stores.timeline.annotate(
            Annotation::Cancellation(ending),
            std::iter::once(cancel).chain(answer).collect(),
        );
    }

    /// Notes that the agent said something in a session (§15 q7).
    ///
    /// Two questions, both answered from the same frame: whether a load in
    /// flight has had anything replayed into it, and whether this connection
    /// has ever heard the agent talk in that session at all — which is what
    /// says there was a conversation for a load to replay.
    ///
    /// The session id is read off the envelope rather than out of a decoded
    /// notification: an update this client cannot decode is still the agent
    /// talking, and a rule about what crossed must not turn on what the typed
    /// layer recognized (§8). An update naming no session names none, and
    /// decides nothing.
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
        // The first one only: the claim it is kept for is that the session was
        // not empty, and one message is the whole of that.
        if self.known_sessions().contains(&session) {
            locked(&self.conversed)
                .entry(session)
                .or_insert_with(|| frame.clone());
        }
    }

    /// Says so when a `session/load` answered before the conversation had been
    /// replayed — and says nothing at all where the frames cannot decide it
    /// (§15 q7).
    ///
    /// **Only reached with a load in hand**, which is this rule's half of the
    /// admission rule: the frame is proof this client asked an agent to load a
    /// session, which is the only condition under which the ordering MUST
    /// applies.
    ///
    /// Two things draw nothing, and both are the boundary rather than mercy. A
    /// load the agent *refused* restored no session, so there was no
    /// conversation it owed anybody — the MUST is on a load that answers. And a
    /// load of a session this connection has never heard a word in replayed
    /// nothing into a session nothing is known to have been said in, which is
    /// the shape the trace cannot tell from an empty session; an inspector that
    /// annotated there would be inventing a rule rather than reporting one.
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

        // Chronologically, which is how the evidence reads: the agent talking in
        // that session, the load that asked for it back, and the answer that
        // came without it.
        self.stores.timeline.annotate(
            Annotation::Replay(Replay::Nothing),
            vec![said, loading.request, answer.clone()],
        );
    }
}
