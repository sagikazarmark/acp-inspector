//! Everything the agent sends, dispatched in one place, and the teardown when
//! the connection goes away.

use std::sync::atomic::Ordering;

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::blocking::RequestId;
use crate::call::CallError;
use crate::connection::Received;
use crate::frame::{Frame, Omission};
use crate::permission::PermissionPolicy;
use crate::rpc::{Incoming, answer_id, classify, mistyped, ours};
use crate::store::locked;
use crate::timeline::{Recorded, Unrecognized};
use crate::turn::TurnState;

use super::Client;

impl Client {
    /// Everything the agent says, in one place.
    ///
    /// The four shapes a frame can arrive in, and what this client does with
    /// each: decode a `session/update` as v1, refuse a method it does not
    /// service, hand an answer to whoever asked, and *show* whatever is left
    /// (§8). No arm drops a frame.
    pub(crate) async fn receive(&self, received: Received) {
        let Received { frame, captured } = received;
        // The frame and the row it crossed as, paired for whichever store keeps
        // it: an entry is made of frames, and where the trace put each of them
        // is half of what makes the two screens one thing (§6.2).
        let recorded = |frame: Frame| Recorded { frame, captured };
        if let Some(omission) = frame.omission() {
            self.omitted(&frame, omission);
            self.stores.timeline.unrecognized(
                Unrecognized::Dropped {
                    length: omission.length,
                    limit: omission.limit,
                },
                recorded(frame),
            );
            return;
        }
        match classify(&frame) {
            Incoming::Notification { method, params } => {
                if method == v1::CLIENT_METHOD_NAMES.session_update {
                    let session = params
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .map(v1::SessionId::new);
                    // Another session's update changes nothing here: shown, and
                    // marked as not the live session's.
                    if let Some(session) = session.filter(|session| self.foreign(session)) {
                        self.stores.timeline.unrecognized(
                            Unrecognized::ForeignSession { session },
                            recorded(frame),
                        );
                        return;
                    }
                    // Before the decoding, and off the envelope rather than out
                    // of it: an update variant v1 has no name for is still the
                    // agent talking in that session, and the load rule is about
                    // what crossed rather than about what this client could read
                    // (§8).
                    self.heard(&params, &frame);
                    match crate::decode::from_value::<v1::SessionNotification>(params) {
                        Ok(notification) => {
                            // The settings before the timeline, for the reason
                            // the pending list goes before it: a screen woken
                            // by the timeline moving reads a surface that
                            // already accounts for what woke it.
                            self.reconfigured(&notification);
                            self.stores.timeline.update(notification, recorded(frame));
                        }
                        // An update variant v1 has no name for: an unstable-v1
                        // one, an extension, or v2 arriving early. It is shown,
                        // and the decoder's complaint is shown with it.
                        Err(problem) => self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        ),
                    }
                } else if method == v1::CLIENT_METHOD_NAMES.elicitation_complete {
                    // The one notification that names something rather than
                    // reporting it (§7.8): a URL elicitation's out-of-band half
                    // finished, and the entry it names says so. A completion for
                    // an id nobody is holding is *ignored*, which the
                    // specification asks for and this tool spells as unsolicited
                    // traffic — shown, and acted on by nothing.
                    match crate::decode::from_value::<v1::CompleteElicitationNotification>(params) {
                        Ok(notification) => {
                            let completed = self
                                .stores
                                .outstanding
                                .completed(&notification.elicitation_id);
                            if completed.is_empty() {
                                self.stores
                                    .timeline
                                    .unrecognized(Unrecognized::Unsolicited, recorded(frame));
                            } else {
                                let named: Vec<RequestId> = completed
                                    .iter()
                                    .map(|request| request.id().clone())
                                    .collect();
                                self.stores.timeline.completed(&named, recorded(frame));
                            }
                        }
                        Err(problem) => self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        ),
                    }
                } else if method == v1::PROTOCOL_LEVEL_METHOD_NAMES.cancel_request {
                    self.withdrawn(method, params, recorded(frame)).await;
                } else {
                    self.stores
                        .timeline
                        .unrecognized(Unrecognized::NotServiced { method }, recorded(frame));
                }
            }

