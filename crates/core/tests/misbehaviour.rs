//! The raw-first rule against agents that do not behave
//! (`docs/architecture.md` §8, §7.3), which is the inspector's subject matter
//! rather than its error path.
//!
//! The agents are `/bin/sh` one-liners emitting arbitrary frames, so what each
//! test is about is the typed layer's behaviour and not an agent's: Testy is
//! conformant by construction and cannot say any of this (§12, and the hedge
//! against taking its coverage on faith).

mod common;

use acp_inspector_core::{
    AgentCommand, Direction, EntryKind, Inspector, TimelineEntry, Unrecognized,
};

use common::until;

/// The frames an entry was made of, which every entry keeps whatever the typed
/// layer made of them (§8).
fn raw(entry: &TimelineEntry) -> Vec<&str> {
    entry
        .frames
        .iter()
        .map(|recorded| recorded.frame.as_str())
        .collect()
}

fn shell(script: &str) -> AgentCommand {
    AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\n{script}"),
        ..AgentCommand::default()
    }
}

/// An agent that says its piece and then holds the connection open, so that
/// what the test is waiting for is the frame and never the exit.
fn saying(frames: &[&str]) -> AgentCommand {
    let lines = frames
        .iter()
        .map(|frame| format!("'{frame}'"))
        .collect::<Vec<_>>()
        .join(" ");
    shell(&format!("printf '%s\\n' {lines}; cat > /dev/null"))
}

async fn entries(inspector: &Inspector, count: usize) -> Vec<TimelineEntry> {
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the timeline to fill", || {
        inspector.timeline().len() >= count
    })
    .await;
    inspector.timeline().entries()
}

#[tokio::test]
async fn an_unknown_method_is_a_first_class_entry_carrying_its_raw_json() {
    // `_`-prefixed extension methods and `_meta` payloads are what real agents
    // send (§8): they display verbatim, because the typed layer decorates the
    // frames the trace already captured and never stands in for them.
    const EXTENSION: &str = r#"{"jsonrpc":"2.0","method":"_zed/telemetry","params":{"_meta":{"note":"kept as it was"}}}"#;

    let inspector = Inspector::new();
    inspector.connect(&saying(&[EXTENSION]).factory());

    let entries = entries(&inspector, 1).await;
    let EntryKind::Unrecognized(Unrecognized::NotServiced { method }) = &entries[0].kind else {
        panic!("an unknown method is an unrecognized entry: {entries:?}");
    };
    assert_eq!(method, "_zed/telemetry");
    assert_eq!(raw(&entries[0]), [EXTENSION], "verbatim, down to the byte");
}

#[tokio::test]
async fn an_unknown_session_update_variant_is_a_first_class_entry() {
    // Decoding is explicitly *as v1* (§11 seam 1), so a variant v1 has no name
    // for — an unstable-v1 one, or v2 traffic arriving early — degrades to
    // visible raw instead of breaking anything.
    const FUTURE: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"thought_signature","payload":{"_meta":{"from":"v2"}}}}}"#;

    let inspector = Inspector::new();
    inspector.connect(&saying(&[FUTURE]).factory());

    let entries = entries(&inspector, 1).await;
    let EntryKind::Unrecognized(Unrecognized::Undecodable { method, problem }) = &entries[0].kind
    else {
        panic!("an unknown update variant is an unrecognized entry: {entries:?}");
    };
    assert_eq!(method.as_deref(), Some("session/update"));
    assert_eq!(raw(&entries[0]), [FUTURE]);
    assert!(
        problem.contains("thought_signature"),
        "and it says what it could not read: {problem}"
    );
}

