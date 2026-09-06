//! What the integration tests need to drive a real agent: where Testy is, and
//! awaits that fail the test instead of hanging it.

#![allow(dead_code)] // Each test file uses a subset of the harness.

use std::path::PathBuf;
use std::time::Duration;

use acp_inspector_core::{
    AgentCommand, CallError, Changes, Diagnostic, DiagnosticStream, Direction, ElicitationRequest,
    Frame, FrameStream, Inspector, PermissionRequest, v1,
};

/// Nothing an agent does in these tests takes seconds. The timeout is here so a
/// transport bug reports itself as a failed assertion rather than as a CI job
/// that runs until the runner gives up.
pub const PATIENCE: Duration = Duration::from_secs(20);

/// What a missing Testy should tell whoever hit it.
const NOT_BUILT: &str =
    "Testy is not built. Run `just testy` at the repository root, or set TESTY_BIN.";

/// Testy, built by `just testy` (`scripts/build-testy.sh`).
///
/// `agent-client-protocol-test` is `publish = false`, so the binary cannot be a
/// dev-dependency: it is built from a rust-sdk checkout into the workspace's
/// `.testy/bin`. A missing binary is a loud, actionable failure rather than a
/// silently skipped test — a test suite that quietly stops covering the wire is
/// worse than one that says it cannot run.
pub fn testy() -> PathBuf {
    if let Some(path) = std::env::var_os("TESTY_BIN") {
        return PathBuf::from(path);
    }

    // Two levels up from `crates/core/` is the workspace root, where `just testy`
    // builds it.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.testy/bin/testy")
        .canonicalize()
        .unwrap_or_else(|error| panic!("{NOT_BUILT} ({error})"))
}

pub async fn next_frame(frames: &mut FrameStream) -> Frame {
    tokio::time::timeout(PATIENCE, frames.recv())
        .await
        .expect("waited for a frame and none arrived")
        .expect("the frame stream ended while a frame was expected")
        .frame
}

/// Whether the frame stream ends rather than going quiet — the difference
/// between an agent that is done and a connection that hangs.
pub async fn frames_end(frames: &mut FrameStream) -> bool {
    tokio::time::timeout(PATIENCE, frames.recv())
        .await
        .expect("the frame stream must end rather than stay open with nothing to say")
        .is_none()
}

pub async fn next_diagnostic(diagnostics: &mut DiagnosticStream) -> Diagnostic {
    tokio::time::timeout(PATIENCE, diagnostics.recv())
        .await
        .expect("waited for a diagnostic and none arrived")
        .expect("the diagnostic channel ended while a diagnostic was expected")
}

/// Waits until a store says what the test is waiting for.
///
/// Subscribing rather than polling on a sleep, because it is what the UI does
/// with the same subscription: a test that waits the way the desktop shell
/// waits cannot pass on timing the shell does not have.
pub async fn until<T: Clone>(
    changes: &mut Changes<T>,
    expectation: &str,
    mut ready: impl FnMut() -> bool,
) {
    let waited = tokio::time::timeout(PATIENCE, async {
        while !ready() {
            if changes.next().await.is_none() {
                return false;
            }
        }
        true
    })
    .await;

    match waited {
        Ok(true) => {}
        Ok(false) => panic!("the store stopped changing before {expectation}"),
        Err(_) => panic!("waited for {expectation} and it never happened"),
    }
}

/// Long enough for a chatty agent to have said something more, short enough to
/// keep the suite quick. Only for asserting that nothing *else* arrives — every
/// positive await goes through [`until`].
pub const QUIET: Duration = Duration::from_millis(400);

/// The next frame carrying `needle`, with everything before it returned too —
/// an agent is free to interleave notifications ahead of the response.
pub async fn frames_through(frames: &mut FrameStream, needle: &str) -> Vec<Frame> {
    let mut seen = Vec::new();
    loop {
        let frame = next_frame(frames).await;
        let found = frame.as_str().contains(needle);
        seen.push(frame);
        if found {
            return seen;
        }
    }
}

