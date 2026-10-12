//! What crosses the wire is what the record shows, including what agents get
//! wrong: bad bytes, CRLF, oversized answers, other sessions' updates, ids of
//! the wrong type, and requests withdrawn or piled up.

mod common;

use std::time::Duration;

use acp_inspector_core::{
    CallError, ConnectionStatus, DiagnosticKind, EntryKind, FrameKind, Inspector, PermissionState,
    StdioSpawn, Stop, TurnState, Unrecognized, v1,
};
use serde_json::Value;

use common::{PATIENCE, QUIET, received, until};

/// A scripted agent: it answers `initialize` and `session/new` (session `s1`,
/// two modes), then does what `mode` says with the next request.
const PROBE: &str = r#"
import json, sys, time
mode = sys.argv[1]
out = sys.stdout.buffer
def send(obj, end=b"\n"):
    out.write(json.dumps(obj).encode() + end); out.flush()
def read():
    line = sys.stdin.buffer.readline()
    return json.loads(line) if line else None
req = read()
if mode == "stdout_bad":
    out.write(b'{"jsonrpc":"2.0","method":"_x/note","params":{"s":"\xff"}}\n'); out.flush()
send({"jsonrpc":"2.0","id":req["id"],"result":{"protocolVersion":1,"agentCapabilities":{}}})
req = read()
modes = {"currentModeId":"a","availableModes":[{"id":"a","name":"A"},{"id":"b","name":"B"}]}
send({"jsonrpc":"2.0","id":req["id"],"result":{"sessionId":"s1","modes":modes}})
req = read()
if mode == "foreign":
    send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"OTHER","update":{"sessionUpdate":"current_mode_update","currentModeId":"b"}}})
    send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"OTHER","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"from another session"}}}})
    send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"from the live session"}}}})
if mode == "big":
    send({"jsonrpc":"2.0","id":req["id"],"result":{"stopReason":"end_turn","_meta":{"pad":"x"*(11*1024*1024)}}})
elif mode == "crlf":
    send({"jsonrpc":"2.0","id":req["id"],"result":{"stopReason":"end_turn"}}, b"\r\n")
elif mode == "mistyped":
    send({"jsonrpc":"2.0","id":str(req["id"]),"result":{"stopReason":"end_turn"}})
    send({"jsonrpc":"2.0","id":None,"error":{"code":-32700,"message":"Parse error"}})
elif mode == "stall":
    cancel = read()
    sys.stderr.write(json.dumps(cancel) + "\n"); sys.stderr.flush()
    send({"jsonrpc":"2.0","id":req["id"],"error":{"code":-32800,"message":"cancelled"}})
elif mode == "withdraw":
    ask = {"sessionId":"s1","toolCall":{"toolCallId":"t1"},"options":[{"optionId":"ok","name":"OK","kind":"allow_once"}]}
    send({"jsonrpc":"2.0","id":"perm-1","method":"session/request_permission","params":ask})
    time.sleep(0.3)
    send({"jsonrpc":"2.0","method":"$/cancel_request","params":{"requestId":"perm-1"}})
    answer = read()
    sys.stderr.write(json.dumps(answer) + "\n"); sys.stderr.flush()
    send({"jsonrpc":"2.0","id":req["id"],"result":{"stopReason":"end_turn"}})
elif mode == "flood":
    ask = {"sessionId":"s1","toolCall":{"toolCallId":"t1"},"options":[{"optionId":"ok","name":"OK","kind":"allow_once"}]}
    for i in range(101):
        send({"jsonrpc":"2.0","id":"perm-%d" % i,"method":"session/request_permission","params":ask})
    refusal = read()
    sys.stderr.write(json.dumps(refusal) + "\n"); sys.stderr.flush()
else:
    send({"jsonrpc":"2.0","id":req["id"],"result":{"stopReason":"end_turn"}})
sys.stdin.read()
"#;

fn probe(mode: &str) -> StdioSpawn {
    StdioSpawn::new("python3").args(["-c", PROBE, mode])
}

async fn opened(mode: &str) -> Inspector {
    let inspector = Inspector::new();
    inspector.connect(&probe(mode));
    tokio::time::timeout(PATIENCE, async {
        inspector.initialize().await.unwrap();
        inspector.new_session("/tmp", None).await.unwrap();
    })
    .await
    .expect("the handshake completes");
    inspector
}

