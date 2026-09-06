//! The typed layer against a real agent (`docs/architecture.md` §14 step 3):
//! spawn Testy through the real stdio factory, drive the conformant path —
//! `initialize`, `session/new`, `session/prompt`, `session/cancel` — and read
//! what happened back out of the stores the desktop shell subscribes to.
//!
//! Every assertion here is on core's public surface: a store's contents, the
//! trace's frames, or the value a driven method answered with. Nothing reaches
//! into the client, and nothing injects a transport — the agent is a real
//! subprocess speaking real ACP over a real pipe (§12).

mod common;

use acp_inspector_core::{
    AgentCommand, CallError, ConnectionStatus, Direction, EntryId, EntryKind, Inspector,
    ProtocolVersion, TurnState, v1,
};

use common::{testy, until};

/// Testy's scenarios are prompt-driven: the prompt text is the command
/// (`agent-client-protocol-test`'s `TestyCommand`, serialized as JSON).
const SESSION_UPDATES: &str = r#"{"command":"run_scenario","scenario":"session_updates"}"#;
const TOOL_CALLS: &str = r#"{"command":"run_scenario","scenario":"tool_calls"}"#;
const WAIT_FOR_CANCEL: &str = r#"{"command":"run_scenario","scenario":"wait_for_cancel"}"#;

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

/// The `sessionUpdate` discriminant of an entry, which is what "every variant
/// arrived" is counted in.
///
/// **Eleven, not the spec's twelve.** The spec counts the stable v1 variants
/// from the schema metadata the surface research read
/// (docs/architecture.md §7.4); `agent-client-protocol-schema` 1.6 exposes
/// eleven outside its `unstable_*` features, and unstable ones are not decoded
/// on purpose — they are unknown traffic, which is displayed rather than
/// understood (§8). This match is what the schema crate actually offers, and
/// its `_` arm is what says so when that changes.
fn variant(kind: &EntryKind) -> Option<&'static str> {
    let EntryKind::Update(update) = kind else {
        return None;
    };
    Some(match update {
        v1::SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        v1::SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        v1::SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        v1::SessionUpdate::ToolCall(_) => "tool_call",
        v1::SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        v1::SessionUpdate::Plan(_) => "plan",
        v1::SessionUpdate::AvailableCommandsUpdate(_) => "available_commands_update",
        v1::SessionUpdate::CurrentModeUpdate(_) => "current_mode_update",
        v1::SessionUpdate::ConfigOptionUpdate(_) => "config_option_update",
        v1::SessionUpdate::SessionInfoUpdate(_) => "session_info_update",
        v1::SessionUpdate::UsageUpdate(_) => "usage_update",
        // The enum is `#[non_exhaustive]`: a variant the schema crate grows
        // later is one this test has not been taught to expect, which is a
        // failure of the list below and not of the timeline.
        _ => "unnamed",
    })
}

