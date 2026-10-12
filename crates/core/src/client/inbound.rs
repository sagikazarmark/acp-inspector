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
    /// Handles one frame from the agent: decodes `session/update` as v1,
    /// services or refuses requests, hands answers to their callers, and shows
    /// anything else as unrecognized (§8). No frame is dropped.
    pub(crate) async fn receive(&self, received: Received) {
        let Received { frame, captured } = received;
        // Entries keep the trace position of each frame, which links the
        // timeline to the trace.
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
                    // Another session's update is shown, marked foreign, and
                    // changes nothing.
                    if let Some(session) = session.filter(|session| self.foreign(session)) {
                        self.stores.timeline.unrecognized(
                            Unrecognized::ForeignSession { session },
                            recorded(frame),
                        );
                        return;
                    }
                    // Before decoding: an update v1 cannot name is still the
                    // agent talking in that session, which the replay rule
                    // counts.
                    self.heard(&params, &frame);
                    match crate::decode::from_value::<v1::SessionNotification>(params) {
                        Ok(notification) => {
                            // Settings and reports before the timeline, so a
                            // screen woken by the timeline reads stores that
                            // include it.
                            self.reconfigured(&notification);
                            self.reported(&notification);
                            self.stores.timeline.update(notification, recorded(frame));
                        }
                        // An unstable, extension or v2 variant: shown with the
                        // decoder's complaint.
                        Err(problem) => self.stores.timeline.unrecognized(
                            Unrecognized::Undecodable {
                                method: Some(method),
                                problem: problem.to_string(),
                            },
                            recorded(frame),
                        ),
                    }
                } else if method == v1::CLIENT_METHOD_NAMES.elicitation_complete {
                    // A URL elicitation finished out of band (§7.8). A
                    // completion for an unknown id is ignored, as the
                    // specification asks, and shown as unsolicited.
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
                        .unrecognized(Unrecognized::Ignored { method }, recorded(frame));
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

            // Client services and anything else (`docs/client-services.md`).
            Incoming::Request { id, method, params } => {
                self.service(id, method, params, recorded(frame)).await;
            }

            Incoming::Answer { id, outcome } => match ours(&id) {
                // An answer to a hand-sent request drives no store and arms no
                // rule; it is read off the trace.
                Some(id) if self.rpc.is_hand_sent(id) => {
                    self.rpc.answer(id, outcome);
                }
                // The stores first, the caller second, so the screen does not
                // depend on anyone awaiting the call.
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

    /// The connection is gone: fails every pending call, refuses every set and
    /// driven advertisement still in flight, abandons every blocking request,
    /// and ends an open turn (§6.1).
    pub(crate) fn disconnected(&self) {
        self.rpc.close();
        *locked(&self.awaiting) = None;
        // A load or open that will never be answered decides nothing.
        *locked(&self.loading) = None;
        *locked(&self.opening) = None;
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
        // Taking the whole map also means an answer arriving after this has no
        // registration to land in.
        for capability in std::mem::take(&mut *locked(&self.driving))
            .into_values()
            .flatten()
        {
            self.stores
                .driven
                .update(|record| record.refused(capability, CallError::Disconnected));
        }
        // Nobody is left to answer: every blocking request, whatever its scope,
        // and every outstanding elicitation id ends with the connection.
        self.stores.permissions.abandon();
        self.stores.elicitations.abandon();
        self.stores.services.abandon();
        self.stores.outstanding.clear();
        // Unless a newer connection already owns the policy.
        if self.stores.generation.load(Ordering::SeqCst) == self.generation {
            self.stores.policy.set(PermissionPolicy::Ask);
        }
        self.end_turn(TurnState::Failed(CallError::Disconnected));
    }

    /// The sessions an update may be about: the live one, the one the reader
    /// last saw opened, and the one an open in flight is asking for.
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

    /// Folds what a `session_info_update` or `usage_update` says into its
    /// session's report. The timeline keeps the update either way.
    fn reported(&self, notification: &v1::SessionNotification) {
        let session = &notification.session_id;
        match &notification.update {
            v1::SessionUpdate::SessionInfoUpdate(update) => {
                self.stores
                    .reports
                    .update(|reports| reports.info(session, update));
            }
            v1::SessionUpdate::UsageUpdate(update) => {
                self.stores
                    .reports
                    .update(|reports| reports.usage(session, update));
            }
            _ => {}
        }
    }

    /// Whether an update names a session other than the known ones. With none
    /// known yet (a `session/new` unanswered), nothing is foreign.
    pub(super) fn foreign(&self, session: &v1::SessionId) -> bool {
        let known = self.known_sessions();
        !known.is_empty() && !known.contains(session)
    }

    /// The transport dropped a frame for its length. If it was the answer to
    /// one of this client's calls, everything waiting on that answer is told it
    /// will not come.
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
