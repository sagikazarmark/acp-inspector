//! The stdio transport through its public seam: spawn a process, exchange whole
//! frames, and read its diagnostic channel. Nothing here reaches inside the
//! connection.
//!
//! The agents are `/bin/sh` one-liners rather than Testy because what is under
//! test is the transport's own behaviour — framing, stderr, spawn failure, the
//! four spawn fields — and a shell says exactly what it will emit. Testy drives
//! the protocol-shaped test next door (`testy.rs`).

mod common;

use std::time::SystemTime;

use acp_inspector_core::{ConnectionFactory, DiagnosticKind, Frame, StdioSpawn};

use common::{QUIET, frames_end, next_diagnostic, next_frame};

#[tokio::test]
async fn failed_write_releases_senders_before_diagnostic_capacity_is_available() {
    let mut connection = StdioSpawn::new("python3")
        .args([
            "-c",
            r#"
import os, sys, time
os.close(0)
# More lines than the diagnostic queue, fewer bytes than the OS pipe. The
# consumer deliberately does not read diagnostics until sends have failed.
sys.stderr.write('x\n'*1000)
sys.stderr.flush()
print('{"ready":true}', flush=True)
time.sleep(60)
"#,
        ])
        .connect();
    assert_eq!(
        next_frame(connection.incoming()).await.as_str(),
        r#"{"ready":true}"#
    );
    tokio::time::sleep(QUIET).await;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        for _ in 0..1000 {
            if let Err(error) = connection.outgoing().send(Frame::new("{}")).await {
                assert_eq!(error, acp_inspector_core::SendError::Disconnected);
                return;
            }
        }
        panic!("a failed writer must close the outgoing queue");
    })
    .await
    .expect("a full diagnostic channel must not keep failed sends blocked");
    let mut stderr = 0;
    let mut failed = false;
    for _ in 0..1001 {
        match next_diagnostic(connection.diagnostics()).await.kind {
            DiagnosticKind::Stderr(_) => stderr += 1,
            DiagnosticKind::WriteFailed(_) => failed = true,
            other => panic!("unexpected diagnostic: {other:?}"),
        }
    }
    assert_eq!(stderr, 1000);
    assert!(failed);
}

/// An agent that writes back every line it is given, and ends on stdin EOF.
fn echo_agent() -> StdioSpawn {
    shell("while IFS= read -r line; do printf '%s\\n' \"$line\"; done")
}

fn shell(script: &str) -> StdioSpawn {
    StdioSpawn::new("/bin/sh").arg("-c").arg(script)
}

#[tokio::test]
async fn frames_cross_whole_in_both_directions() {
    let mut connection = echo_agent().connect();

    connection
        .outgoing()
        .send(Frame::new(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        ))
        .await
        .expect("the agent is alive");

    let frame = next_frame(connection.incoming()).await;
    assert_eq!(
        frame.as_str(),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#
    );
}

