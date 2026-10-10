//! The driven-capability record (§7.7): which Advertisement each call on the
//! wire drives, and what the agent answered it with.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::driven::AgentCapability;
use crate::restore::Restore;
use crate::store::locked;

use super::Client;

impl Client {
    /// Records what the agent answered a driven Advertisement with (§7.7).
    ///
    /// **Written where the frame is read**, beside the other answer-dispatch
    /// calls and for their reason: the record is what a panel shows, and a panel
    /// must not depend on anybody being awake to await the call.
    ///
    /// **Answered is read off the answer rather than off a decoding of it.** An
    /// agent that answered with something v1 could not read still answered; what
    /// this record is about is whether the advertisement was taken up and what
    /// came back, and the frame is in the trace, complete (§8).
    pub(super) fn drove(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(capabilities) = locked(&self.driving).remove(&id) else {
            return;
        };

        // One answer, every Advertisement the call drove: a `session/load`
        // carrying roots is answered on the load row and on the
        // `additionalDirectories` row by the same frame (§7.7).
        self.stores.driven.update(|record| {
            let mut moved = false;
            for capability in capabilities {
                moved |= match outcome {
                    Ok(_) => record.answered(capability),
                    // The agent's own error, whole: a client that paraphrased a
                    // refusal would be standing between the reader and the wire.
                    Err(error) => record.refused(capability, CallError::Rejected(error.clone())),
                };
            }
            moved
        });
    }

    /// Says which Advertisements the call going out under `id` is driving
    /// (§7.7), where it drives any.
    ///
    /// Nothing is registered for a call that drives none — `session/new` without
    /// roots is one, and so is the listing the tool refreshes on its own
    /// initiative — because an entry the record has nothing to say about is one
    /// the teardown would sweep and one `drove` would find and do nothing with.
    pub(super) fn registers(&self, id: i64, driving: &[AgentCapability]) {
        if !driving.is_empty() {
            locked(&self.driving).insert(id, driving.to_vec());
        }
    }

    /// Says in the record that a driven Advertisement got no answer at all
    /// (§7.7).
    ///
    /// `driven` is what the call was driving, and an empty list writes nothing —
    /// because a record of what was driven has nothing to say about a call that
    /// drove none of it.
    ///
    /// `registered` is the id it went out under, or `None` for a failure that
    /// happened before it was a frame — the two judged the way
    /// [`mode_change_failed`](Self::mode_change_failed) judges the same pair. An
    /// unregistered failure is this call's alone and nothing can have replaced
    /// it; a registered one says nothing where the reader has already said what
    /// the agent answered, or where the teardown has already refused it on the
    /// way out.
    pub(super) fn drive_failed(
        &self,
        registered: Option<i64>,
        driven: &[AgentCapability],
        error: &CallError,
    ) {
        if driven.is_empty() {
            return;
        }
        if let Some(id) = registered
            && locked(&self.driving).remove(&id).is_none()
        {
            return;
        }

        // Every one of them, and never short-circuited: a call that drove two
        // Advertisements is refused on both rows or the second would be left
        // reading *not driven* about an ask that was made.
        self.stores.driven.update(|record| {
            let mut moved = false;
            for capability in driven {
                moved |= record.refused(*capability, error.clone());
            }
            moved
        });
    }

    /// Asks something that drives an Advertisement, and leaves the ask on the
    /// record (§7.7).
    ///
    /// **Prepared, registered, and only then sent** — the rule stated in five
    /// places here, and the reason [`call`](Self::call) is not enough for these:
    /// the answer is allowed to arrive before anyone is looking for it, so the
    /// id has to mean something by the time the frame is on the wire. The split
    /// allocates the id exactly where the one-shot call allocated it and changes
    /// no wire ordering, which the fixtures that answer by counting reads depend
    /// on (`crates/core/tests/capability.rs`).
    ///
    /// `driving` is the capability the call is about, or `None` for one this
    /// record does not key on — the listing the tool refreshes on its own
    /// initiative, which sends the same frame and writes nothing.
    ///
    /// **What an answer does is not here**: it is [`drove`](Self::drove)'s, at
    /// the reader, because a panel must not depend on anybody being awake to
    /// await the call. What is left for the caller is the failure that never
    /// became a frame an agent could answer.
    ///
    /// [`restore`](Self::restore) prepares and registers by hand rather than
    /// through this, and has to: it gives up the live session between the
    /// registration and the send, and the registration has to be the earlier of
    /// the two ([`restore`](Self::restore) says why).
    pub(super) async fn drive(
        &self,
        method: &str,
        params: Value,
        driving: Option<AgentCapability>,
    ) -> Result<Value, CallError> {
        let driving: Vec<_> = driving.into_iter().collect();
        let call = self
            .rpc
            .prepare(method, params)
            .inspect_err(|error| self.drive_failed(None, &driving, error))?;
        let sent = call.id();
        self.registers(sent, &driving);

        let outcome = self.rpc.send(call).await;
        if let Err(error) = &outcome {
            // A call that never became a frame the agent could answer has no
            // reader event, so the record is written by whoever awaited it —
            // under the id guard the turn and the two setters already use.
            self.drive_failed(Some(sent), &driving, error);
        }
        outcome
    }
}

/// Which Advertisement one of the two ways of reopening a session drives
/// (§7.7).
///
/// **One place, because it is one decision** — and one both this client and the
/// [`Inspector`](crate::Inspector) have to make: the caller writes the record
/// where there was no client to ask at all, which is the shape `authenticate`
/// and the two setters already have.
///
/// **One operation with a property, and two Advertisements** ([`Restore`]). An
/// agent claims `loadSession` and `sessionCapabilities.resume` separately and
/// may claim either alone, so what was driven is whichever call the property
/// selected — writing a resume's outcome on the load row would be the panel
/// reporting an answer about a call that never went out.
pub(crate) fn drives(how: Restore) -> AgentCapability {
    match how {
        Restore::Load => AgentCapability::Load,
        Restore::Resume => AgentCapability::Resume,
    }
}

/// Whether a session-opening call drives `additionalDirectories`, **read off
/// the frame that is about to cross** (§7.7).
///
/// The field is omitted when the list is empty, so a request that carries it
/// asked for roots and one that does not asked for none — which is why this asks
/// the encoded parameters rather than the list this client assembled, or the
/// intent behind it. *What crossed must not turn on what this client
/// recognized* is the rule the incomplete-option-set annotation already reads
/// its evidence by.
///
/// A list because that is what a call registers: the reopen adds it to the
/// Advertisement the call itself drives, and `session/new` drives this or
/// nothing.
pub(super) fn drives_roots(params: &Value) -> Vec<AgentCapability> {
    params
        .get(AgentCapability::AdditionalDirectories.name())
        .map(|_| AgentCapability::AdditionalDirectories)
        .into_iter()
        .collect()
}

pub(super) fn drives_setup(params: &Value) -> Vec<AgentCapability> {
    let mut driving = drives_roots(params);
    if let Some(servers) = params.get("mcpServers").and_then(Value::as_array) {
        for (transport, capability) in [
            ("http", AgentCapability::McpHttp),
            ("sse", AgentCapability::McpSse),
        ] {
            if servers
                .iter()
                .any(|server| server.get("type").and_then(Value::as_str) == Some(transport))
            {
                driving.push(capability);
            }
        }
    }
    driving
}
