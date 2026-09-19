//! Exit evidence through the public Inspector and real subprocess seam.

mod common;

use acp_inspector_core::{ConnectionStatus, DiagnosticKind, Inspector, StdioSpawn, TurnState, v1};
use common::{PATIENCE, QUIET, received, until};

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
