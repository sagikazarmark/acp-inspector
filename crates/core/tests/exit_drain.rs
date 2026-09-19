//! Exit evidence through the public Inspector and real subprocess seam.

mod common;

use acp_inspector_core::{ConnectionStatus, DiagnosticKind, Inspector, StdioSpawn, TurnState, v1};
use common::{PATIENCE, QUIET, received, until};

#[tokio::test]
async fn failed_automatic_replies_preserve_all_final_frames_and_prompt_answer() {
    for _ in 0..8 {
        let inspector = Inspector::new();
        let mut status = inspector.status_changes();
        inspector.connect(&StdioSpawn::new("python3").args(["-c", r#"
import json, sys, os
for result in [{'protocolVersion': 1, 'agentCapabilities': {}}, {'sessionId': 'final'}]:
    request = json.loads(sys.stdin.readline())
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
request = json.loads(sys.stdin.readline())
# Guarantee failed replies, independent of whether the reader beats process exit.
os.close(0)
frames = [json.dumps({'jsonrpc': '2.0', 'id': 'agent-'+str(i), 'method': 'unsupported'}) for i in range(80)]
frames.append(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': {'stopReason': 'end_turn'}}))
sys.stdout.write('\n'.join(frames)+'\n')
sys.stdout.flush()
os._exit(0)
"#]));
        tokio::time::timeout(PATIENCE, async {
            inspector.initialize().await.unwrap();
            inspector.new_session("/tmp", None).await.unwrap();
        })
        .await
        .unwrap();
        let prompt = inspector.submit_prompt(vec!["finish".into()]).unwrap();
        until(&mut status, "exit despite failed automatic replies", || {
            inspector.status() == ConnectionStatus::Lost
        })
        .await;
        let frames = received(&inspector);
        assert_eq!(frames.len(), 83, "all Agent Frames must drain");
        assert_eq!(inspector.trace().dropped(), 0);
        for (i, frame) in frames[2..82].iter().enumerate() {
            let frame: serde_json::Value = serde_json::from_str(frame).unwrap();
            assert_eq!(frame["id"], format!("agent-{i}"));
        }
        assert_eq!(prompt.await.unwrap().unwrap(), v1::StopReason::EndTurn);
        assert!(matches!(
            inspector.turn(),
            TurnState::Ended(v1::StopReason::EndTurn)
        ));
        let diagnostics = inspector.diagnostics().entries();
        assert!(
            diagnostics
                .iter()
                .any(|line| matches!(line.kind, DiagnosticKind::WriteFailed(_)))
        );
        assert!(
            matches!(diagnostics.last().unwrap().kind, DiagnosticKind::AgentExited(status) if status.success())
        );
    }
}

#[tokio::test]
async fn failed_automatic_replies_with_full_diagnostics_do_not_deadlock_exit() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import os, sys, json, time, threading
def noise():
    time.sleep(0.1)
    for i in range(2000):
        print('stderr-'+str(i), file=sys.stderr, flush=True)
threading.Thread(target=noise, daemon=True).start()
threading.Timer(0.4, lambda: os._exit(0)).start()
for i in range(10000):
    print(json.dumps({'jsonrpc': '2.0', 'id': i, 'method': 'unsupported'}), flush=True)
time.sleep(0.2)
os._exit(0)
"#,
    ]));
    let exited = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        until(
            &mut status,
            "exit under reply and diagnostic backpressure",
            || inspector.status() == ConnectionStatus::Lost,
        )
        .await;
    })
    .await;
    assert!(
        exited.is_ok(),
        "exited Agent stayed {:?}: {} Frames, {} diagnostics",
        inspector.status(),
        inspector.trace().len(),
        inspector.diagnostics().entries().len()
    );
    let diagnostics = inspector.diagnostics().entries();
    assert!(
        diagnostics
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::Stderr(_)))
    );
    assert!(
        diagnostics
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::WriteFailed(_)))
    );
    assert!(
        matches!(diagnostics.last().unwrap().kind, DiagnosticKind::AgentExited(status) if status.success())
    );
}