fn records(export: &str) -> Vec<Value> {
    export
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn an_answer_ending_in_crlf_is_recorded_with_its_cr_and_still_answers() {
    let inspector = opened("crlf").await;
    let stop = tokio::time::timeout(PATIENCE, inspector.prompt("hi"))
        .await
        .unwrap();
    assert_eq!(stop, Ok(v1::StopReason::EndTurn));
    let answer = received(&inspector).pop().unwrap();
    assert!(answer.ends_with("}\r"), "{answer:?}");
    inspector.disconnect();
}

#[tokio::test]
async fn a_frame_that_is_not_utf8_exports_as_base64_of_its_bytes() {
    let inspector = opened("stdout_bad").await;
    let records = records(&inspector.trace().export().to_string());
    let lossy = &records[2];
    assert_eq!(lossy["encoding"], "base64");
    // {"jsonrpc":"2.0","method":"_x/note","params":{"s":"\xff"}}
    assert_eq!(
        lossy["frame"],
        "eyJqc29ucnBjIjoiMi4wIiwibWV0aG9kIjoiX3gvbm90ZSIsInBhcmFtcyI6eyJzIjoi/yJ9fQ=="
    );
    assert_eq!(records[3]["encoding"], "utf8");
    inspector.disconnect();
}

#[tokio::test]
async fn an_answer_too_long_to_carry_fails_its_call_and_leaves_a_placeholder() {
    let inspector = opened("big").await;
    let stop = tokio::time::timeout(PATIENCE, inspector.prompt("hi"))
        .await
        .expect("the call is failed rather than left waiting");
    let Err(CallError::AnswerDropped { length }) = stop else {
        panic!("expected the dropped answer, got {stop:?}");
    };
    assert!(length > 11 * 1024 * 1024);
    assert!(matches!(
        inspector.turn(),
        TurnState::Failed(CallError::AnswerDropped { .. })
    ));
    assert!(inspector.pending_calls().is_empty());

    let traced = inspector.trace().entries();
    let placeholder = &traced.last().unwrap().frame;
    assert_eq!(placeholder.omission().unwrap().length, length);
    assert!(
        placeholder
            .as_str()
            .starts_with(r#"{"jsonrpc": "2.0", "id": 3"#)
    );

    let export = inspector.trace().export();
    assert_eq!(export.dropped(), 1);
    let records = records(&export.to_string());
    assert_eq!(records[0]["dropped"], 1);
    assert_eq!(records[0]["frames"], records.len() - 2);
    let last = records.last().unwrap();
    assert_eq!(last["type"], "dropped");
    assert_eq!(last["length"], length);

    assert!(inspector.timeline().entries().iter().any(|entry| matches!(
        entry.kind,
        EntryKind::Unrecognized(Unrecognized::Dropped { .. })
    )));
    inspector.disconnect();
}

#[tokio::test]
async fn another_sessions_update_changes_nothing_and_is_marked() {
    let inspector = opened("foreign").await;
    let mode = inspector.session_settings();
    inspector.prompt("hi").await.unwrap();
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the live session's line", || {
        inspector.timeline().len() >= 3
    })
    .await;
    assert_eq!(inspector.session_settings(), mode, "settings left alone");
    let entries = inspector.timeline().entries();
    let foreign: Vec<_> = entries
        .iter()
        .filter(|entry| {
            matches!(&entry.kind,
                EntryKind::Unrecognized(Unrecognized::ForeignSession { session }) if session.0.as_ref() == "OTHER")
        })
        .collect();
    assert_eq!(foreign.len(), 2);
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Update(_)))
    );
    inspector.disconnect();
}

#[tokio::test]
async fn requests_and_answers_are_paired_with_their_elapsed_time() {
    let inspector = opened("plain").await;
    let (traced, dropped) = inspector.trace().snapshot();
    let ordinal = |position: usize| (dropped + position) as u64;
    let asked = inspector.trace().counterpart(ordinal(0)).expect("paired");
    assert_eq!(asked.ordinal, ordinal(1));
    assert!(!asked.mistyped);
    let answered = inspector.trace().counterpart(ordinal(1)).unwrap();
    assert_eq!(answered.ordinal, ordinal(0));
    assert_eq!(answered.elapsed, asked.elapsed);
    assert_eq!(traced[1].frame.summary().kind, FrameKind::Result);
    inspector.disconnect();
}