/// The frames this client sent, as they crossed.
///
/// Read off the trace rather than out of a hook, which is what makes it an
/// assertion about the wire: everything this client says is in there, in order,
/// before any layer above the seam has had an opinion about it.
pub fn sent(inspector: &Inspector) -> Vec<String> {
    inspector
        .trace()
        .entries()
        .into_iter()
        .filter(|entry| entry.direction == Direction::ToAgent)
        .map(|entry| entry.frame.as_str().to_owned())
        .collect()
}

/// The frames the agent sent, as they crossed.
///
/// [`sent`]'s other half, off the same record and for the same reason: what an
/// agent answered, in the order it answered it, before any layer above the seam
/// had an opinion about it.
pub fn received(inspector: &Inspector) -> Vec<String> {
    inspector
        .trace()
        .entries()
        .into_iter()
        .filter(|entry| entry.direction == Direction::FromAgent)
        .map(|entry| entry.frame.as_str().to_owned())
        .collect()
}

/// Where a frame carrying `needle` sits among the ones this client sent.
///
/// The tests that assert an *order* are asserting about the wire, so both of
/// these read positions off the trace rather than off anything that happened
/// in a store.
pub fn position(inspector: &Inspector, needle: &str) -> Option<usize> {
    sent(inspector)
        .iter()
        .position(|frame| frame.contains(needle))
}

/// Where a frame carrying `needle` sits among the ones the agent sent.
pub fn said_at(inspector: &Inspector, needle: &str) -> Option<usize> {
    received(inspector)
        .iter()
        .position(|frame| frame.contains(needle))
}

/// Starts a turn without waiting for it, the way the composer does, and hands
/// back what the agent eventually answered it with.
pub fn prompting(
    inspector: &Inspector,
    prompt: &'static str,
) -> tokio::task::JoinHandle<Result<v1::StopReason, CallError>> {
    tokio::spawn({
        let inspector = inspector.clone();
        async move { inspector.prompt(prompt).await }
    })
}

/// Waits until the turn is not only registered but *asked for*: the prompt is
/// on the wire, so anything sent after this is sent after it.
pub async fn running(inspector: &Inspector) {
    let mut frames = inspector.trace().changes();
    until(&mut frames, "the prompt to reach the agent", || {
        position(inspector, r#""method":"session/prompt""#).is_some()
    })
    .await;
}

/// One `session/update` in `session`, carrying `text` — a line of conversation
/// for a scripted agent to say, already quoted as a `printf` argument.
///
/// Here rather than in either file that needs it: the load-ordering rule is
/// about a conversation crossing before an answer (§15 q7), so the tests for
/// the rule and the tests for the affordance both have to be able to make an
/// agent say something, and one shell literal is enough of them.
pub fn said_in(session: &str, text: &str) -> String {
    format!(
        concat!(
            r#"'{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"{}","#,
            r#""update":{{"sessionUpdate":"agent_message_chunk","#,
            r#""content":{{"type":"text","text":"{}"}}}}}}}}' "#,
        ),
        session, text
    )
}

/// An agent scripted line by line: one argument per line is the spawn form's
/// rule (`AgentCommand::args`), so the script is one line and separates its
/// steps with `;`.
pub fn shell(script: &str) -> AgentCommand {
    AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\n{script}"),
        ..AgentCommand::default()
    }
}