#[tokio::test]
async fn failed_replies_drain_every_frame_and_stderr_line_after_backpressure() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args(["-c", r#"
import json, os, sys, threading, time
for result in [{'protocolVersion': 1, 'agentCapabilities': {}}, {'sessionId': 'final'}]:
    request = json.loads(sys.stdin.readline())
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
request = json.loads(sys.stdin.readline())
def noise():
    time.sleep(0.1)
    for i in range(1000):
        print(str(i), file=sys.stderr, flush=True)
noise_thread = threading.Thread(target=noise)
noise_thread.start()
# Unblock failed writes even if the main thread is stuck writing stdout. All
# final evidence is then written before exit, so its exact count is assertable.
threading.Timer(0.4, lambda: os.close(0)).start()
for i in range(3000):
    print(json.dumps({'jsonrpc': '2.0', 'id': 'agent-'+str(i), 'method': 'unsupported'}), flush=True)
print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': {'stopReason': 'end_turn'}}), flush=True)
noise_thread.join()
"#]));
    tokio::time::timeout(PATIENCE, async {
        inspector.initialize().await.unwrap();
        inspector.new_session("/tmp", None).await.unwrap();
    })
    .await
    .unwrap();
    let prompt = inspector.submit_prompt(vec!["finish".into()]).unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        until(
            &mut status,
            "all final evidence after failed replies",
            || inspector.status() == ConnectionStatus::Lost,
        ),
    )
    .await
    .unwrap();
    let frames = received(&inspector);
    assert_eq!(frames.len(), 3003);
    assert_eq!(inspector.trace().dropped(), 0);
    for (i, frame) in frames[2..3002].iter().enumerate() {
        let frame: serde_json::Value = serde_json::from_str(frame).unwrap();
        assert_eq!(frame["id"], format!("agent-{i}"));
    }
    assert_eq!(prompt.await.unwrap().unwrap(), v1::StopReason::EndTurn);
    assert!(matches!(
        inspector.turn(),
        TurnState::Ended(v1::StopReason::EndTurn)
    ));
    let diagnostics = inspector.diagnostics().entries();
    let stderr: Vec<_> = diagnostics
        .iter()
        .filter_map(|line| match &line.kind {
            DiagnosticKind::Stderr(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(stderr.len(), 1000);
    for (i, text) in stderr.iter().enumerate() {
        assert_eq!(**text, i.to_string());
    }
    assert_eq!(diagnostics.len(), 1002, "stderr, write failure, exit");
    assert!(
        matches!(diagnostics.last().unwrap().kind, DiagnosticKind::AgentExited(status) if status.success())
    );
}

#[tokio::test]
async fn read_failure_interrupts_a_backpressured_automatic_reply() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import json, os, sys, time
# Long ids fill the stdin pipe well before the bounded outgoing queue. The
# remaining Frames fit in the incoming queue, letting its pump see the error.
for i in range(400):
    print(json.dumps({'jsonrpc': '2.0', 'id': str(i)+'x'*4096, 'method': 'unsupported'}))
sys.stdout.flush()
os.write(1, b'\xff\n')
time.sleep(60)
"#,
    ]));
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        until(&mut status, "read failure during a blocked reply", || {
            inspector.status() == ConnectionStatus::Lost
        }),
    )
    .await
    .expect("a terminal read failure must interrupt a blocked automatic reply");
    assert!(
        inspector
            .diagnostics()
            .entries()
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::TransportFailed(_)))
    );
}

const FINAL_TURN: &str = r#"
import json, sys
def answer(result):
    request = json.loads(sys.stdin.readline())
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
answer({'protocolVersion': 1, 'agentCapabilities': {}})
answer({'sessionId': 'final'})
request = json.loads(sys.stdin.readline())
frames = [json.dumps({'jsonrpc': '2.0', 'method': 'final/evidence', 'params': {'ordinal': i}}) for i in range(1000)]
frames.append(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': {'stopReason': 'end_turn'}}))
sys.stdout.write('\n'.join(frames) + '\n')
sys.stdout.flush()
sys.stderr.write('goodbye\n')
sys.stderr.flush()
"#;

#[tokio::test]
async fn exit_preserves_final_frames_and_prompt_outcome_before_reporting_lost() {
    // More than the bounded pipe can queue, fewer than the Trace can retain.
    // Repeat to exercise the independently scheduled stdout and exit tasks.
    for _ in 0..8 {
        let inspector = Inspector::new();
        let mut status = inspector.status_changes();
        inspector.connect(&StdioSpawn::new("python3").args(["-c", FINAL_TURN]));
        tokio::time::timeout(PATIENCE, async {
            inspector.initialize().await.unwrap();
            inspector.new_session("/tmp", None).await.unwrap();
        })
        .await
        .expect("the handshake completes");
        let prompt = inspector.submit_prompt(vec!["finish".into()]).unwrap();
        until(&mut status, "exit after the final Frames", || {
            inspector.status() == ConnectionStatus::Lost
        })
        .await;

        let frames = received(&inspector);
        assert_eq!(
            frames.len(),
            1003,
            "two setup responses, 1000 notifications, final response"
        );
        assert_eq!(inspector.trace().dropped(), 0);
        for (ordinal, frame) in frames[2..1002].iter().enumerate() {
            let frame: serde_json::Value = serde_json::from_str(frame).unwrap();
            assert_eq!(frame["params"]["ordinal"], ordinal);
        }
        assert!(matches!(
            inspector.turn(),
            TurnState::Ended(v1::StopReason::EndTurn)
        ));
        assert_eq!(
            tokio::time::timeout(PATIENCE, prompt)
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            v1::StopReason::EndTurn
        );
        let diagnostics = inspector.diagnostics().entries();
        assert!(matches!(&diagnostics[0].kind, DiagnosticKind::Stderr(line) if line == "goodbye"));
        assert!(
            matches!(diagnostics[1].kind, DiagnosticKind::AgentExited(status) if status.success())
        );
    }
}

#[tokio::test]
async fn closed_stdout_and_failed_stderr_do_not_end_a_live_agent() {
    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import os, sys
print('{"jsonrpc":"2.0","method":"ready"}', flush=True)
os.close(1)
os.write(2, b'\xff\n')
os.close(2)
sys.stdin.read()
"#,
    ]));
    until(&mut frames, "the final stdout Frame", || {
        inspector.trace().len() == 1
    })
    .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(inspector.status(), ConnectionStatus::Connected);
    inspector.disconnect();
    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
}