#[tokio::test]
async fn starting_an_agent_opens_the_session_a_turn_needs() {
    // What the Launch button is (§9): one thing the user asked for and three
    // the protocol needs — spawn, `initialize`, `session/new`. The sequence is
    // core's because it is protocol choreography, and the window that renders
    // the timeline should have nothing to know about it beyond that it happened.
    let inspector = Inspector::new();

    let session = inspector.start(&agent()).await.expect("Testy starts");

    assert_eq!(inspector.status(), ConnectionStatus::Connected);
    assert!(
        inspector.agent().is_some(),
        "the agent's own account of itself is in the store the capability display reads"
    );
    assert_eq!(inspector.session(), Some(session));
    // A session with nothing prompted into it yet: the composer's condition.
    assert_eq!(inspector.turn(), TurnState::Idle);

    let stop = inspector
        .prompt(r#"{"command":"echo","message":"hello"}"#)
        .await
        .expect("a turn runs on the session it opened");
    assert_eq!(stop, v1::StopReason::EndTurn);
}

#[tokio::test]
async fn starting_an_agent_that_never_runs_answers_instead_of_hanging() {
    // The failure the whole design is against (§6.1): a spawn that failed must
    // reach the caller as news, not as a handshake that waits forever on an
    // agent that was never there.
    let inspector = Inspector::new();
    let mut statuses = inspector.status_changes();

    let error = inspector
        .start(&AgentCommand {
            command: "/nonexistent/agent".into(),
            ..AgentCommand::default()
        })
        .await
        .expect_err("there is no such program");

    assert_eq!(error, CallError::Disconnected);
    assert!(
        inspector.session().is_none(),
        "and nothing claims a session on an agent that never ran"
    );

    // The evidence comes up the diagnostic channel rather than out of the call
    // (§6.1), so the status arrives on its own moments later — which is the
    // shell's own order of events: it subscribes, and the console opens itself
    // when the status turns.
    until(&mut statuses, "the failed spawn to be reported", || {
        inspector.status() == ConnectionStatus::FailedToStart
    })
    .await;
}

#[tokio::test]
async fn the_conformant_path_fills_the_stores() {
    let inspector = Inspector::new();
    let mut turns = inspector.turn_changes();

    inspector.connect(&agent().factory());

    // `initialize` is the first display artifact (§7.1): what the agent claims
    // about itself, before anything is asked of it.
    let initialized = inspector.initialize().await.expect("Testy initializes");
    assert_eq!(initialized.protocol_version, ProtocolVersion::V1);

    let described = inspector.agent().expect("the store holds what it claimed");
    assert_eq!(described, initialized);
    // Testy sends no `agentInfo`, and the store says so rather than filling it
    // in: a capability display invents nothing about the agent it is describing.
    assert_eq!(described.agent_info, None);
    assert!(
        described.agent_capabilities.load_session,
        "Testy's advertised capabilities are the store's: {:?}",
        described.agent_capabilities
    );
    assert!(
        !described.auth_methods.is_empty(),
        "Testy advertises an auth method"
    );

    let session = inspector
        .new_session("/tmp", None)
        .await
        .expect("a new session");
    assert_eq!(inspector.session(), Some(session.clone()));
    assert_eq!(inspector.turn(), TurnState::Idle);

    let stop = inspector
        .prompt(SESSION_UPDATES)
        .await
        .expect("the turn ran");
    assert_eq!(stop, v1::StopReason::EndTurn);

    // Turn state is a field the stream drives, not a bracket around the call
    // (§11 seam 2): by the time the prompt answers, the state and the stop
    // reason are already what the stream made of them.
    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));

    // Every stable v1 variant Testy emits, decoded. `tool_call_update` is the
    // one that is not an entry of its own and never should be: it is a patch to
    // the tool call it names, and the assertion that it arrived and was
    // understood is that the calls below finished.
    let entries = inspector.timeline().entries();
    let seen: Vec<_> = entries
        .iter()
        .filter_map(|entry| variant(&entry.kind))
        .collect();
    for expected in [
        "user_message_chunk",
        "agent_message_chunk",
        "agent_thought_chunk",
        "tool_call",
        "plan",
        "available_commands_update",
        "current_mode_update",
        "config_option_update",
        "session_info_update",
        "usage_update",
    ] {
        assert!(
            seen.contains(&expected),
            "the `session_updates` scenario's {expected} reached the timeline: {seen:?}"
        );
    }
    assert!(
        !seen.contains(&"unnamed"),
        "every entry decoded as a v1 variant this test knows: {seen:?}"
    );
    assert!(
        entries.iter().all(|entry| match &entry.kind {
            EntryKind::Update(v1::SessionUpdate::ToolCall(call)) =>
                call.status == v1::ToolCallStatus::Completed,
            _ => true,
        }),
        "the tool-call updates were decoded, onto the calls they update: {entries:?}"
    );

    // The typed layer decorates the trace, it does not replace it (§8): every
    // notification that crossed is on the entry it was decoded into, verbatim,
    // and the entries hold no frame the wire did not carry.
    let streamed: Vec<_> = inspector
        .trace()
        .entries()
        .into_iter()
        .filter(|entry| {
            entry.direction == Direction::FromAgent
                && entry
                    .frame
                    .as_str()
                    .contains(r#""method":"session/update""#)
        })
        .map(|entry| entry.frame)
        .collect();
    let decorated: Vec<_> = entries
        .iter()
        .flat_map(|entry| entry.frames.iter().map(|recorded| recorded.frame.clone()))
        .collect();
    assert_eq!(
        decorated, streamed,
        "the timeline is the trace, decoded — same frames, same order"
    );

    // Watched, not polled: the shell learns the turn ended the same way.
    until(&mut turns, "the turn to have ended", || {
        matches!(inspector.turn(), TurnState::Ended(_))
    })
    .await;
}

