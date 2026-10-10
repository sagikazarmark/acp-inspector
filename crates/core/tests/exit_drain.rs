//! Exit evidence through the public Inspector and real subprocess seam.

mod common;

use std::sync::Arc;

use acp_inspector_core::{
    Connection, ConnectionFactory, ConnectionStatus, Diagnostic, DiagnosticKind, Frame, Inspector,
    StdioSpawn, TransportEnds, TurnState, connection, v1,
};
use common::{PATIENCE, QUIET, received, until};

/// A transport whose agent is a script run against the transport ends, for
/// the failures no subprocess can be made to produce.
struct Scripted<F>(std::sync::Mutex<Option<F>>);

impl<F, R> ConnectionFactory for Scripted<F>
where
    F: FnOnce(TransportEnds) -> R,
    R: std::future::Future<Output = ()> + Send + 'static,
{
    fn connect(&self) -> Connection {
        let (connection, ends) = connection();
        let script = self.0.lock().unwrap().take().expect("connected once");
        tokio::spawn(script(ends));
        connection
    }
}

fn scripted<F, R>(script: F) -> Scripted<F>
where
    F: FnOnce(TransportEnds) -> R,
    R: std::future::Future<Output = ()> + Send + 'static,
{
    Scripted(std::sync::Mutex::new(Some(script)))
}

fn broke() -> Diagnostic {
    Diagnostic::now(DiagnosticKind::TransportFailed(Arc::new(
        std::io::Error::other("the pipe broke"),
    )))
}

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
        // Whether a reply's write fails is up to the OS: on macOS a child spawned
        // by a concurrent test can inherit this pipe's read end (pipe() and
        // FD_CLOEXEC are two calls there), and then the write succeeds. What the
        // transport guarantees is the order: any write failure precedes the exit.
        let diagnostics = inspector.diagnostics().entries();
        if let Some(failed) = diagnostics
            .iter()
            .position(|line| matches!(line.kind, DiagnosticKind::WriteFailed(_)))
        {
            let exited = diagnostics
                .iter()
                .position(|line| matches!(line.kind, DiagnosticKind::AgentExited(_)))
                .expect("the exit is reported");
            assert!(
                failed < exited,
                "a write failure must be reported before the exit"
            );
        }
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
    // A pipe that cannot be read has no subprocess recipe (any bytes are now a
    // frame), so the transport is scripted: replies are never taken, so the
    // automatic ones back up, and then the read fails.
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&scripted(|ends: TransportEnds| async move {
        let TransportEnds {
            outgoing,
            incoming,
            diagnostics,
            shutdown,
            ..
        } = ends;
        for i in 0..400 {
            let request = format!(r#"{{"jsonrpc":"2.0","id":"{i}","method":"unsupported"}}"#);
            if incoming.send(Frame::new(request)).await.is_err() {
                return;
            }
        }
        let _ = diagnostics.send(broke()).await;
        shutdown.cancelled().await;
        drop(outgoing);
    }));
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
async fn closed_stdout_and_closed_stderr_do_not_end_a_live_agent() {
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
    // The line that is not UTF-8 is still on the console, and marked.
    let diagnostics = inspector.diagnostics().entries();
    assert!(
        matches!(&diagnostics[..], [line, note]
            if matches!(&line.kind, DiagnosticKind::Stderr(text) if text == "\u{FFFD}")
                && matches!(note.kind, DiagnosticKind::StderrLineLossy)),
        "{diagnostics:?}"
    );
    inspector.disconnect();
    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
}

#[tokio::test]
async fn stderr_that_is_not_utf8_is_drained_to_the_end_and_the_agent_lives() {
    // One bad byte used to end console capture, and the agent then died of a
    // broken pipe on its next stderr write.
    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import sys, time
err = sys.stderr.buffer
err.write(b'before \xff\xfe bad bytes\n'); err.flush()
time.sleep(0.1)
for i in range(2000):
    err.write(('after %d %s\n' % (i, 'x' * 100)).encode()); err.flush()
print('{"jsonrpc":"2.0","method":"_probe/done"}', flush=True)
sys.stdin.read()
"#,
    ]));
    until(&mut frames, "the agent to finish writing stderr", || {
        !received(&inspector).is_empty()
    })
    .await;
    let mut lines = inspector.diagnostics().changes();
    until(&mut lines, "every stderr line", || {
        inspector.diagnostics().len() >= 2002
    })
    .await;
    let diagnostics = inspector.diagnostics().entries();
    assert!(
        matches!(&diagnostics[0].kind, DiagnosticKind::Stderr(text) if text == "before \u{FFFD}\u{FFFD} bad bytes")
    );
    assert!(matches!(
        diagnostics[1].kind,
        DiagnosticKind::StderrLineLossy
    ));
    for (i, line) in diagnostics[2..2002].iter().enumerate() {
        assert!(
            matches!(&line.kind, DiagnosticKind::Stderr(text) if text.starts_with(&format!("after {i} "))),
            "{line:?}"
        );
    }
    assert_eq!(inspector.status(), ConnectionStatus::Connected);
    inspector.disconnect();
}

#[tokio::test]
async fn a_stdout_line_that_is_not_utf8_is_a_recorded_frame_and_the_handshake_completes() {
    let inspector = Inspector::new();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        r#"
import json, sys
out = sys.stdout.buffer
request = json.loads(sys.stdin.readline())
out.write(b'{"jsonrpc":"2.0","method":"_x/note","params":{"s":"\xff"}}\n')
out.write(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': {'protocolVersion': 1, 'agentCapabilities': {}}}).encode() + b'\n')
out.flush()
sys.stdin.read()
"#,
    ]));
    tokio::time::timeout(PATIENCE, inspector.initialize())
        .await
        .unwrap()
        .expect("the answer after the bad line still answers");
    assert_eq!(inspector.status(), ConnectionStatus::Connected);
    let traced = inspector.trace().entries();
    let lossy = &traced[1].frame;
    assert!(lossy.is_lossy());
    assert_eq!(
        lossy.as_str(),
        "{\"jsonrpc\":\"2.0\",\"method\":\"_x/note\",\"params\":{\"s\":\"\u{FFFD}\"}}"
    );
    assert_eq!(
        lossy.bytes(),
        b"{\"jsonrpc\":\"2.0\",\"method\":\"_x/note\",\"params\":{\"s\":\"\xff\"}}"
    );
    assert!(!traced[2].frame.is_lossy());
    inspector.disconnect();
}

#[tokio::test]
async fn pipe_failure_settles_a_pending_call_without_waiting_for_process_exit() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&scripted(|ends: TransportEnds| async move {
        let TransportEnds {
            mut outgoing,
            diagnostics,
            shutdown,
            ..
        } = ends;
        let _ = outgoing.recv().await;
        let _ = diagnostics.send(broke()).await;
        // The process is still running: nothing reports an exit.
        shutdown.cancelled().await;
    }));
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
    // Only how each replaced agent stopped, never its stderr.
    assert!(
        inspector
            .diagnostics()
            .entries()
            .iter()
            .all(|line| matches!(line.kind, DiagnosticKind::Stopped { .. }))
    );
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
