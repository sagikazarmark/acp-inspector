//! A prompt turn: sending it, cancelling it, and settling it from the frame
//! that ends it. The turn state is driven by the stream, not by the call
//! (`docs/architecture.md` §11 seam 2).

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
    /// `session/prompt` with ordered content blocks, each media kind checked
    /// against what the agent advertised.
    ///
    /// The turn is registered before the frame goes out, so the reader
    /// recognizes the answer whenever it arrives. Any failure is written to the
    /// turn store, but only while this turn is still the awaited one: a prompt
    /// abandoned by a session switch must not mark the next turn failed.
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
        // A cancel still armed belongs to a turn prompted over. That turn draws
        // no annotation: its answer may yet cross, as an unsolicited entry.
        *locked(&self.cancelled) = None;
        self.stores.turn.set(TurnState::InFlight);
        // Opened here so traffic arriving before this call returns is stamped
        // with the turn.
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

    /// Records a failed turn, if it is still the one the screen is about.
    ///
    /// `registered` is the turn's id, or `None` for a failure before it was
    /// sent, which is always this call's. A registered failure is dropped once
    /// `awaiting` no longer holds its id: the reader already settled the turn
    /// from its answer, or the session was left.
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

    /// `session/cancel`, then `cancelled` for every request the turn blocked
    /// on (§7.2).
    ///
    /// The turn ends when the agent resolves the prompt; until then it is
    /// [`TurnState::Cancelling`]. The blocked requests are answered after the
    /// notification, so the agent knows why.
    pub(crate) async fn cancel(&self) -> Result<(), CallError> {
        let session = self.session()?;
        let sent = self
            .rpc
            .notify(
                v1::AGENT_METHOD_NAMES.session_cancel,
                encode(&v1::CancelNotification::new(session)),
            )
            .await?;

        // A request-scoped elicitation belongs to its call, not the turn.
        self.answer_cancelled(Debt::TheTurns).await;

        // Only a turn still in flight moves to cancelling and arms the rule.
        // Both locks are held together, in the order `settle` takes them, so a
        // cancel racing the turn's answer resolves one way or the other.
        let mut cancelled = locked(&self.cancelled);
        let open = self.stores.turn.update(|state| {
            let open = *state == TurnState::InFlight;
            if open {
                *state = TurnState::Cancelling;
            }
            open
        });
        if open {
            // A second cancel keeps the first one's frame: that is the one the
            // agent was answering.
            *cancelled = Some(sent);
        }
        Ok(())
    }

    /// Settles the turn from the answer that ended it, if this is that answer.
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

        // Under one hold, in `cancel`'s lock order: the armed cancel is taken
        // exactly once, so it judges this turn or none.
        let cancelled = {
            let mut cancelled = locked(&self.cancelled);
            let armed = cancelled.take();
            self.stores.turn.set(resolved.clone());
            armed
        };
        if let Some(cancel) = cancelled {
            self.judge(&resolved, cancel, Some(frame.clone()));
        }
        // After the annotation, so it falls inside the turn's span.
        if let Some(outcome) = TurnOutcome::of(&resolved) {
            self.stores.timeline.close_turn(outcome);
        }

        // A blocking request belongs to the turn it blocked (§7.2), so the ones
        // left unanswered are given up on. Requests scoped to a JSON-RPC call
        // rather than a session survive until the connection ends (§7.8).
        self.stores.permissions.abandon();
        self.stores
            .elicitations
            .abandon_unless(|request| request.session_id().is_none());
        self.stores
            .services
            .abandon_unless(|request| request.session_id().is_none());
    }

    /// Ends an open turn with `ended`; a turn the agent already ended keeps its
    /// own reason.
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
