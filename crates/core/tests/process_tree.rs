//! Process ownership through real stdio Connections and the public Inspector.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use acp_inspector_core::{
    ConnectionFactory, ConnectionStatus, DiagnosticKind, Inspector, StdioSpawn, TurnState, v1,
};
use common::{next_frame, received, until};

// A TCP listener gives us an OS-owned liveness witness, including on Windows
// and on Linux hosts whose PID 1 leaves dead orphan processes as zombies.
const DESCENDANT: &str = r#"
import socket, time
s = socket.socket()
s.bind(('127.0.0.1', 0))
s.listen()
print(s.getsockname()[1], flush=True)
s.settimeout(0.1)
end = time.monotonic() + 60
while time.monotonic() < end:
    try:
        connection, _ = s.accept()
        with connection:
            connection.settimeout(0.1)
            if connection.recv(1) == b'q':
                break
    except socket.timeout:
        pass
"#;

fn python() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".into())
}

fn launcher() -> StdioSpawn {
    // Two generations below the launcher, like a package runner and its Agent.
    let wrapper = format!(
        "import subprocess, sys; subprocess.Popen([sys.executable, '-c', {DESCENDANT:?}]).wait()"
    );
    StdioSpawn::new(python()).args([
        "-c",
        &format!(
            r#"
import subprocess, sys, time
subprocess.Popen([sys.executable, '-c', {wrapper:?}]).wait()
"#
        ),
    ])
}

// On a failed assertion, tell only this fixture's listener to exit. Its parents
// are waiting for it, so a red test doesn't leave a tree running for 60 seconds.
struct Witness(u16);

impl Drop for Witness {
    fn drop(&mut self) {
        use std::io::Write;
        if let Ok(mut stream) =
            std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.0))
        {
            let _ = stream.write_all(b"q");
        }
    }
}

fn listening(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(50))
        .is_ok()
}

async fn stopped(port: u16) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while listening(port) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the owned descendant must stop, not outlive its Connection");
}

fn stopped_without_executor(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while listening(port) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !listening(port),
        "shutdown must terminate the tree without another executor tick"
    );
}

struct Survivor(Child);

impl Survivor {
    fn spawn() -> (Self, u16) {
        let mut child = Command::new(python())
            .args(["-c", DESCENDANT])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut port = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        (Self(child), port.trim().parse().unwrap())
    }
}

impl Drop for Survivor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn dropping_connection_stops_descendant_but_not_unrelated_process() {
    let (_survivor, unrelated) = Survivor::spawn();
    let mut connection = launcher().connect();
    let port = next_frame(connection.incoming())
        .await
        .as_str()
        .parse()
        .unwrap();
    assert!(listening(port));
    let _cleanup = Witness(port);
    let start = Instant::now();
    drop(connection);
    assert!(start.elapsed() < Duration::from_millis(500));
    stopped(port).await;
    assert!(
        listening(unrelated),
        "cleanup must only reach owned processes"
    );
}

async fn connected(inspector: &Inspector) -> u16 {
    let before = received(inspector).len();
    let mut changes = inspector.trace().changes();
    inspector.connect(&launcher());
    until(&mut changes, "the descendant's listening port", || {
        received(inspector).len() > before
    })
    .await;
    received(inspector).last().unwrap().parse().unwrap()
}

#[tokio::test]
async fn dropping_last_inspector_owner_stops_its_tree() {
    let inspector = Inspector::new();
    let port = connected(&inspector).await;
    let _cleanup = Witness(port);
    let last = inspector.clone();
    drop(inspector);
    assert!(
        listening(port),
        "another public owner still holds the Inspector"
    );
    drop(last);
    stopped(port).await;
}

#[tokio::test]
async fn disconnect_and_reconnect_only_stop_the_previous_tree() {
    let inspector = Inspector::new();
    let first = connected(&inspector).await;
    let _first_cleanup = Witness(first);
    let second = connected(&inspector).await;
    let _second_cleanup = Witness(second);
    stopped(first).await;
    assert!(listening(second));
    assert_eq!(inspector.status(), ConnectionStatus::Connected);
    let start = Instant::now();
    inspector.disconnect();
    assert!(start.elapsed() < Duration::from_millis(500));
    stopped(second).await;
}

#[test]
fn dropping_runtime_stops_tree_even_with_connection_still_owned() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (connection, port) = runtime.block_on(async {
        let mut connection = launcher().connect();
        let port: u16 = next_frame(connection.incoming())
            .await
            .as_str()
            .parse()
            .unwrap();
        (connection, port)
    });
    let _cleanup = Witness(port);
    drop(runtime);
    stopped_without_executor(port);
    drop(connection);
}

#[test]
fn native_shutdown_disconnect_needs_no_further_executor_ticks() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let inspector = Inspector::new();
    let port = runtime.block_on(connected(&inspector));
    let _cleanup = Witness(port);
    // The event loop can call disconnect immediately before process exit,
    // without polling the transport or Inspector reader again.
    inspector.disconnect();
    stopped_without_executor(port);
    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
}

#[tokio::test]
async fn launcher_exit_ends_inherited_pipes_and_preserves_final_turn_evidence() {
    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&StdioSpawn::new(python()).args([
        "-c",
        &format!(
            r#"
import json, subprocess, sys
subprocess.Popen([sys.executable, '-c', {DESCENDANT:?}])
def answer(result):
    request = json.loads(sys.stdin.readline())
    print(json.dumps({{'jsonrpc': '2.0', 'id': request['id'], 'result': result}}), flush=True)
answer({{'protocolVersion': 1, 'agentCapabilities': {{}}}})
answer({{'sessionId': 'final'}})
request = json.loads(sys.stdin.readline())
for i in range(1000):
    print(json.dumps({{'jsonrpc': '2.0', 'method': 'final/evidence', 'params': {{'ordinal': i}}}}))
    print(str(i), file=sys.stderr)
print(json.dumps({{'jsonrpc': '2.0', 'id': request['id'], 'result': {{'stopReason': 'end_turn'}}}}))
sys.stdout.flush()
sys.stderr.flush()
"#
        ),
    ]));
    until(&mut frames, "the inherited-pipe descendant", || {
        !received(&inspector).is_empty()
    })
    .await;
    let port: u16 = received(&inspector)[0].parse().unwrap();
    let _cleanup = Witness(port);
    assert!(listening(port));
    inspector.initialize().await.unwrap();
    inspector.new_session("/tmp", None).await.unwrap();
    let mut status = inspector.status_changes();
    let prompt = inspector.submit_prompt(vec!["finish".into()]).unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        until(&mut status, "launcher exit", || {
            inspector.status() == ConnectionStatus::Lost
        }),
    )
    .await
    .expect("inherited stdout/stderr must not postpone exit indefinitely");
    assert_eq!(prompt.await.unwrap().unwrap(), v1::StopReason::EndTurn);
    assert!(matches!(
        inspector.turn(),
        TurnState::Ended(v1::StopReason::EndTurn)
    ));
    assert_eq!(received(&inspector).len(), 1004);
    let diagnostics = inspector.diagnostics().entries();
    assert_eq!(diagnostics.len(), 1001);
    for (i, line) in diagnostics[..1000].iter().enumerate() {
        assert!(matches!(&line.kind, DiagnosticKind::Stderr(text) if text == &i.to_string()));
    }
    assert!(
        matches!(diagnostics[1000].kind, DiagnosticKind::AgentExited(status) if status.success())
    );
    stopped(port).await;
}
