//! The session lifecycle on a live connection (`docs/architecture.md` §7.5):
//! a second session opened without relaunching the agent, and everything that
//! being the *live* session costs the one being left.
//!
//! **Creating a session while one is live is a switch**, and the switch is what
//! these assert: the turn that was running is cancelled, every request waiting
//! on the user is answered `cancelled`, and the timeline is discarded and
//! rebuilt for the session that just opened. The trace is untouched by all of
//! it — a switch costs the view and never the evidence.
//!
//! Driven the way the shell drives it, through the public inspector API only
//! (§12). Testy is the agent wherever a conformant one can produce the case;
//! `/bin/sh` one-liners cover what it cannot, the way `misbehaviour.rs` and
//! `permission.rs` already do.

mod common;

use acp_inspector_core::{AgentCommand, CallError, Inspector, PermissionState, TurnState, v1};

use common::{blocked, position, prompting, running, sent, testy, until};

/// Testy's scenarios are prompt-driven: the prompt text is the command.
const ECHO: &str = r#"{"command":"echo","message":"before the switch"}"#;
const AFTER: &str = r#"{"command":"echo","message":"after the switch"}"#;
/// Accepts the prompt and then waits, so a switch has a live turn to walk away
/// from.
const WAIT_FOR_CANCEL: &str = r#"{"command":"run_scenario","scenario":"wait_for_cancel"}"#;
/// Opens with a `session/request_permission` and waits for the answer.
const CALLBACKS: &str = r#"{"command":"run_scenario","scenario":"callbacks"}"#;

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

#[tokio::test]
async fn a_session_created_on_a_live_connection_becomes_the_live_one() {
    // The affordance the ring is about, driven the way the shell drives it: a
    // second session on the invocation that is already running — no spawn form,
    // no relaunch — and the composer ready to prompt into what was just
    // created.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");

    let second = inspector
        .open_session(&agent(), None)
        .await
        .expect("Testy opens another session");

    assert_ne!(second, first, "a second session, not the first one again");
    assert_eq!(inspector.session(), Some(second.clone()));

    // Ready to prompt into: the turn that follows is the new session's, on the
    // wire and in the store.
    inspector.prompt(ECHO).await.expect("a turn in it");
    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));
}

#[tokio::test]
async fn switching_discards_the_timeline_and_rebuilds_it_for_the_new_session() {
    // The inspector holds one live session and the timeline is its view (§7.5):
    // opening another discards and rebuilds, with no merge, no partition and no
    // second timeline — the same rule the host repository settled for
    // hydration.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");
    assert!(
        !inspector.timeline().is_empty(),
        "the session that is about to be left said something"
    );

    let second = inspector
        .new_session("/tmp", None)
        .await
        .expect("another session");

    assert!(
        inspector.timeline().is_empty(),
        "the view of the session that was left goes with it: {:?}",
        inspector.timeline().entries()
    );

    // Rebuilt, from the new session's own traffic and nothing else.
    inspector
        .prompt(AFTER)
        .await
        .expect("a turn in the new one");
    let entries = inspector.timeline().entries();
    assert!(!entries.is_empty(), "the new session fills it");
    assert!(
        entries
            .iter()
            .flat_map(|entry| entry.frames.iter())
            .all(|recorded| recorded.frame.as_str().contains(&second.to_string())),
        "and every frame on it is the live session's: {entries:?}"
    );
}