/// An agent that advertises `capabilities`, opens `s1`, says one line in it, and
/// then answers whatever is asked of it next with `answers`, in order.
///
/// **The line it says is what makes a discarded view assertable**: a timeline
/// that is empty after a session ends is only evidence if it had something in it
/// first. It answers by counting rather than by reading ids, which works because
/// this client numbers its calls from one, in order — so the first answer here
/// is the third call, after `initialize` and `session/new`.
///
/// Here rather than in one test file because two of them need the same agent:
/// what an advertisement entitles the inspector to send is one rule (§7.5), and
/// an agent that claims a method it has not implemented is how it is asserted
/// wherever it is asserted.
pub fn advertising(capabilities: &str, answers: &[&str]) -> AgentCommand {
    let handshake = format!(
        concat!(
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1,"agentCapabilities":{}}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"sessionId":"s1"}}}}' {}; "#,
        ),
        capabilities,
        said_in("s1", "before the session was ended"),
    );
    let rest: String = answers
        .iter()
        .map(|answer| format!(r#"read _; printf '%s\n' {answer}; "#))
        .collect();

    shell(&format!("{handshake}{rest}cat > /dev/null"))
}

/// The agent's own description of a session it listed.
///
/// Opening one is handing back what the agent said about it — the id, the
/// working directory, the roots — so the tests take it from the listing rather
/// than building one, which is what the screen does with the row that was
/// clicked (§7.5).
///
/// Here rather than in one test file because two of them reopen a session: what
/// a listed session is a way into is one rule, and the row it is read off is the
/// same row wherever it is driven.
pub async fn listed(inspector: &Inspector, session: &v1::SessionId) -> v1::SessionInfo {
    inspector
        .list_sessions(None)
        .await
        .expect("the agent lists its sessions")
        .sessions
        .into_iter()
        .find(|info| &info.session_id == session)
        .unwrap_or_else(|| panic!("{session} is in the listing"))
}

/// Waits until the agent has said something in the live session, so that a
/// timeline emptied afterwards is a timeline that had something in it.
///
/// Without the wait it would be a race rather than an assertion: the line is
/// written straight after the `session/new` answer, and a test that did not wait
/// for it could discard *before* it landed and then find it in whatever came
/// next.
pub async fn spoken_in(inspector: &Inspector) {
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the session to say something first", || {
        !inspector.timeline().is_empty()
    })
    .await;
}

/// Waits for the agent to ask something, and answers with the elicitation it
/// asked.
///
/// [`blocked`]'s other half (§7.8), subscribing rather than polling for its
/// reason. Sequential by construction: the pending list is empty again the
/// moment one is answered, so a second call waits for the *next* ask.
pub async fn elicited(inspector: &Inspector) -> ElicitationRequest {
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the agent to ask an elicitation", || {
        !inspector.pending_elicitations().is_empty()
    })
    .await;

    inspector
        .pending_elicitations()
        .pop()
        .expect("the elicitation the wait was for")
}

/// A reader who accepts everything, for the tests whose subject is what happens
/// *around* an elicitation rather than what is answered.
///
/// Testy's `callbacks` and `full` stop on five of them, so a test about the turn
/// they are asked in has to answer them the way a person would — otherwise the
/// turn it is waiting on is waiting on the test. Answered with no content, which
/// is a shape the specification permits and Testy accepts.
///
/// Returns the task, because it never ends on its own: the caller aborts it when
/// what it was waiting for has happened.
pub fn accepting_elicitations(inspector: &Inspector) -> tokio::task::JoinHandle<()> {
    let inspector = inspector.clone();
    tokio::spawn(async move {
        let mut changes = inspector.timeline().changes();
        loop {
            for request in inspector.pending_elicitations() {
                let _ = request.accept(None).await;
            }
            if changes.next().await.is_none() {
                return;
            }
        }
    })
}

/// Waits for the agent to block a turn on a permission request, and answers
/// with it.
pub async fn blocked(inspector: &Inspector) -> PermissionRequest {
    let mut changes = inspector.timeline().changes();
    until(
        &mut changes,
        "the turn to block on a permission request",
        || !inspector.pending_permissions().is_empty(),
    )
    .await;

    inspector
        .pending_permissions()
        .pop()
        .expect("the request the wait was for")
}
