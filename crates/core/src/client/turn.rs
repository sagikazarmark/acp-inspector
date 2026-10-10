//! A prompt turn (§7.1, §7.2): sending it, cancelling it, and settling it from
//! the frame that ends it.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::driven::AgentCapability;
use crate::frame::Frame;
use crate::store::locked;
use crate::turn::{TurnOutcome, TurnState};

use super::lifecycle::Debt;
use super::{Client, decode, encode};

impl Client {
    /// `session/prompt`: ordered content blocks; each media kind is advertised.
    ///
    /// The turn is registered before the frame goes out, so the answer cannot
    /// arrive before the reader knows what it is an answer to.
    ///
    /// **A turn that fails says so in the store**, whether it failed on the way
    /// out, on the way back, or before it was a frame at all. The screen reads
    /// the turn rather than the call (§11 seam 2), so a prompt that quietly
    /// answered an error to whoever asked would be a composer whose button does
    /// nothing visible.
    ///
    /// **But only while the turn that failed is still the one on screen.** A
    /// prompt that was walked away from — the session switched, closed or
    /// deleted under it (§7.5) — answers its own caller long afterwards, and a
    /// failure written then would paint one session's turn over another's. So
    /// the guard is the id: `awaiting` is emptied by every way of leaving a
    /// session ([`forget`](Self::forget)), by the connection ending, and by the
    /// answer that settled the turn, and a failure whose id is no longer the
    /// awaited one says nothing here. The frame that carried it is in the trace
    /// either way (§8).
    ///
    /// A failure that happens *before* the frame goes out is nobody else's:
    /// nothing was registered, so nothing can have replaced it, and a composer
    /// whose button left no trace anywhere would be the screen keeping a secret
    /// the store is there to tell.
    pub(crate) fn submit_prompt(
        &self,
        content: Vec<v1::ContentBlock>,
    ) -> Result<tokio::task::JoinHandle<Result<v1::StopReason, CallError>>, CallError> {
        let session = self
            .session()
            .inspect_err(|error| self.turn_failed(None, error))?;
        let has_image = content
            .iter()
            .any(|block| matches!(block, v1::ContentBlock::Image(_)));
        let has_audio = content
            .iter()
            .any(|block| matches!(block, v1::ContentBlock::Audio(_)));
        let has_embedded = content
            .iter()
            .any(|block| matches!(block, v1::ContentBlock::Resource(_)));
        if content.iter().any(|block| {
            !(matches!(
                block,
                v1::ContentBlock::Text(_) | v1::ContentBlock::Image(_) | v1::ContentBlock::Audio(_)
            ) || matches!(block, v1::ContentBlock::Resource(resource) if match &resource.resource {
                v1::EmbeddedResourceResource::TextResourceContents(_) => true,
                v1::EmbeddedResourceResource::BlobResourceContents(blob) => blob.mime_type.as_deref() == Some("application/pdf"),
                _ => false,
            }))
        }) {
            return Err(CallError::UnsupportedPromptContent);
        }
        if has_image
            && !self
                .stores
                .agent
                .get()
                .is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.image)
        {
            let error = CallError::ImageNotAdvertised;
            self.turn_failed(None, &error);
            return Err(error);
        }
        let request = v1::PromptRequest::new(session, content);
        if has_audio
            && !self
                .stores
                .agent
                .get()
                .is_some_and(|agent| agent.agent_capabilities.prompt_capabilities.audio)
        {
            let error = CallError::AudioNotAdvertised;
            self.turn_failed(None, &error);
            return Err(error);
        }
        let driven: Vec<_> = [
            (has_image, AgentCapability::Image),
            (has_audio, AgentCapability::Audio),
            (has_embedded, AgentCapability::EmbeddedContext),
        ]
        .into_iter()
        .filter_map(|(present, kind)| present.then_some(kind))
        .collect();

        if has_embedded
            && !self.stores.agent.get().is_some_and(|agent| {
                agent
                    .agent_capabilities
                    .prompt_capabilities
                    .embedded_context
            })
        {
            let error = CallError::EmbeddedContextNotAdvertised;
            self.turn_failed(None, &error);
            return Err(error);
        }