#[tokio::test]
async fn creating_a_session_mid_turn_cancels_that_turn_first() {
    // Walking away from a live turn would leave the agent believing a turn is
    // running with a client that stopped listening (§7.5). So the turn is
    // cancelled *before* the switch, and the order is the assertion: the two
    // frames are on the wire in that order.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, WAIT_FOR_CANCEL);
    running(&inspector).await;

    inspector
        .new_session("/tmp", None)
        .await
        .expect("another session");

    let cancelled = position(&inspector, r#""method":"session/cancel""#)
        .expect("the turn was cancelled on the way out");
    let created = sent(&inspector)
        .iter()
        .rposition(|frame| frame.contains(r#""method":"session/new""#))
        .expect("the session was created");
    assert!(
        cancelled < created,
        "the cancel goes out before the create: {:?}",
        sent(&inspector)
    );

    // The agent got what it was owed and ended the turn it was told to end —
    // and that turn is the *left* session's, so the live one is where a session
    // nobody has prompted in stands.
    assert_eq!(
        turn.await.expect("the prompt task"),
        Ok(v1::StopReason::Cancelled)
    );
    assert_eq!(inspector.turn(), TurnState::Idle);
}

#[tokio::test]
async fn switching_answers_every_waiting_permission_request_cancelled() {
    // What a client owes a request it walks away from (§7.2), and the only way
    // an agent blocked on one gets to end its turn. The sequence is
    // `session/cancel`'s, reused rather than written a second time.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, CALLBACKS);
    let request = blocked(&inspector).await;

    inspector
        .new_session("/tmp", None)
        .await
        .expect("another session");

    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
        "answered, not abandoned: the agent was told"
    );
    assert!(
        inspector.pending_permissions().is_empty(),
        "and nothing is waiting on the user in the session that just opened"
    );
    let answer = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains("outcome"))
        .expect("the cancelled answer was sent");
    assert!(
        answer.contains(r#""outcome":"cancelled""#),
        "to its face: {answer}"
    );

    let _ = turn.await;
}

#[tokio::test]
async fn every_frame_from_the_discarded_session_stays_in_the_trace() {
    // The timeline is a view and the trace is the record (§7.5). A switch costs
    // the first and never the second: the frames the discarded session was made
    // of are where they were, in the order they crossed.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");

    let recorded: Vec<_> = inspector
        .trace()
        .entries()
        .into_iter()
        .map(|entry| entry.frame)
        .collect();
    assert!(!recorded.is_empty());

    inspector
        .new_session("/tmp", None)
        .await
        .expect("another session");

    let kept: Vec<_> = inspector
        .trace()
        .entries()
        .into_iter()
        .map(|entry| entry.frame)
        .collect();
    assert_eq!(
        kept[..recorded.len()],
        recorded[..],
        "every frame of the session that was left, unmoved and unrenumbered"
    );
    assert!(
        kept.len() > recorded.len(),
        "with the switch's own frames after them"
    );
    assert!(
        inspector.timeline().is_empty(),
        "the view is what the switch spent"
    );
}

#[tokio::test]
async fn creating_a_session_on_a_connection_that_has_gone_away_says_so() {
    // The affordance is gated on a live connection, and a connection that went
    // away between the render and the click is a failure to report rather than
    // a button that does nothing.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.disconnect();

    assert_eq!(
        inspector.new_session("/tmp", None).await,
        Err(CallError::Disconnected)
    );
    assert_eq!(
        inspector.session(),
        Some(session),
        "and nothing was switched away from: the last session is still the one on screen"
    );
}

#[tokio::test]
async fn the_launch_handshake_still_opens_its_session_the_way_it_did() {
    // The new affordance is a second caller of `session/new`, not a change to
    // the first one (#80): the handshake is `initialize` and `session/new`, in
    // that order and with nothing else in it, because a fresh connection has no
    // live session to leave.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let handshake = sent(&inspector);
    assert_eq!(handshake.len(), 2, "two frames: {handshake:?}");
    assert!(handshake[0].contains(r#""method":"initialize""#));
    assert!(handshake[1].contains(r#""method":"session/new""#));

    // And a relaunch leaves the session the timeline was a view of, so it
    // empties: the rows of a dead agent's session above a live one's, with the
    // composer underneath sending to the live one, are two sessions drawn as
    // one conversation. A `session/new` on the same agent has always cleared
    // for this reason, and a relaunch is the larger break of the two.
    inspector.prompt(ECHO).await.expect("a turn to look at");
    assert!(!inspector.timeline().is_empty());

    inspector.start(&agent()).await.expect("a second agent");
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .all(|entry| entry.turn.is_none()),
        "the last agent's turns went with its session: {:?}",
        inspector.timeline().entries()
    );

    // Nothing that was evidence went with them: the trace is untouched by any
    // of it, and it is where what was said stays readable (§6.2).
    assert!(
        sent(&inspector).len() > 2,
        "both handshakes are still on the wire"
    );
}

/// An agent that says something before anything has opened a session with it.
fn talks_before_any_session() -> AgentCommand {
    shell(concat!(
        r#"printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"before anybody asked"}}}}'; "#,
        "cat > /dev/null",
    ))
}

#[tokio::test]
async fn a_launch_that_never_opened_a_session_leaves_the_rows_it_found() {
    // The rule is that a *view* is emptied when the session it is a view of is
    // left behind — not that a connection empties one. An agent that talked
    // before any session existed (§8), or one whose handshake never got that
    // far, produced rows belonging to no session, and a relaunch after it has
    // nothing to replace them with.
    let inspector = Inspector::new();
    inspector.connect(&talks_before_any_session().factory());
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the agent to talk unprompted", || {
        !inspector.timeline().is_empty()
    })
    .await;
    let unprompted = inspector.timeline().len();

    inspector.connect(&agent().factory());
    assert_eq!(
        inspector.timeline().len(),
        unprompted,
        "no session was left behind, so nothing here was a view of one: {:?}",
        inspector.timeline().entries()
    );
}

/// An agent scripted line by line: one argument per line is the spawn form's
/// rule (`AgentCommand::args`), so the script is one line and separates its
/// steps with `;`.
fn shell(script: &str) -> AgentCommand {
    AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\n{script}"),
        ..AgentCommand::default()
    }
}

/// An agent that answers a turn *after* the client has switched away from it.
///
/// Testy cannot produce this — it ends a cancelled turn where it is asked to —
/// and it is the shape that decides what a turn nobody is listening for any
/// more does to the session that replaced it. The silent reads are the prompt,
/// the `session/cancel` the switch sends, and the `session/new` it switches
/// with; the last read is the `session/list` the test drives afterwards, and it
/// is the starting pistol: the walked-away turn is answered before the listing
/// is, so a listing that has answered is a stale answer this client has already
/// been through. It counts rather than reading ids, which works because this
/// client numbers its calls from one, in order.
const ANSWERS_LATE: &str = concat!(
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
    r#"read _; read _; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{"sessionId":"s2"}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}' "#,
    r#"'{"jsonrpc":"2.0","id":5,"result":{"sessions":[]}}'; "#,
    "cat > /dev/null",
);

#[tokio::test]
async fn a_turn_the_switch_walked_away_from_is_not_the_new_sessions() {
    // The turn state is the *live* session's (§11 seam 2), and after a switch
    // the live session is the one that just opened. An agent that answers the
    // walked-away turn afterwards is answering about a session that is no
    // longer on screen — and the specification's cancellation MUST is decided
    // from frames (§15 q7), which a client that stopped listening has none of.
    // So the answer reaches whoever asked for it, and changes nothing about the
    // session that replaced it.
    let inspector = Inspector::new();
    inspector
        .start(&shell(ANSWERS_LATE))
        .await
        .expect("it answered");

    let turn = prompting(&inspector, "anything");
    running(&inspector).await;

    let second = inspector
        .new_session("/tmp", None)
        .await
        .expect("the switch");
    assert_eq!(second, v1::SessionId::new("s2"));
    assert_eq!(inspector.turn(), TurnState::Idle);

    // The stale answer, and a call after it whose own answer is how this test
    // knows it has been read: the agent answers the walked-away turn first, and
    // one reader takes them in that order (§5).
    inspector
        .list_sessions(None)
        .await
        .expect("the listing after it");
    assert_eq!(
        turn.await.expect("the prompt task"),
        Ok(v1::StopReason::EndTurn),
        "the stop reason still reaches whoever asked for that turn"
    );

    assert_eq!(
        inspector.turn(),
        TurnState::Idle,
        "the session that just opened has had no turn"
    );
    assert!(
        inspector.timeline().is_empty(),
        "and nothing about the one that was left is on its timeline: {:?}",
        inspector.timeline().entries()
    );
}

/// An agent that asks for permission without a turn to ask it in, and then
/// answers the switch.
///
/// A conformant agent cannot do this — a `session/request_permission` belongs
/// to a turn (§7.2) — and it is the case that decides who the `cancelled` is
/// owed to: the requests, or the turn they were supposed to be part of.
///
/// It reads *until* it sees the `session/new` rather than counting lines, so a
/// client that stopped answering the request fails the assertion below instead
/// of leaving this waiting for a line that is not coming.
const ASKS_WITHOUT_A_TURN: &str = concat!(
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}' "#,
    r#"'{"jsonrpc":"2.0","id":"ask","method":"session/request_permission","params":{"sessionId":"s1","toolCall":{"toolCallId":"call-1","title":"Unprompted","kind":"delete","status":"pending"},"options":[{"optionId":"yes","name":"Allow once","kind":"allow_once"}]}}'; "#,
    r#"while read line; do case "$line" in *session/new*) "#,
    r#"printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"sessionId":"s2"}}'; break;; esac; done; "#,
    "cat > /dev/null",
);

/// An agent that opens one session and refuses to open another.
///
/// The capability lie #75 story 26 is about, in its smallest form: an
/// affordance that is offered because the protocol offers it, driven, and
/// refused.
const REFUSES_A_SECOND: &str = concat!(
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":3,"error":{"code":-32603,"message":"one session is enough"}}'; "#,
    "cat > /dev/null",
);

#[tokio::test]
async fn a_request_waiting_outside_a_turn_is_answered_by_the_switch_too() {
    // The debt is to the requests, not to the turn they should have been part
    // of (§7.2): an agent that asked outside a turn is misbehaving, which is
    // this tool's subject matter rather than a reason to tell it nothing (§8).
    // So the answer goes out — and the notification does not, because nothing
    // was running to be asked to stop.
    let inspector = Inspector::new();
    inspector
        .start(&shell(ASKS_WITHOUT_A_TURN))
        .await
        .expect("it answered");

    let request = blocked(&inspector).await;
    assert_eq!(inspector.turn(), TurnState::Idle, "and no turn is running");

    inspector
        .new_session("/tmp", None)
        .await
        .expect("the switch");

    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
        "answered, not abandoned: the agent was told"
    );
    assert!(
        sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""outcome":"cancelled""#)),
        "on the wire: {:?}",
        sent(&inspector)
    );
    assert!(
        position(&inspector, r#""method":"session/cancel""#).is_none(),
        "and nothing was asked to stop, because nothing was running: {:?}",
        sent(&inspector)
    );
}

#[tokio::test]
async fn a_refused_switch_leaves_the_session_it_could_not_replace() {
    // An agent that will not open a second session is offered the affordance
    // anyway and refused it out loud (#75), so what matters is what the refusal
    // leaves: the session that was live is still live, its view is still on
    // screen, and the failure is the caller's answer rather than a silence.
    let inspector = Inspector::new();
    let first = inspector
        .start(&shell(REFUSES_A_SECOND))
        .await
        .expect("it answered");
    assert!(
        inspector.timeline().is_empty(),
        "nothing has happened in it yet"
    );

    let refused = inspector.new_session("/tmp", None).await;

    assert!(
        matches!(refused, Err(CallError::Rejected(_))),
        "the agent's own refusal, whole: {refused:?}"
    );
    assert_eq!(
        inspector.session(),
        Some(first),
        "and the session it could not replace is still the live one"
    );
    assert_eq!(inspector.turn(), TurnState::Idle);
}
