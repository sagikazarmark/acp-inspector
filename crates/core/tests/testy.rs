//! The whole frame layer against a real agent: spawn Testy through the real
//! stdio factory, exchange real ACP frames, and read the trace and the
//! diagnostic log back through the public API only.
//!
//! The frames here are hand-written JSON-RPC rather than the typed layer's,
//! which is the point: the trace records the wire, not a decoding of it
//! (docs/architecture.md §6.2, §8), and it does that for whoever is above it —
//! including nobody at all. `crates/core/tests/typed.rs` drives the same agent through
//! the layer that does decode.

mod common;

use std::time::SystemTime;

use acp_inspector_core::{ConnectionFactory, DiagnosticKind, Direction, Frame, StdioSpawn, Trace};
use agent_client_protocol_schema::v1;

use common::{frames_through, next_diagnostic, testy};

#[tokio::test]
async fn drives_testy_through_a_turn_and_records_every_frame() {
    let started = SystemTime::now();
    let (connection, trace) = Trace::decorate(StdioSpawn::new(testy()).cwd("/tmp").connect());
    let (outgoing, mut incoming, mut diagnostics) = connection.into_parts();

    let initialize = Frame::new(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}"#,
    );
    outgoing
        .send(initialize.clone())
        .await
        .expect("Testy is alive");
    let initialized = frames_through(&mut incoming, r#""id":1"#)
        .await
        .pop()
        .unwrap();

    // The frame layer decodes nothing, so this is the test decoding rather than
    // the code: what the transport carried is the ACP message Testy sent, intact
    // enough for the layer above to make sense of it.
    let initialize_result: v1::InitializeResponse =
        serde_json::from_value(result_of(&initialized)).expect("an ACP v1 initialize result");
    assert!(
        !initialize_result.auth_methods.is_empty(),
        "Testy advertises an auth method: {initialized}"
    );
    assert!(
        trace
            .entries()
            .iter()
            .any(|entry| entry.frame == initialized),
        "a frame is in the trace by the time anything above the seam has it"
    );

    outgoing
        .send(Frame::new(
            r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/tmp","mcpServers":[]}}"#,
        ))
        .await
        .expect("Testy is alive");
    let session = frames_through(&mut incoming, r#""id":2"#)
        .await
        .pop()
        .unwrap();
    let session_id = session_id_of(&session);

    outgoing
        .send(Frame::new(format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{{"sessionId":"{session_id}","prompt":[{{"type":"text","text":"{{\"command\":\"echo\",\"message\":\"frame layer\"}}"}}]}}}}"#,
        )))
        .await
        .expect("Testy is alive");
    let turn = frames_through(&mut incoming, r#""id":3"#).await;

    assert!(
        turn.iter()
            .any(|frame| frame.as_str().contains(r#""method":"session/update""#)),
        "the turn streamed its update notifications: {turn:?}"
    );
    assert!(
        turn.last()
            .unwrap()
            .as_str()
            .contains(r#""stopReason":"end_turn""#),
        "the turn ended: {:?}",
        turn.last()
    );

    // Closing the client's end of stdin ends the agent, which is how this test
    // gets an exit to assert on rather than a kill on drop.
    drop(outgoing);
    let exit = next_diagnostic(&mut diagnostics).await;
    assert!(
        matches!(&exit.kind, DiagnosticKind::AgentExited(status) if status.success()),
        "Testy ended cleanly on stdin EOF: {exit:?}"
    );

    let entries = trace.entries();

    let sent: Vec<_> = entries
        .iter()
        .filter(|e| e.direction == Direction::ToAgent)
        .collect();
    assert_eq!(sent.len(), 3, "three requests were sent");
    assert_eq!(
        sent[0].frame, initialize,
        "the trace holds the frame verbatim"
    );

    let received: Vec<_> = entries
        .iter()
        .filter(|e| e.direction == Direction::FromAgent)
        .collect();
    assert_eq!(
        received.len(),
        1 + 1 + turn.len(),
        "every frame the agent wrote is in the trace, notifications included"
    );

    assert!(
        entries.windows(2).all(|pair| pair[0].at <= pair[1].at),
        "the trace is in wire order, timestamped"
    );
    assert!(entries.first().unwrap().at >= started);
}

#[tokio::test]
async fn a_frame_testy_never_answers_is_still_in_the_trace() {
    // Unknown traffic is the inspector's subject matter (docs/architecture.md
    // §8). Nothing below the typed layer judges a frame — an extension method
    // no agent implements is recorded on the way out like any other.
    let (connection, trace) = Trace::decorate(StdioSpawn::new(testy()).connect());
    let (outgoing, mut incoming, _diagnostics) = connection.into_parts();

    let extension = Frame::new(
        r#"{"jsonrpc":"2.0","id":7,"method":"_inspector/unknown","params":{"_meta":{}}}"#,
    );
    outgoing
        .send(extension.clone())
        .await
        .expect("Testy is alive");

    let answer = frames_through(&mut incoming, r#""id":7"#)
        .await
        .pop()
        .unwrap();
    assert!(
        answer.as_str().contains("error"),
        "an agent may refuse it: {answer}"
    );

    let entries = trace.entries();
    assert_eq!(entries[0].direction, Direction::ToAgent);
    assert_eq!(entries[0].frame, extension);
    assert!(entries.iter().any(|entry| entry.frame == answer));
}

fn result_of(frame: &Frame) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(frame.as_str()).expect("valid JSON-RPC");
    value["result"].clone()
}

fn session_id_of(frame: &Frame) -> String {
    result_of(frame)["sessionId"]
        .as_str()
        .unwrap_or_else(|| panic!("a session/new result carries a sessionId: {frame}"))
        .to_owned()
}
