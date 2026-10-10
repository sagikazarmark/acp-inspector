//! The driven-capability record: which advertisement each call in flight
//! drives, and what the agent answered it with. See `docs/architecture.md`
//! §7.7.

use agent_client_protocol_schema::v1;
use serde_json::Value;

use crate::call::CallError;
use crate::driven::AgentCapability;
use crate::restore::Restore;
use crate::store::locked;

use super::Client;

impl Client {
    /// Records the agent's answer against every advertisement the call drove.
    ///
    /// Called by the reader, so the record does not depend on anyone awaiting
    /// the call. Any answer counts as answered, even one v1 cannot decode.
    pub(super) fn drove(&self, id: i64, outcome: &Result<Value, v1::Error>) {
        let Some(capabilities) = locked(&self.driving).remove(&id) else {
            return;
        };

        self.stores.driven.update(|record| {
            let mut moved = false;
            for capability in capabilities {
                moved |= match outcome {
                    Ok(_) => record.answered(capability),
                    // The agent's own error, unparaphrased.
                    Err(error) => record.refused(capability, CallError::Rejected(error.clone())),
                };
            }
            moved
        });
    }

    /// Registers the advertisements the call going out under `id` drives. A
    /// call that drives none registers nothing.
    pub(super) fn registers(&self, id: i64, driving: &[AgentCapability]) {
        if !driving.is_empty() {
            locked(&self.driving).insert(id, driving.to_vec());
        }
    }

    /// Records that a driven call got no answer at all.
    ///
    /// `registered` is the call's id, or `None` for a failure before it was
    /// sent. A registered failure records nothing if its entry is gone: the
    /// reader recorded the answer, or the teardown already refused it.
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

        // Every row, never short-circuited.
        self.stores.driven.update(|record| {
            let mut moved = false;
            for capability in driven {
                moved |= record.refused(*capability, error.clone());
            }
            moved
        });
    }

    /// Sends a call that drives `driving` (or nothing, for `None`) and records
    /// a failure to send it.
    ///
    /// Prepared, registered, then sent, so the id is registered before the
    /// answer can arrive. Ids are allocated in the same order as a plain call,
    /// which fixtures that answer by counting reads rely on
    /// (`crates/core/tests/capability.rs`). The answer itself is recorded by
    /// [`drove`](Self::drove).
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
            // No frame the agent could answer, so no reader event: record the
            // failure here, under the id guard.
            self.drive_failed(Some(sent), &driving, error);
        }
        outcome
    }
}

/// The advertisement a reopen drives: `loadSession` or
/// `sessionCapabilities.resume`, whichever call `how` selects. Also used by the
/// inspector when there is no client to ask.
pub(crate) fn drives(how: Restore) -> AgentCapability {
    match how {
        Restore::Load => AgentCapability::Load,
        Restore::Resume => AgentCapability::Resume,
    }
}

/// `additionalDirectories`, if the encoded request carries it. Read off the
/// frame: the field is omitted when the list is empty.
pub(super) fn drives_roots(params: &Value) -> Vec<AgentCapability> {
    params
        .get(AgentCapability::AdditionalDirectories.name())
        .map(|_| AgentCapability::AdditionalDirectories)
        .into_iter()
        .collect()
}

/// What a session setup request drives by what it carries: roots, and MCP
/// servers over HTTP or SSE.
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