            // Past the cap, a blocking request is refused rather than pinned.
            Incoming::Request { id, method, .. }
                if (method == v1::CLIENT_METHOD_NAMES.session_request_permission
                    || method == v1::CLIENT_METHOD_NAMES.elicitation_create)
                    && self.saturated() =>
            {
                self.over_capacity(id, method, recorded(frame)).await;
            }

            Incoming::Request { id, method, params }
                if method == v1::CLIENT_METHOD_NAMES.session_request_permission =>
            {
                self.permission_requested(id, method, params, recorded(frame))
                    .await;
            }

            Incoming::Request { id, method, params }
                if method == v1::CLIENT_METHOD_NAMES.elicitation_create =>
            {
                self.elicitation_requested(id, method, params, recorded(frame))
                    .await;
            }

            // Everything else the agent asks of this client: answered by a
            // rules file where one matches, by the reader where the service
            // was claimed or the reader opted in, and method-not-found
            // otherwise (F3, `docs/client-services.md`).
            Incoming::Request { id, method, params } => {
                self.service(id, method, params, recorded(frame)).await;
            }

            // Whether the id could be one of ours is the envelope's question,
            // asked once (`rpc::ours`) and answered for both of the things that
            // happen to an answer.
            Incoming::Answer { id, outcome } => match ours(&id) {
                // An answer to a request a person sent by hand (F2) is theirs
                // to read off the trace: it drives no store and arms no rule.
                Some(id) if self.rpc.is_hand_sent(id) => {
                    self.rpc.answer(id, outcome);
                }
                // The stores first, the caller second: what the screen shows
                // must not depend on anyone being awake to await the call.
                Some(id) => {
                    self.note_login(&outcome);
                    self.drove(id, &outcome);
                    self.settle(id, &outcome, &frame);
                    self.loaded(id, &outcome, &frame);
                    self.opened(id, &outcome);
                    self.changed_mode(id, &outcome);
                    self.set_answered(id, &outcome, &frame);
                    if !self.rpc.answer(id, outcome) {
                        self.stores
                            .timeline
                            .unrecognized(Unrecognized::Unsolicited, recorded(frame));
                    }
                }
                None => {
                    let unrecognized = match mistyped(&id).filter(|&call| self.rpc.is_pending(call))
                    {
                        Some(call) => Unrecognized::MistypedId {
                            id: id.to_string(),
                            call,
                        },
                        None => Unrecognized::Unsolicited,
                    };
                    self.stores
                        .timeline
                        .unrecognized(unrecognized, recorded(frame));
                }
            },

            Incoming::Unaddressed(error) => self
                .stores
                .timeline
                .unrecognized(Unrecognized::Unaddressed { error }, recorded(frame)),