#[tokio::test]
async fn tool_call_updates_merge_into_one_entry() {
    // The v2 seam that has to be built now (§11 seam 3): entities are keyed by
    // the agent's id, so a tool call that streams three updates is one card and
    // not four.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");
    let session = inspector
        .new_session("/tmp", None)
        .await
        .expect("a new session");

    inspector.prompt(TOOL_CALLS).await.expect("the turn ran");

    let calls: Vec<_> = inspector
        .timeline()
        .entries()
        .into_iter()
        .filter_map(|entry| match (&entry.id, &entry.kind) {
            (EntryId::ToolCall(_, id), EntryKind::Update(v1::SessionUpdate::ToolCall(call))) => {
                Some((id.clone(), call.clone(), entry.frames.clone()))
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        calls.len(),
        2,
        "Testy's `tool_calls` scenario runs two tool calls: {calls:?}"
    );
    for (id, call, frames) in &calls {
        assert_eq!(id, &call.tool_call_id);
        assert_eq!(
            call.status,
            v1::ToolCallStatus::Completed,
            "the updates merged into the call rather than duplicating it: {call:?}"
        );
        assert_eq!(
            inspector
                .timeline()
                .entries()
                .iter()
                .filter(|entry| entry.id == EntryId::ToolCall(session.clone(), id.clone()))
                .count(),
            1,
            "one entry per tool call id"
        );

        // Merged, not replaced: the entry keeps every frame it was made of, so
        // the card the shell renders can still be opened onto the wire (§8).
        assert!(
            frames.len() > 1,
            "the entry kept the updates it merged: {frames:?}"
        );
        assert!(
            frames[0]
                .frame
                .as_str()
                .contains(r#""sessionUpdate":"tool_call""#),
            "starting with the create: {frames:?}"
        );
        assert!(
            frames[1..].iter().all(|recorded| recorded
                .frame
                .as_str()
                .contains(r#""sessionUpdate":"tool_call_update""#)),
            "and the updates after it: {frames:?}"
        );
    }
    assert!(
        calls.iter().any(|(_, call, _)| call.raw_output.is_some()),
        "and the merge carried the fields the updates brought: {calls:?}"
    );
}

#[tokio::test]
async fn cancelling_resolves_the_turn_as_cancelled() {
    // The conformance point worth watching (§7.1): a cancelled prompt must
    // still resolve, and it must resolve as `cancelled`.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a new session");

    let mut frames = inspector.trace().changes();
    let prompting = tokio::spawn({
        let inspector = inspector.clone();
        async move { inspector.prompt(WAIT_FOR_CANCEL).await }
    });

    // Cancel once the prompt is on the wire, not merely in flight in a task:
    // what the trace has recorded, the transport has been handed.
    until(&mut frames, "the prompt to be sent", || {
        inspector.trace().entries().iter().any(|entry| {
            entry.direction == Direction::ToAgent
                && entry
                    .frame
                    .as_str()
                    .contains(r#""method":"session/prompt""#)
        })
    })
    .await;
    assert_eq!(inspector.turn(), TurnState::InFlight);

    inspector.cancel().await.expect("Testy is alive");
    assert_eq!(inspector.turn(), TurnState::Cancelling);

    let stop = prompting
        .await
        .expect("the prompt task")
        .expect("it resolved");
    assert_eq!(stop, v1::StopReason::Cancelled);
    assert_eq!(
        inspector.turn(),
        TurnState::Ended(v1::StopReason::Cancelled)
    );

    assert!(
        inspector.trace().entries().iter().any(|entry| {
            entry.direction == Direction::ToAgent
                && entry
                    .frame
                    .as_str()
                    .contains(r#""method":"session/cancel""#)
        }),
        "and the cancel itself is in the trace like everything else"
    );
}

#[tokio::test]
async fn a_second_turn_starts_from_the_state_the_first_one_left() {
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a new session");

    inspector
        .prompt(r#"{"command":"echo","message":"first"}"#)
        .await
        .expect("the first turn ran");
    let after_first = inspector.timeline().len();

    inspector
        .prompt(r#"{"command":"echo","message":"second"}"#)
        .await
        .expect("the second turn ran");

    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));
    assert!(
        inspector.timeline().len() > after_first,
        "the second turn appended to the timeline the first one filled"
    );
}