#[tokio::test]
async fn one_write_carrying_two_frames_arrives_as_two_frames() {
    // Framing is the transport's job (docs/architecture.md §6.1): what the agent
    // writes in one syscall is two JSON-RPC messages, so it is two frames.
    let mut connection = shell(r#"printf '%s\n%s\n' '{"one":1}' '{"two":2}'"#).connect();

    assert_eq!(
        next_frame(connection.incoming()).await.as_str(),
        r#"{"one":1}"#
    );
    assert_eq!(
        next_frame(connection.incoming()).await.as_str(),
        r#"{"two":2}"#
    );
}

#[tokio::test]
async fn a_frame_split_across_writes_is_reassembled() {
    let mut connection = shell(r#"printf '{"half":'; sleep 0.2; printf '1}\n'"#).connect();

    assert_eq!(
        next_frame(connection.incoming()).await.as_str(),
        r#"{"half":1}"#
    );
}

#[tokio::test]
async fn stderr_lines_arrive_line_timestamped() {
    let before = SystemTime::now();
    let mut connection = shell("echo first >&2; echo second >&2; cat > /dev/null").connect();

    let first = next_diagnostic(connection.diagnostics()).await;
    let second = next_diagnostic(connection.diagnostics()).await;

    let DiagnosticKind::Stderr(line) = &first.kind else {
        panic!("expected an agent stderr line, got {first:?}");
    };
    assert_eq!(line, "first");
    let DiagnosticKind::Stderr(line) = &second.kind else {
        panic!("expected an agent stderr line, got {second:?}");
    };
    assert_eq!(line, "second");

    assert!(first.at >= before, "a line is stamped when it is read");
    assert!(
        second.at >= first.at,
        "lines are stamped in the order they arrive"
    );
}

#[tokio::test]
async fn the_spawn_fields_reach_the_child() {
    let mut connection =
        shell("printf '%s\\n' \"$INSPECTOR_TEST_MARK\" >&2; pwd >&2; cat > /dev/null")
            .env("INSPECTOR_TEST_MARK", "carried")
            .cwd("/tmp")
            .connect();

    let mark = next_diagnostic(connection.diagnostics()).await;
    let cwd = next_diagnostic(connection.diagnostics()).await;

    assert!(
        matches!(&mark.kind, DiagnosticKind::Stderr(line) if line == "carried"),
        "{mark:?}"
    );
    assert!(
        matches!(&cwd.kind, DiagnosticKind::Stderr(line) if line.ends_with("/tmp")),
        "{cwd:?}"
    );
}

#[tokio::test]
async fn a_spawn_failure_surfaces_its_evidence_instead_of_hanging() {
    // The failure flow the desktop shell auto-surfaces (docs/architecture.md §9):
    // the evidence is in the diagnostic channel, and the frame stream ends —
    // a client waiting for the agent's first frame is told, not left waiting.
    let mut connection = StdioSpawn::new("inspector-no-such-agent-2f9c1a").connect();

    let diagnostic = next_diagnostic(connection.diagnostics()).await;
    let DiagnosticKind::SpawnFailed { command, error } = &diagnostic.kind else {
        panic!("expected a spawn failure, got {diagnostic:?}");
    };
    assert!(
        command.contains("inspector-no-such-agent-2f9c1a"),
        "{command}"
    );
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);

    assert!(
        frames_end(connection.incoming()).await,
        "a connection that never started must not leave the stream open"
    );
}

#[tokio::test]
async fn a_frame_too_long_to_carry_is_dropped_and_announced_and_the_wire_survives() {
    // An agent that emits an 11 MB line is misbehaviour, which is the subject
    // matter (§7.3, §8): the frame is the only thing lost, the loss is on the
    // record, and whatever the agent says next still arrives.
    let mut connection = shell(
        r#"head -c 11000000 /dev/zero | tr '\0' 'x'; printf '\n'; printf '%s\n' '{"after":1}'"#,
    )
    .connect();

    let dropped = next_diagnostic(connection.diagnostics()).await;
    assert!(
        matches!(dropped.kind, DiagnosticKind::FrameDropped { .. }),
        "{dropped:?}"
    );

    assert_eq!(
        next_frame(connection.incoming()).await.as_str(),
        r#"{"after":1}"#
    );
}

#[tokio::test]
async fn a_frame_the_transport_cannot_carry_as_one_message_is_refused() {
    // The trace is the record of what crossed the wire, so the seam refuses what
    // would cross as two messages while the trace would show one.
    let connection = echo_agent().connect();

    let refused = connection
        .outgoing()
        .send(Frame::new("{\"one\":1}\n{\"two\":2}"))
        .await;

    assert!(
        matches!(refused, Err(acp_inspector_core::SendError::NotOneMessage)),
        "{refused:?}"
    );
}

#[tokio::test]
async fn dropping_the_sink_ends_the_agent_and_reports_its_exit() {
    let connection = echo_agent().connect();
    let (outgoing, mut incoming, mut diagnostics) = connection.into_parts();

    // Closing the client's end of stdin is how an stdio agent is asked to stop
    // (`app/bridge` §3.4's first step).
    drop(outgoing);

    assert!(
        frames_end(&mut incoming).await,
        "the agent wrote nothing more"
    );

    let exit = next_diagnostic(&mut diagnostics).await;
    let DiagnosticKind::AgentExited(status) = exit.kind else {
        panic!("expected the agent's exit, got {exit:?}");
    };
    assert!(status.success(), "the echo agent ends cleanly on stdin EOF");
}