            Incoming::Unreadable(problem) => self.stores.timeline.unrecognized(
                Unrecognized::Undecodable {
                    method: None,
                    problem,
                },
                recorded(frame),
            ),
        }
    }

    /// The connection is gone: nothing more will be answered, and a turn that
    /// was open is over however it looked a moment ago.
    ///
    /// Without this, an agent that dies mid-turn leaves a prompt awaiting an
    /// answer from a process that no longer exists, and a screen that still
    /// says the turn is running (§6.1: a transport that cannot deliver is
    /// evidence, never a hang).
    pub(crate) fn disconnected(&self) {
        self.rpc.close();
        *locked(&self.awaiting) = None;
        // A load that will never be answered decides nothing: the rule is about
        // what a load that *answers* did first, and this one did not answer.
        *locked(&self.loading) = None;
        // And a session that will never be opened publishes nothing.
        *locked(&self.opening) = None;
        // A set that will never be answered is refused by the teardown instead,
        // the way the turn below is ended by it: an answer nobody will ever
        // give is news, and a control left waiting on one would be the window
        // showing a question that has stopped being one.
        if let Some(changing) = locked(&self.changing_mode).take() {
            self.stores
                .settings
                .update(|settings| settings.mode_refused(changing.mode, CallError::Disconnected));
        }
        if let Some(setting) = locked(&self.setting_option).take() {
            self.stores.settings.update(|settings| {
                settings.option_refused(setting.option, setting.value, CallError::Disconnected)
            });
        }
        // And every advertisement still being driven is refused by the teardown
        // too (§7.7), for the reason the two setters above are: a connection
        // that died under a call in flight is where `AuthState` and both setters
        // already put a refusal, and a row left reading *not driven* about a
        // capability somebody drove would be the panel keeping a secret this
        // record exists to tell.
        //
        // **Emptying it is what closes the late-answer race**, which is why the
        // whole map goes rather than the ones a caller happens to be awaiting:
        // an answer that arrives after this has no registration to land in.
        for capability in std::mem::take(&mut *locked(&self.driving))
            .into_values()
            .flatten()
        {
            self.stores
                .driven
                .update(|record| record.refused(capability, CallError::Disconnected));
        }
        // Every request the agent left blocking is given up on: there is
        // nobody to answer to any more, and a panel that went on offering
        // buttons would be offering to send a frame into a closed pipe. Every
        // elicitation with it, whatever it was scoped to — a request-scoped one
        // outlives its turn and nothing outlives the connection it was asked on
        // (§7.8), which is the same sentence the outstanding ids answer to.
        self.stores.permissions.abandon();
        self.stores.elicitations.abandon();
        self.stores.services.abandon();
        self.stores.outstanding.clear();
        // The policy ends with the connection, unless a newer connection has
        // already started and owns it.
        if self.stores.generation.load(Ordering::SeqCst) == self.generation {
            self.stores.policy.set(PermissionPolicy::Ask);
        }
        self.end_turn(TurnState::Failed(CallError::Disconnected));
    }

    /// The sessions an update may be about: the live one, the one the reader
    /// last saw opened, and the one an open on the wire is asking for.
    pub(super) fn known_sessions(&self) -> Vec<v1::SessionId> {
        let opening = locked(&self.opening)
            .as_ref()
            .and_then(|opening| opening.session.clone());
        [
            self.stores.session.get(),
            locked(&self.opened_last).clone(),
            opening,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Whether an update names a session other than the ones this client
    /// knows of. With none known yet — a `session/new` still unanswered —
    /// nothing is foreign, because nothing can be shown to be.
    pub(super) fn foreign(&self, session: &v1::SessionId) -> bool {
        let known = self.known_sessions();
        !known.is_empty() && !known.contains(session)
    }

    /// The transport dropped a frame for its length. Where the placeholder
    /// shows it was an answer to a call of this client's, everything waiting
    /// on that answer is told it will not come.
    pub(super) fn omitted(&self, frame: &Frame, omission: Omission) {
        let Some(id) = answer_id(frame.as_str()) else {
            return;
        };
        let error = CallError::AnswerDropped {
            length: omission.length,
        };
        if let Some(capabilities) = locked(&self.driving).remove(&id) {
            self.stores.driven.update(|record| {
                let mut moved = false;
                for capability in capabilities {
                    moved |= record.refused(capability, error.clone());
                }
                moved
            });
        }
        locked(&self.loading).take_if(|load| load.call == id);
        locked(&self.opening).take_if(|open| open.call == id);
        if let Some(changing) = locked(&self.changing_mode).take_if(|change| change.call == id) {
            self.stores
                .settings
                .update(|settings| settings.mode_refused(changing.mode, error.clone()));
        }
        if let Some(setting) = locked(&self.setting_option).take_if(|set| set.call == id) {
            self.stores.settings.update(|settings| {
                settings.option_refused(setting.option, setting.value, error.clone())
            });
        }
        self.turn_failed(Some(id), &error);
        self.rpc.fail(id, error);
    }
}