#[tokio::test]
async fn an_answer_with_the_wrong_id_type_is_named_and_leaves_the_call_waiting() {
    let inspector = opened("mistyped").await;
    let prompt = inspector.submit_prompt(vec!["hi".into()]).unwrap();
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "both odd answers", || {
        inspector.timeline().len() >= 2
    })
    .await;
    let entries = inspector.timeline().entries();
    assert!(matches!(
        &entries[0].kind,
        EntryKind::Unrecognized(Unrecognized::MistypedId { id, call: 3 }) if id == "\"3\""
    ));
    assert!(matches!(
        &entries[1].kind,
        EntryKind::Unrecognized(Unrecognized::Unaddressed { error }) if i32::from(error.code) == -32700
    ));
    let pending = inspector.pending_calls();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].method, "session/prompt");
    let captured = pending[0].captured.expect("the request is in the trace");
    assert!(inspector.trace().counterpart(captured).is_none());
    let mistyped = inspector.trace().counterpart(captured + 1).unwrap();
    assert!(mistyped.mistyped);
    assert_eq!(mistyped.ordinal, captured);
    assert!(!prompt.is_finished());
    inspector.disconnect();
}

#[tokio::test]
async fn a_pending_call_can_be_cancelled_with_cancel_request() {
    let inspector = opened("stall").await;
    let prompt = inspector.submit_prompt(vec!["hi".into()]).unwrap();
    let mut frames = inspector.trace().changes();
    until(&mut frames, "the prompt to be pending", || {
        inspector
            .pending_calls()
            .iter()
            .any(|call| call.captured.is_some())
    })
    .await;
    let id = inspector.pending_calls()[0].id;
    inspector.cancel_call(id).await.unwrap();
    let stop = tokio::time::timeout(PATIENCE, prompt)
        .await
        .unwrap()
        .unwrap();
    let Err(CallError::Rejected(error)) = stop else {
        panic!("expected the agent's cancellation, got {stop:?}");
    };
    assert_eq!(i32::from(error.code), -32800);
    let mut lines = inspector.diagnostics().changes();
    until(&mut lines, "the agent to echo the cancel", || {
        !inspector.diagnostics().is_empty()
    })
    .await;
    let DiagnosticKind::Stderr(echo) = &inspector.diagnostics().entries()[0].kind else {
        panic!("expected the echo");
    };
    let echo: Value = serde_json::from_str(echo).unwrap();
    assert_eq!(echo["method"], "$/cancel_request");
    assert_eq!(echo["params"]["requestId"], id);
    assert_eq!(inspector.cancel_call(id).await, Err(CallError::NotWaiting));
    inspector.disconnect();
}

#[tokio::test]
async fn a_request_the_agent_withdraws_is_answered_cancelled_and_marked() {
    let inspector = opened("withdraw").await;
    let stop = tokio::time::timeout(PATIENCE, inspector.prompt("hi"))
        .await
        .unwrap();
    assert_eq!(stop, Ok(v1::StopReason::EndTurn));
    let entry = inspector
        .timeline()
        .entries()
        .into_iter()
        .find(|entry| matches!(entry.kind, EntryKind::Permission(_)))
        .unwrap();
    let EntryKind::Permission(request) = &entry.kind else {
        unreachable!()
    };
    assert_eq!(request.state(), PermissionState::CancelledByAgent);
    assert_eq!(entry.frames.len(), 2, "the withdrawal is part of the entry");
    let mut lines = inspector.diagnostics().changes();
    until(&mut lines, "the agent to echo its answer", || {
        !inspector.diagnostics().is_empty()
    })
    .await;
    let DiagnosticKind::Stderr(echo) = &inspector.diagnostics().entries()[0].kind else {
        panic!("expected the echo");
    };
    let echo: Value = serde_json::from_str(echo).unwrap();
    assert_eq!(echo["id"], "perm-1");
    assert_eq!(echo["error"]["code"], -32800);
    inspector.disconnect();
}

#[tokio::test]
async fn blocking_requests_past_the_cap_are_refused_and_recorded() {
    let inspector = opened("flood").await;
    let _prompt = inspector.submit_prompt(vec!["hi".into()]).unwrap();
    let mut lines = inspector.diagnostics().changes();
    until(&mut lines, "the refusal to reach the agent", || {
        !inspector.diagnostics().is_empty()
    })
    .await;
    assert_eq!(inspector.pending_permissions().len(), 100);
    let DiagnosticKind::Stderr(echo) = &inspector.diagnostics().entries()[0].kind else {
        panic!("expected the echo");
    };
    let echo: Value = serde_json::from_str(echo).unwrap();
    assert_eq!(echo["id"], "perm-100");
    assert!(echo["error"].is_object());
    assert!(inspector.timeline().entries().iter().any(|entry| matches!(
        &entry.kind,
        EntryKind::Unrecognized(Unrecognized::OverCapacity { limit: 100, .. })
    )));
    inspector.disconnect();
}