#[tokio::test]
async fn pipe_failure_settles_a_pending_call_without_waiting_for_process_exit() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import os, sys
sys.stdin.readline()
os.write(1, b'\xff\n')
sys.stdin.read()
"#,
    ]));
    let result = tokio::time::timeout(PATIENCE, inspector.initialize())
        .await
        .unwrap();
    assert!(matches!(
        result,
        Err(acp_inspector_core::CallError::Disconnected)
    ));
    until(&mut status, "the broken pipe to end the Connection", || {
        inspector.status() == ConnectionStatus::Lost
    })
    .await;
    assert!(
        inspector
            .diagnostics()
            .entries()
            .iter()
            .any(|line| matches!(line.kind, DiagnosticKind::TransportFailed(_)))
    );
}

#[tokio::test]
async fn disconnect_interrupts_a_reader_backpressured_by_an_agent_not_reading_stdin() {
    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import sys
for i in range(10000):
    print('{"jsonrpc":"2.0","id":%d,"method":"unsupported"}' % i, flush=True)
sys.stdin.read()
"#,
    ]));
    until(
        &mut frames,
        "enough requests to backpressure replies",
        || inspector.trace().len() >= 1000,
    )
    .await;
    inspector.disconnect();
    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
    let count = inspector.trace().len();
    tokio::time::sleep(QUIET).await;
    assert_eq!(inspector.trace().len(), count);
}

#[tokio::test]
async fn reconnect_is_not_released_or_polluted_by_the_cancelled_reader() {
    let inspector = Inspector::new();
    for _ in 0..16 {
        // Both readers can first be scheduled after the replacement is installed.
        inspector.connect(&StdioSpawn::new("python3").args([
            "-c",
            r#"
import sys
print('{"jsonrpc":"2.0","method":"old/evidence"}', flush=True)
print('old diagnostic', file=sys.stderr, flush=True)
"#,
        ]));
        inspector.connect(&StdioSpawn::new("python3").args(["-c", r#"
import json, sys
request = json.loads(sys.stdin.readline())
print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': {'protocolVersion': 1, 'agentCapabilities': {}}}), flush=True)
sys.stdin.read()
"#]));
        tokio::time::timeout(PATIENCE, inspector.initialize())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inspector.status(), ConnectionStatus::Connected);
    }
    assert_eq!(received(&inspector).len(), 16);
    assert!(inspector.diagnostics().is_empty());
    inspector.disconnect();
}

#[tokio::test]
async fn exit_drains_backpressured_diagnostics_in_order_alongside_frames() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import json, sys
for i in range(1000):
    print(json.dumps({'jsonrpc': '2.0', 'method': 'evidence', 'params': {'ordinal': i}}))
    print(str(i), file=sys.stderr)
sys.stdout.flush()
sys.stderr.flush()
"#,
    ]));
    until(&mut status, "all exit evidence", || {
        inspector.status() == ConnectionStatus::Lost
    })
    .await;
    assert_eq!(received(&inspector).len(), 1000);
    let lines = inspector.diagnostics().entries();
    assert_eq!(lines.len(), 1001);
    for (ordinal, line) in lines[..1000].iter().enumerate() {
        assert!(matches!(&line.kind, DiagnosticKind::Stderr(text) if text == &ordinal.to_string()));
    }
    assert!(matches!(lines[1000].kind, DiagnosticKind::AgentExited(_)));
}
