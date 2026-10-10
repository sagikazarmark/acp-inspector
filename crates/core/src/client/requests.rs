//! The requests the agent blocks on: permission (§7.2), elicitation (§7.8) and
//! client services (`docs/client-services.md`), and the agent withdrawing one.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::blocking::{RequestId, Resolver};
use crate::conformance::{Annotation, Reuse};
use crate::elicitation::ElicitationRequest;
use crate::permission::PermissionRequest;
use crate::script::{Scripted, Unmatched};
use crate::service::{AnsweredBy, ServiceReply, ServiceRequest};
use crate::timeline::{EntryId, Recorded, Unrecognized};

use super::{Client, encode};

/// How many blocking requests may wait on the reader at once. Past it a
/// request is recorded and refused, so an agent cannot pin unbounded entries.
pub(super) const PENDING_CAPACITY: usize = 100;

/// Whether this client advertised the mode an elicitation asks in: form or
/// URL. An extension mode was never claimed, so it is refused.
fn advertised(mode: &v1::ElicitationMode) -> bool {
    matches!(
        mode,
        v1::ElicitationMode::Form(_) | v1::ElicitationMode::Url(_)
    )
}

/// A mode as the agent spelled it on the wire, read back from the
/// serialization: it is only asked about modes this client has no name for.
fn spelt_mode(mode: &v1::ElicitationMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|value| {
            value
                .get("mode")
                .and_then(Value::as_str)
                .map(|mode| format!("mode `{mode}`"))
        })
        .unwrap_or_else(|| "an elicitation mode with no name on the wire".to_owned())
}

impl Client {
    /// Refuses a blocking request because the reader already holds
    /// [`PENDING_CAPACITY`] of them.
    pub(super) async fn over_capacity(&self, id: Value, method: String, frame: Recorded) {
        self.stores.timeline.unrecognized(
            Unrecognized::OverCapacity {
                method,
                limit: PENDING_CAPACITY,
            },
            frame,
        );
        let refusal = v1::Error::new(
            i32::from(v1::ErrorCode::InternalError),
            format!("the client already has {PENDING_CAPACITY} unanswered requests"),
        );
        let _ = self.rpc.respond(&id, Err(refusal)).await;
    }

    /// `session/request_permission`: becomes a pending request on the timeline
    /// until somebody answers it, unless the session's policy answers first.
    pub(super) async fn permission_requested(
        &self,
        id: Value,
        method: String,
        params: Value,
        frame: Recorded,
    ) {
        match crate::decode::from_value::<v1::RequestPermissionRequest>(params) {
            Ok(request) => {
                let request = PermissionRequest::new(
                    RequestId::new(id),
                    request,
                    Resolver::new(self.rpc.clone(), self.stores.timeline.ticker()),
                );
                // The policy answers only with a `*_once` option. The entry is
                // made first so the answer lands on a card that says so.
                let policy = self.stores.policy.get();
                if policy.pick(request.options()).is_some() {
                    self.stores.timeline.blocked(request.clone(), frame);
                    request.answer_by_policy(policy).await;
                    return;
                }
                // The list before the timeline, so a screen woken by the
                // timeline reads a pending list that includes it.
                self.stores.permissions.arrived(request.clone());
                self.stores.timeline.blocked(request, frame);
            }
            // Invalid params rather than method-not-found: the method is
            // serviced here.
            Err(problem) => self.invalid_params(id, method, problem, frame).await,
        }
    }

    /// `elicitation/create`: becomes a pending request on the timeline. A mode
    /// this client did not advertise is refused with invalid params, as the
    /// specification says, rather than declined on the reader's behalf.
    pub(super) async fn elicitation_requested(
        &self,
        id: Value,
        method: String,
        params: Value,
        recorded: Recorded,
    ) {
        let request = match crate::decode::from_value::<v1::CreateElicitationRequest>(params) {
            Ok(request) => request,
            Err(problem) => return self.invalid_params(id, method, problem, recorded).await,
        };
        if !advertised(&request.mode) {
            self.stores.timeline.unrecognized(
                Unrecognized::Unadvertised {
                    method,
                    detail: spelt_mode(&request.mode),
                },
                recorded,
            );
            let _ = self
                .rpc
                .respond(&id, Err(v1::Error::invalid_params()))
                .await;
            return;
        }

        let frame = recorded.frame.clone();
        let request = ElicitationRequest::new(
            RequestId::new(id),
            request,
            &frame,
            Resolver::new(self.rpc.clone(), self.stores.timeline.ticker()),
        );
        // The id is registered before the entry is made, so a reuse
        // annotation lands below the traffic it is about.
        let reused = request.elicitation_id().cloned().and_then(|elicited| {
            self.stores
                .outstanding
                .arrived(elicited.clone(), request.clone(), frame.clone())
                .map(|first| (elicited, first))
        });
        // The list before the timeline, as for permissions.
        self.stores.elicitations.arrived(request.clone());
        self.stores.timeline.elicited(request, recorded);
        if let Some((elicited, first)) = reused {
            self.stores.timeline.annotate(
                Annotation::ElicitationId(Reuse::Outstanding(elicited)),
                vec![first, frame],
            );
        }
    }