        let call = self
            .rpc
            .prepare_bounded(
                v1::AGENT_METHOD_NAMES.session_prompt,
                encode(&request),
                10 * 1024 * 1024,
            )
            .inspect_err(|error| self.turn_failed(None, error))?;
        let id = call.id();
        *locked(&self.awaiting) = Some(id);
        self.registers(id, &driven);
        // Nothing has been asked to stop *this* turn, so nothing is watching
        // it. A cancel still armed here belongs to a turn the user prompted
        // over — and that turn draws no annotation, because the connection is
        // still open and its answer may yet cross. If it does it lands as an
        // unsolicited entry (§8), which is the visible thing to have happened
        // and better than a claim that "never" had arrived.
        *locked(&self.cancelled) = None;
        self.stores.turn.set(TurnState::InFlight);
        // And the span the timeline stamps its entries with. Opened here rather
        // than where the state is read, because what belongs to a turn is
        // everything after the prompt went out — including the traffic that
        // arrives before this call returns.
        self.stores.timeline.open_turn();

        self.rpc.enqueue(&call).inspect_err(|error| {
            self.turn_failed(Some(id), error);
            self.drive_failed(Some(id), &driven, error);
        })?;
        let client = self.clone();
        Ok(tokio::spawn(async move {
            client.finish_prompt(call, driven).await
        }))
    }

    pub(super) async fn finish_prompt(
        &self,
        call: crate::rpc::Call,
        driven: Vec<AgentCapability>,
    ) -> Result<v1::StopReason, CallError> {
        let id = call.id();
        let outcome = match self.rpc.wait(call).await {
            Ok(result) => decode::<v1::PromptResponse>(result).map(|response| response.stop_reason),
            Err(error) => {
                self.drive_failed(Some(id), &driven, &error);
                Err(error)
            }
        };
        if let Err(error) = &outcome {
            self.turn_failed(Some(id), error);
        }
        outcome
    }

    /// Says on screen that a turn failed, where the turn that failed is still
    /// the one the screen is about.
    ///
    /// `registered` is the id the turn went out under, or `None` for a failure
    /// that happened before it was a frame at all. The two are judged
    /// differently for the reason [`prompt`](Self::prompt) states: an unregistered
    /// failure is this call's alone, and a registered one belongs to whichever
    /// turn the store is about now.
    ///
    /// Where the answer *did* cross, the reader has already settled the turn
    /// from the frame ([`settle`](Self::settle)) and emptied `awaiting` doing
    /// it, so this says nothing over it — which is the same conclusion by the
    /// same rule, and why the turn state stays the stream's (§11 seam 2).
    pub(super) fn turn_failed(&self, registered: Option<i64>, error: &CallError) {
        if let Some(id) = registered {
            let mut awaiting = locked(&self.awaiting);
            if *awaiting != Some(id) {
                return;
            }
            *awaiting = None;
        }
        self.stores.turn.set(TurnState::Failed(error.clone()));
        self.stores
            .timeline
            .close_turn(TurnOutcome::Failed(error.clone()));
    }

    /// `session/cancel`: a notification, then the answers it owes, and then a
    /// wait (§7.1).
    ///
    /// Cancelling does not end the turn — the agent does, by resolving the
    /// prompt with `cancelled`. What this changes is what the turn is *doing*,
    /// which is now waiting for an agent that has been asked to stop.
    ///
    /// **Every request the turn was blocked on is answered `cancelled`**, which
    /// the protocol requires of a client that cancels (§7.2) and which is also
    /// the only way the agent gets to stop: an agent waiting on a permission it
    /// will never be told about cannot end the turn it was asked to end. They
    /// are answered *after* the notification, so the agent knows why they are
    /// being cancelled by the time they are.
    pub(crate) async fn cancel(&self) -> Result<(), CallError> {
        let session = self.session()?;
        let sent = self
            .rpc
            .notify(
                v1::AGENT_METHOD_NAMES.session_cancel,
                encode(&v1::CancelNotification::new(session)),
            )
            .await?;

        // The turn's own: a request-scoped elicitation is tied to a call rather
        // than to a conversation (§7.8), so cancelling this turn is not a
        // sentence about it and its row is still there to answer.
        self.answer_cancelled(Debt::TheTurns).await;

        // Only if the turn is still open: an agent that answered while the
        // cancel was on its way has already ended the turn, and this is not
        // news that outranks it.
        //
        // **Arming the conformance rule is that same decision, so it is made
        // without letting go in between.** The reader task is the other
        // producer here (`settle`), and it takes these two in this same order,
        // so a cancel going out as its answer arrives resolves one way or the
        // other and never into both halves of neither: either the cancel arms
        // before the answer is judged, or it finds a turn already ended and
        // arms nothing. Two locks taken apart is exactly the gap
        // `Field::update` exists to close.
        let mut cancelled = locked(&self.cancelled);
        let open = self.stores.turn.update(|state| {
            let open = *state == TurnState::InFlight;
            if open {
                *state = TurnState::Cancelling;
            }
            open
        });
        if open {
            // A second cancel for a turn already cancelling leaves the first
            // one's frame in place: that is the one the agent was answering.
            *cancelled = Some(sent);
        }
        Ok(())
    }

    /// Settles the turn from the frame that ended it, when this answer is the
    /// one the turn was waiting for.
    pub(super) fn settle(&self, id: i64, outcome: &Result<Value, v1::Error>, frame: &Frame) {
        {
            let mut awaiting = locked(&self.awaiting);
            if *awaiting != Some(id) {
                return;
            }
            *awaiting = None;
        }

        let resolved = match outcome {
            Ok(result) => match decode::<v1::PromptResponse>(result.clone()) {
                Ok(response) => TurnState::Ended(response.stop_reason),
                Err(error) => TurnState::Failed(error),
            },
            Err(error) => TurnState::Failed(CallError::Rejected(error.clone())),
        };

        // Whether a cancel was watching this turn, and the end of the turn
        // itself, under one hold and in the order `cancel` takes them: taken
        // exactly once, so an answer arriving as a cancel goes out is judged by
        // that cancel or by none, and never by the next turn's.
        let cancelled = {
            let mut cancelled = locked(&self.cancelled);
            let armed = cancelled.take();
            self.stores.turn.set(resolved.clone());
            armed
        };
        if let Some(cancel) = cancelled {
            self.judge(&resolved, cancel, Some(frame.clone()));
        }
        // After the annotation and not before it: a conformance annotation is a
        // finding *about* this turn, so it belongs inside the span rather than
        // in whatever arrives next.
        if let Some(outcome) = TurnOutcome::of(&resolved) {
            self.stores.timeline.close_turn(outcome);
        }

        // **A blocking request belongs to the turn it blocked** (§7.2): an
        // agent that ended its turn is not waiting on anything any more,
        // whatever it left unanswered. Given up on rather than quietly dropped,
        // so that the panel says nobody answered it instead of going on
        // offering buttons for a turn that is over.
        //
        // **Unless nothing tied it to the turn** (§7.8). An elicitation scoped
        // to a JSON-RPC call rather than to a session is not this turn's to give
        // up on — the agent may well still be waiting on it, its row survives
        // the turn boundary like every row, and the connection ending is what
        // ends it.
        self.stores.permissions.abandon();
        self.stores
            .elicitations
            .abandon_unless(|request| request.session_id().is_none());
        self.stores
            .services
            .abandon_unless(|request| request.session_id().is_none());
    }

    /// Ends an open turn, and leaves a settled one alone: a turn the agent
    /// already ended keeps the reason the agent gave it.
    pub(super) fn end_turn(&self, ended: TurnState) {
        let cancelled = {
            let mut cancelled = locked(&self.cancelled);
            let open = self.stores.turn.update(|state| {
                let open = state.is_running();
                if open {
                    *state = ended.clone();
                }
                open
            });
            cancelled.take().filter(|_| open)
        };

        if let Some(cancel) = cancelled {
            self.judge(&ended, cancel, None);
        }
        if let Some(outcome) = TurnOutcome::of(&ended) {
            self.stores.timeline.close_turn(outcome);
        }
    }
}