#[tokio::test]
async fn a_declined_client_service_is_answered_method_not_found_and_recorded() {
    // Declined by advertising no capability for either of them (§7.3): a
    // conformant agent never calls these, and a misbehaving one is exactly what
    // this tool is for, so the call is evidence in the trace rather than
    // something swallowed.
    //
    // **Two of them, and the third left this list with the fifth ring** (§7.8).
    // `elicitation/*` was declined in the same sentence as these two, on the
    // strength of a clause about what this tool had built rather than what it
    // can do; it is serviced now, and `crates/core/tests/elicitation.rs` is where that
    // is asserted. What is left here is the pair that stays declined for the
    // reason the sentence always meant: a filesystem this tool cannot mediate
    // and a terminal it does not embed.
    for (id, overreach, method) in [
        (
            41,
            r#"{"jsonrpc":"2.0","id":41,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/etc/passwd"}}"#,
            "fs/read_text_file",
        ),
        (
            42,
            r#"{"jsonrpc":"2.0","id":42,"method":"terminal/create","params":{"sessionId":"s1","command":"echo"}}"#,
            "terminal/create",
        ),
    ] {
        let inspector = Inspector::new();
        inspector.connect(&saying(&[overreach]).factory());

        let entries = entries(&inspector, 1).await;
        let EntryKind::Unrecognized(Unrecognized::NotServiced { method: called }) =
            &entries[0].kind
        else {
            panic!("the overreach is an entry of its own: {entries:?}");
        };
        assert_eq!(called, method);
        assert_eq!(raw(&entries[0]), [overreach]);

        let mut frames = inspector.trace().changes();
        until(&mut frames, "the refusal to be sent", || {
            inspector
                .trace()
                .entries()
                .iter()
                .any(|entry| entry.direction == Direction::ToAgent)
        })
        .await;

        let trace = inspector.trace().entries();
        assert_eq!(trace[0].direction, Direction::FromAgent);
        assert_eq!(trace[0].frame.as_str(), overreach, "the call is recorded");

        let answer = trace[1].frame.as_str();
        let answered: serde_json::Value = serde_json::from_str(answer).expect("valid JSON-RPC");
        assert_eq!(
            answered["id"], id,
            "answered, and answered to its face: {answer}"
        );
        assert_eq!(
            answered["error"]["code"], -32601,
            "with method-not-found: {answer}"
        );
    }
}

#[tokio::test]
async fn a_frame_that_is_not_json_rpc_at_all_is_still_shown() {
    const NONSENSE: &str = "not json, not a frame, still something the agent said";

    let inspector = Inspector::new();
    inspector.connect(&saying(&[NONSENSE]).factory());

    let entries = entries(&inspector, 1).await;
    let EntryKind::Unrecognized(Unrecognized::Undecodable { method, problem }) = &entries[0].kind
    else {
        panic!("a frame that is not JSON-RPC is an unrecognized entry: {entries:?}");
    };
    assert_eq!(method.as_deref(), None, "it announced no method");
    assert_eq!(raw(&entries[0]), [NONSENSE]);
    assert!(!problem.is_empty(), "and it says what went wrong");
}

#[tokio::test]
async fn an_answer_to_a_request_nobody_sent_is_kept() {
    const UNSOLICITED: &str = r#"{"jsonrpc":"2.0","id":99,"result":{"stopReason":"end_turn"}}"#;

    let inspector = Inspector::new();
    inspector.connect(&saying(&[UNSOLICITED]).factory());

    let entries = entries(&inspector, 1).await;
    let EntryKind::Unrecognized(Unrecognized::Unsolicited) = &entries[0].kind else {
        panic!("an answer to nothing is an unrecognized entry: {entries:?}");
    };
    assert_eq!(raw(&entries[0]), [UNSOLICITED]);
}

#[tokio::test]
async fn an_agent_that_goes_away_mid_turn_ends_the_turn_it_left_open() {
    use acp_inspector_core::{CallError, ConnectionStatus, TurnState};

    // An agent that answers `initialize` and `session/new`, takes the prompt,
    // and dies with it in hand. The turn is over either way; what the inspector
    // may not do is leave it looking like it is still running.
    //
    // It answers by counting rather than by reading ids, which works because
    // this client numbers its calls from one, in order.
    let inspector = Inspector::new();
    inspector.connect(
        &shell(concat!(
            r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'; "#,
            r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
            "read _; exit 3",
        ))
        .factory(),
    );

    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("it answered");

    let failed = inspector.prompt("anything").await;
    assert_eq!(failed, Err(CallError::Disconnected));
    assert_eq!(inspector.turn(), TurnState::Failed(CallError::Disconnected));
    // The teardown that woke that call had already said why: a caller told
    // there will be no answer reads a status accounting for it, not one still
    // claiming the agent is there. Waking anyone is the last thing a teardown
    // does (§6.1).
    assert_eq!(inspector.status(), ConnectionStatus::Lost);

    // And a turn asked for after that says so in the store as well, though it
    // never reaches a wire: the screen renders the turn, not the call, so a
    // prompt that went nowhere has to be visible as a turn that failed rather
    // than as a click with no consequence anywhere.
    assert_eq!(
        inspector.prompt("again").await,
        Err(CallError::Disconnected)
    );
    assert_eq!(inspector.turn(), TurnState::Failed(CallError::Disconnected));
}