    /// Shows a serviced request whose params v1 cannot read, and refuses it as
    /// invalid params.
    async fn invalid_params(
        &self,
        id: Value,
        method: String,
        problem: impl ToString,
        frame: Recorded,
    ) {
        self.stores.timeline.unrecognized(
            Unrecognized::Undecodable {
                method: Some(method),
                problem: problem.to_string(),
            },
            frame,
        );
        let _ = self
            .rpc
            .respond(&id, Err(v1::Error::invalid_params()))
            .await;
    }

    /// Any other request: a client service, or a method this client does not
    /// service itself.
    ///
    /// The first matching rule in the launch's rules file answers it. Otherwise
    /// the reader answers it if the service was claimed or the launch opted in
    /// to answering unserviced requests, unless the rules file says unmatched
    /// requests are method-not-found. Everything else is method-not-found.
    pub(super) async fn service(&self, id: Value, method: String, params: Value, frame: Recorded) {
        let claimed = self.services.claims(&method);
        let script = self.services.script.as_ref();
        if let Some((rule, matched)) = script.and_then(|script| script.find(&method, &params)) {
            let request = ServiceRequest::new(
                RequestId::new(id),
                method,
                params,
                claimed,
                Resolver::new(self.rpc.clone(), self.stores.timeline.ticker()),
            );
            self.stores.timeline.serviced(request.clone(), frame);
            let reply = match &matched.answer {
                Scripted::Result(result) => ServiceReply::Result(result.clone()),
                Scripted::Error(error) => ServiceReply::Error(error.clone()),
            };
            let _ = request.answer(reply, AnsweredBy::Script { rule }).await;
            return;
        }
        let refused_by_script =
            script.is_some_and(|script| script.unmatched() == Unmatched::MethodNotFound);
        let by_hand = (claimed || self.services.answer_unserviced) && !refused_by_script;
        if !by_hand {
            self.stores
                .timeline
                .unrecognized(Unrecognized::NotServiced { method }, frame);
            let _ = self
                .rpc
                .respond(&id, Err(v1::Error::method_not_found()))
                .await;
            return;
        }
        if self.saturated() {
            return self.over_capacity(id, method, frame).await;
        }
        let request = ServiceRequest::new(
            RequestId::new(id),
            method,
            params,
            claimed,
            Resolver::new(self.rpc.clone(), self.stores.timeline.ticker()),
        );
        self.stores.services.arrived(request.clone());
        self.stores.timeline.serviced(request, frame);
    }

    /// Whether the reader already has as many blocking requests as it holds.
    pub(super) fn saturated(&self) -> bool {
        self.stores.permissions.waiting().len()
            + self.stores.elicitations.waiting().len()
            + self.stores.services.waiting().len()
            >= PENDING_CAPACITY
    }

    /// The agent withdrew one of its requests with `$/cancel_request`: the
    /// pending request is answered with the cancellation error and marked as
    /// withdrawn by the agent.
    pub(super) async fn withdrawn(&self, method: String, params: Value, frame: Recorded) {
        let notification = match crate::decode::from_value::<v1::CancelRequestNotification>(params)
        {
            Ok(notification) => notification,
            Err(problem) => {
                return self.stores.timeline.unrecognized(
                    Unrecognized::Undecodable {
                        method: Some(method),
                        problem: problem.to_string(),
                    },
                    frame,
                );
            }
        };
        let id = RequestId::new(encode(&notification.request_id));
        let permission = self
            .stores
            .permissions
            .waiting()
            .into_iter()
            .find(|request| request.id() == &id);
        let elicitation = self
            .stores
            .elicitations
            .waiting()
            .into_iter()
            .find(|request| request.id() == &id);
        let service = self
            .stores
            .services
            .waiting()
            .into_iter()
            .find(|request| request.id() == &id);
        let entry = match (&permission, &elicitation, &service) {
            (Some(_), _, _) => EntryId::Permission(id),
            (None, Some(_), _) => EntryId::Elicitation(id),
            (None, None, Some(_)) => EntryId::Service(id),
            (None, None, None) => {
                return self
                    .stores
                    .timeline
                    .unrecognized(Unrecognized::Unsolicited, frame);
            }
        };
        if !self.stores.timeline.attach(&entry, frame.clone()) {
            self.stores
                .timeline
                .unrecognized(Unrecognized::Unsolicited, frame);
        }
        if let Some(request) = permission {
            let _ = request.withdrawn().await;
        } else if let Some(request) = elicitation {
            let _ = request.withdrawn().await;
        } else if let Some(request) = service {
            let _ = request.withdrawn().await;
        }
    }
}
