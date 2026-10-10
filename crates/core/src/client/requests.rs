//! The requests the agent blocks on: permission, elicitation and client
//! services, and the agent withdrawing one of them.

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

/// How many blocking requests (permission, elicitation and client service)
/// may wait on the reader at once. Past it a request is refused with an error and recorded, so an agent
/// cannot pin unbounded entries in the timeline.
pub(super) const PENDING_CAPACITY: usize = 100;

/// Whether this client advertised the mode an elicitation asks in (§7.8).
///
/// The two [`client_capabilities`] claims, and the escape hatch the schema keeps
/// for modes it has no name for is deliberately not one of them: an extension
/// mode is a shape nobody here claimed, and the specification's own answer to a
/// mode the client did not advertise is an error rather than a rendering.
fn advertised(mode: &v1::ElicitationMode) -> bool {
    matches!(
        mode,
        v1::ElicitationMode::Form(_) | v1::ElicitationMode::Url(_)
    )
}

/// A mode as the agent spelled it on the wire.
///
/// Read back out of the serialization rather than matched arm by arm, for the
/// reason a stop reason is (`conformance::spelt`) and one more: the only mode
/// this is ever asked about is one this client has no name for, so a name of its
/// own invention is the one thing it must not print.
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

    /// Two requests are serviced, and the first is the one an ACP turn is
    /// made of (§7.2): `session/request_permission` becomes a pending
    /// request where the turn stopped for it, and stays pending until
    /// somebody answers it. Nothing is answered here — that is the point
    /// of it.
    ///
    /// `fs/*` and `terminal/*` are declined, which is the capability
    /// story exactly (§7.3): never advertised, so an agent calling one is
    /// behaving badly at a client that says so out loud.
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
                // A session's policy answers before anybody is asked
                // (F11), and only with a `*_once` option: the entry is
                // made first so the answer lands on a card that says
                // the policy chose it.
                let policy = self.stores.policy.get();
                if policy.pick(request.options()).is_some() {
                    self.stores.timeline.blocked(request.clone(), frame);
                    request.answer_by_policy(policy).await;
                    return;
                }
                // The list before the timeline: a screen woken by the
                // timeline moving reads a pending list that already
                // accounts for what woke it.
                self.stores.permissions.arrived(request.clone());
                self.stores.timeline.blocked(request, frame);
            }
            // A request for a method this client *does* service,
            // carrying something v1 cannot read. It is shown like any
            // other unrecognized frame (§8) — and refused as invalid
            // params rather than method-not-found, because the method
            // exists here and saying otherwise would be a lie about
            // this client rather than a finding about the agent.
            Err(problem) => self.invalid_params(id, method, problem, frame).await,
        }
    }

    /// And the second is the one this ring made serviced (§7.8): an
    /// elicitation becomes a pending request where the agent asked it,
    /// in whichever of the two modes this client advertised. A mode it
    /// did not advertise is refused with the error the specification
    /// names for exactly that — and refused rather than *declined*,
    /// because decline and cancel are answers about a reader and nothing
    /// drew this one for one.
    pub(super) async fn elicitation_requested(
        &self,
        id: Value,
        method: String,
        params: Value,
        recorded: Recorded,
    ) {
        let request = match crate::decode::from_value::<v1::CreateElicitationRequest>(params) {
            Ok(request) => request,
            // Params v1 cannot read at all: shown, and refused as
            // invalid params for the reason the permission request is —
            // the method exists here.
            Err(problem) => return self.invalid_params(id, method, problem, recorded).await,
        };
        if !advertised(&request.mode) {
            // A mode outside the two this client claims. It
            // decoded — the schema keeps an escape hatch for
            // modes it has no name for — so this is a decision
            // rather than a decode failure, and the frame is
            // evidence like every refusal §7.3 already makes.
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
        // A URL elicitation's id is registered before the
        // entry is made, so that the annotation the reuse
        // draws is read *below* the traffic it is about
        // rather than above it.
        let reused = request.elicitation_id().cloned().and_then(|elicited| {
            self.stores
                .outstanding
                .arrived(elicited.clone(), request.clone(), frame.clone())
                .map(|first| (elicited, first))
        });
        // The list before the timeline, for the permission
        // request's reason: a screen woken by the timeline
        // moving reads a pending list that already accounts
        // for what woke it.
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

    /// A request for a client service, or for anything else this client does
    /// not service itself (F3).
    ///
    /// **A rule first**: the first rule in the launch's rules file that
    /// matches answers it, and the entry says which. **Then the reader**, where
    /// the service was claimed or the reader asked to answer unserviced
    /// requests by hand — unless the rules file says unmatched requests are
    /// method-not-found. **Otherwise method-not-found**, which is what this
    /// client always said to a service it never claimed (§7.3).
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
        // Past the cap, refused rather than pinned, like the other two.
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
        // The list before the timeline, for the permission request's reason.
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

    /// The agent withdrew a request of its own with `$/cancel_request`: a
    /// pending permission or elicitation is answered with the cancellation
    /// error and marked as cancelled by the agent.
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