/// How the agent stopped, once the graceful disconnect has said so.
async fn stopped_by(inspector: &Inspector) -> Stop {
    let mut lines = inspector.diagnostics().changes();
    until(&mut lines, "the stop to be recorded", || {
        inspector
            .diagnostics()
            .entries()
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::Stopped { .. }))
    })
    .await;
    inspector
        .diagnostics()
        .entries()
        .into_iter()
        .find_map(|line| match line.kind {
            DiagnosticKind::Stopped { by, .. } => Some(by),
            _ => None,
        })
        .unwrap()
}

async fn running(script: &str) -> Inspector {
    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&StdioSpawn::new("python3").args(["-c", script]));
    until(&mut frames, "the agent to start", || {
        !received(&inspector).is_empty()
    })
    .await;
    inspector
}

#[tokio::test]
async fn a_graceful_disconnect_closes_stdin_first() {
    let inspector = running(
        r#"
import sys
print('{"jsonrpc":"2.0","method":"ready"}', flush=True)
sys.stdin.read()
"#,
    )
    .await;
    inspector.disconnect_gracefully();
    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
    assert_eq!(stopped_by(&inspector).await, Stop::StdinClosed);
}

#[cfg(unix)]
#[tokio::test]
async fn a_graceful_disconnect_terminates_an_agent_that_ignores_stdin() {
    let inspector = running(
        r#"
import time
print('{"jsonrpc":"2.0","method":"ready"}', flush=True)
time.sleep(60)
"#,
    )
    .await;
    inspector.disconnect_gracefully();
    assert_eq!(stopped_by(&inspector).await, Stop::Terminated);
}

#[cfg(unix)]
#[tokio::test]
async fn a_graceful_disconnect_kills_an_agent_that_ignores_sigterm() {
    let inspector = running(
        r#"
import signal, time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
print('{"jsonrpc":"2.0","method":"ready"}', flush=True)
time.sleep(60)
"#,
    )
    .await;
    inspector.disconnect_gracefully();
    assert_eq!(stopped_by(&inspector).await, Stop::Killed);
}

#[cfg(unix)]
#[tokio::test]
async fn an_exited_agent_is_reported_lost_though_an_escaped_process_holds_its_pipes() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import os, sys, time
if os.fork() == 0:
    os.setsid()
    time.sleep(5)
    os._exit(0)
print('{"jsonrpc":"2.0","method":"bye"}', flush=True)
"#,
    ]));
    tokio::time::timeout(
        Duration::from_secs(4),
        until(&mut status, "the exit to be reported", || {
            inspector.status() == ConnectionStatus::Lost
        }),
    )
    .await
    .expect("EOF that never comes must not hold the exit back");
    assert_eq!(received(&inspector).len(), 1);
    let diagnostics = inspector.diagnostics().entries();
    assert!(
        diagnostics
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::DrainAbandoned))
    );
    assert!(matches!(
        diagnostics.last().unwrap().kind,
        DiagnosticKind::AgentExited(_)
    ));
    tokio::time::sleep(QUIET).await;
}

/// The writer is the third pipe an escaped process can hold: a frame larger
/// than the pipe's buffer blocks against a stdin nobody reads, and waiting for
/// that write held the exit back until the escaped process let go.
#[cfg(unix)]
#[tokio::test]
async fn an_exited_agent_is_reported_lost_though_an_escaped_process_holds_its_stdin_mid_write() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import os, sys, time
if os.fork() == 0:
    os.setsid()
    time.sleep(12)
    os._exit(0)
print('{"jsonrpc":"2.0","method":"hello"}', flush=True)
time.sleep(0.5)
"#,
    ]));
    // Over any pipe buffer, so the write is still in progress at the exit.
    let params = serde_json::value::to_raw_value(&"x".repeat(4 << 20)).unwrap();
    inspector
        .send_raw("x/large", Some(&params), true)
        .await
        .expect("handed to the transport");
    tokio::time::timeout(
        Duration::from_secs(8),
        until(&mut status, "the exit to be reported", || {
            inspector.status() == ConnectionStatus::Lost
        }),
    )
    .await
    .expect("a write that never completes must not hold the exit back");
    let diagnostics = inspector.diagnostics().entries();
    assert!(
        diagnostics
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::WriteFailed(_))),
        "{diagnostics:?}"
    );
    assert!(matches!(
        diagnostics.last().unwrap().kind,
        DiagnosticKind::AgentExited(_)
    ));
    tokio::time::sleep(QUIET).await;
}
