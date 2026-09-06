//! Blocking requests, answered where the turn stops for them
//! (`docs/architecture.md` §5, §7.2, §14 step 4): a `session/request_permission`
//! arrives mid-turn, becomes a pending request in the timeline, and goes on
//! being pending until somebody answers it — after which the turn visibly
//! resumes.
//!
//! **The lifecycle is core's, and this is where it is asserted without a
//! window.** Arrival, resolution and the auto-`cancelled` a `session/cancel`
//! owes every waiting request are transitions on the public surface: the
//! timeline's entries, the pending list, and the frames the trace recorded.
//! What the desktop crate adds on top is a panel with four buttons in it.
//!
//! Two agents drive it, for two different reasons. **Testy** is first contact
//! (§12): a real agent, blocking a real turn, whose scenarios decide what the
//! timeline sees rather than the test deciding. **Scripted agents** cover what
//! Testy cannot say — it offers two of the four option kinds and never sends a
//! malformed request — the same way `crates/core/tests/misbehaviour.rs` covers the
//! traffic a conformant agent is incapable of emitting. The gap is recorded on
//! the spec issue (#60), which is what the ticket asked for when it said
//! Testy's coverage is taken on faith.

mod common;

use acp_inspector_core::{
    AgentCommand, CallError, EntryId, EntryKind, Inspector, PermissionRequest, PermissionState,
    TimelineEntry, TurnState, Unrecognized, v1,
};

use common::{accepting_elicitations, blocked, sent, testy, until};

/// Testy's `callbacks` scenario opens with `session/request_permission` and
/// waits for the answer before it goes on to anything else.
const CALLBACKS: &str = r#"{"command":"run_scenario","scenario":"callbacks"}"#;

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
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

/// An agent that answers the handshake, blocks its turn on `params`, and ends
/// the turn once the answer arrives.
///
/// It answers by counting rather than by reading ids, which works because this
/// client numbers its calls from one, in order.
fn blocking_on(params: &str) -> AgentCommand {
    shell(&format!(
        concat!(
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"sessionId":"s1"}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":"ask","method":"session/request_permission","params":{params}}}'; "#,
            // The answer, and then the turn going on: a chunk the timeline can
            // show, and the stop reason that ends it.
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"text","text":"carrying on"}}}}}}}}'; "#,
            r#"printf '%s\n' '{{"jsonrpc":"2.0","id":3,"result":{{"stopReason":"end_turn"}}}}'; "#,
            "cat > /dev/null",
        ),
        params = params
    ))
}

/// An agent that asks and then does not wait: it blocks its turn on `params`
/// and ends the turn anyway, leaving the request outstanding.
///
/// Testy cannot do this — it waits for every callback it sends — and it is the
/// shape that decides what an unanswered request becomes when the turn moves on
/// without it.
fn abandoning(params: &str) -> AgentCommand {
    shell(&format!(
        concat!(
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"sessionId":"s1"}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":"ask","method":"session/request_permission","params":{params}}}' "#,
            r#"'{{"jsonrpc":"2.0","id":3,"result":{{"stopReason":"end_turn"}}}}'; "#,
            "cat > /dev/null",
        ),
        params = params
    ))
}

/// A tool call and all four option kinds — the request the spec describes
/// (§7.2), which no scenario Testy ships actually sends.
const FOUR_OPTIONS: &str = concat!(
    r#"{"sessionId":"s1","toolCall":{"toolCallId":"call-1","title":"Remove the build directory","kind":"delete","status":"pending"},"#,
    r#""options":[{"optionId":"yes","name":"Allow once","kind":"allow_once"},"#,
    r#"{"optionId":"always","name":"Always allow","kind":"allow_always"},"#,
    r#"{"optionId":"no","name":"Reject once","kind":"reject_once"},"#,
    r#"{"optionId":"never","name":"Always reject","kind":"reject_always"}]}"#,
);

/// The permission requests on the timeline, in the order they blocked the turn.
fn requests(inspector: &Inspector) -> Vec<PermissionRequest> {
    inspector
        .timeline()
        .entries()
        .into_iter()
        .filter_map(|entry| match entry.kind {
            EntryKind::Permission(request) => Some(request),
            _ => None,
        })
        .collect()
}

/// Starts a turn without waiting for it, the way the composer does.
fn prompting(inspector: &Inspector, prompt: &'static str) -> tokio::task::JoinHandle<()> {
    tokio::spawn({
        let inspector = inspector.clone();
        async move {
            let _ = inspector.prompt(prompt).await;
        }
    })
}

#[tokio::test]
async fn a_request_blocks_the_turn_where_the_timeline_can_show_it() {
    // First contact with a real agent's permission traffic (§15 q5): Testy
    // stops its turn on `session/request_permission` and does nothing else
    // until it is answered, which is exactly the state the timeline has to be
    // able to render.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, CALLBACKS);
    let request = blocked(&inspector).await;

    // The turn is running and is not going to run any further: a blocking
    // request is the main traffic of an ACP turn, not an alert beside it.
    assert_eq!(inspector.turn(), TurnState::InFlight);
    assert!(request.is_waiting(), "nobody has answered it yet");
    assert_eq!(request.state(), PermissionState::Waiting);

    // What the panel renders: the tool call under question, and the options the
    // agent offered — the agent's own, not a set this client made up.
    assert_eq!(
        request.tool_call().tool_call_id,
        v1::ToolCallId::new("testy-permission-1")
    );
    let kinds: Vec<_> = request.options().iter().map(|option| option.kind).collect();
    assert_eq!(
        kinds,
        [
            v1::PermissionOptionKind::AllowOnce,
            v1::PermissionOptionKind::RejectOnce
        ],
        "Testy offers two of the four kinds; the other two are `four_option_kinds…`'s to cover"
    );

    // And it is on the timeline, at the point the turn stopped for it, keeping
    // the frame it arrived in like every other entry (§8).
    let entries = inspector.timeline().entries();
    let entry = entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::Permission(_)))
        .expect("the request is an entry of its own");
    assert_eq!(entry.id, EntryId::Permission(request.id().clone()));
    assert_eq!(entry.frames.len(), 1);
    assert!(
        entry.frames[0]
            .frame
            .as_str()
            .contains(r#""method":"session/request_permission""#),
        "verbatim, like everything else: {:?}",
        entry.frames[0]
    );

    // Nothing has been answered, so nothing has been sent about it.
    assert!(
        !sent(&inspector)
            .iter()
            .any(|frame| frame.contains("outcome")),
        "an unanswered request is an unanswered request"
    );

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn answering_a_request_lets_the_turn_go_on() {
    // The whole point of the panel: the user chooses, `selected` goes out with
    // that option id, and the agent — which had stopped — carries on.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, CALLBACKS);
    let request = blocked(&inspector).await;
    let blocked_at = inspector.timeline().len();

    let chosen = request.options()[0].option_id.clone();
    request.select(&chosen).await.expect("Testy is listening");

    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(
            v1::SelectedPermissionOutcome::new(chosen.clone())
        )),
        "the request holds what it was answered with"
    );
    assert!(
        inspector.pending_permissions().is_empty(),
        "and the agent is no longer waiting on anything"
    );

    let answer = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains("outcome"))
        .expect("the answer was sent");
    assert!(
        answer.contains(r#""outcome":"selected""#) && answer.contains(r#""optionId":"allow_once""#),
        "with the option the user picked, on the wire: {answer}"
    );

    // The turn resumes, visibly: Testy goes on to the rest of its callbacks,
    // every one of which reaches the timeline.
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the turn to carry on", || {
        inspector.timeline().len() > blocked_at
    })
    .await;

    // And it ends. *How* it ends is Testy's business against this client — it
    // goes on to call `fs/*` and `terminal/*`, both of which are declined
    // (§7.3), and then to five elicitations, which are answered here the way a
    // reader would answer them (§7.8) rather than declined out from under the
    // turn this test is about. What is asserted is that the turn is over rather
    // than which words ended it; that it ends `end_turn` is
    // `crates/core/tests/elicitation.rs`'s to say, where the answers are the subject.
    let answering = accepting_elicitations(&inspector);
    let mut turns = inspector.turn_changes();
    until(&mut turns, "the turn to end", || {
        !inspector.turn().is_running()
    })
    .await;
    answering.abort();
    let _ = turn.await;
}

#[tokio::test]
async fn cancelling_a_turn_answers_every_request_it_left_waiting() {
    // The spec's rule, and the protocol's: a client that cancels a turn MUST
    // answer every pending permission request with `cancelled` (§7.2). Testy
    // then ends the turn the way the conformance point says it must —
    // `stopReason: "cancelled"`, from a turn that was blocked when it was
    // stopped.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, CALLBACKS);
    let request = blocked(&inspector).await;

    inspector.cancel().await.expect("Testy is alive");

    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
        "the pending request was resolved by the cancel, not left hanging"
    );
    assert!(inspector.pending_permissions().is_empty());

    let answer = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains("outcome"))
        .expect("the cancelled answer was sent");
    assert!(
        answer.contains(r#""outcome":"cancelled""#),
        "answered `cancelled`, to its face: {answer}"
    );

    let mut turns = inspector.turn_changes();
    until(&mut turns, "the turn to end as cancelled", || {
        inspector.turn() == TurnState::Ended(v1::StopReason::Cancelled)
    })
    .await;
    let _ = turn.await;
}

#[tokio::test]
async fn all_four_option_kinds_reach_the_timeline_and_the_chosen_one_reaches_the_wire() {
    // The gap Testy leaves (§15 q5): its permission request offers `allow_once`
    // and `reject_once` only, so the four kinds the spec names are covered by a
    // scripted agent — the same way `misbehaviour.rs` covers what a conformant
    // agent cannot say.
    let inspector = Inspector::new();
    inspector
        .start(&blocking_on(FOUR_OPTIONS))
        .await
        .expect("the script starts");

    let turn = prompting(&inspector, "anything");
    let request = blocked(&inspector).await;

    let offered: Vec<_> = request
        .options()
        .iter()
        .map(|option| (option.option_id.to_string(), option.kind))
        .collect();
    assert_eq!(
        offered,
        [
            ("yes".to_owned(), v1::PermissionOptionKind::AllowOnce),
            ("always".to_owned(), v1::PermissionOptionKind::AllowAlways),
            ("no".to_owned(), v1::PermissionOptionKind::RejectOnce),
            ("never".to_owned(), v1::PermissionOptionKind::RejectAlways),
        ],
        "all four kinds, in the order the agent offered them"
    );
    assert_eq!(request.tool_call().fields.kind, Some(v1::ToolKind::Delete));

    request
        .select(&v1::PermissionOptionId::new("never"))
        .await
        .expect("the script is listening");

    let answer = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains("outcome"))
        .expect("the answer was sent");
    assert!(
        answer.contains(r#""optionId":"never""#),
        "the option id the user picked, unchanged: {answer}"
    );

    // The turn resumes and ends, which is what an answered request is for.
    let stop = tokio::time::timeout(common::PATIENCE, turn)
        .await
        .expect("the turn ends once the agent has its answer");
    stop.expect("the prompt task");
    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));
    assert!(
        inspector.timeline().entries().iter().any(|entry| matches!(
            &entry.kind,
            EntryKind::Update(v1::SessionUpdate::AgentMessageChunk(_))
        )),
        "and the timeline shows the resumption"
    );
}

#[tokio::test]
async fn an_answer_the_agent_already_has_is_not_sent_twice() {
    // Two clicks on one panel, or a cancel racing a choice: the agent asked
    // once and gets one answer, because a second response to an id already
    // answered is a frame the protocol has no shape for.
    let inspector = Inspector::new();
    inspector
        .start(&blocking_on(FOUR_OPTIONS))
        .await
        .expect("the script starts");

    let turn = prompting(&inspector, "anything");
    let request = blocked(&inspector).await;

    request
        .select(&v1::PermissionOptionId::new("yes"))
        .await
        .expect("the first answer goes out");
    assert_eq!(
        request.select(&v1::PermissionOptionId::new("no")).await,
        Err(CallError::NotWaiting),
        "and the second is refused rather than sent"
    );

    assert_eq!(
        sent(&inspector)
            .iter()
            .filter(|frame| frame.contains("outcome"))
            .count(),
        1,
        "one request, one answer"
    );
    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Selected(
            v1::SelectedPermissionOutcome::new(v1::PermissionOptionId::new("yes"))
        )),
        "the answer the agent actually got is the one it keeps"
    );

    let _ = tokio::time::timeout(common::PATIENCE, turn).await;
}

#[tokio::test]
async fn a_request_the_turn_ended_without_answering_is_given_up_on() {
    // A blocking request belongs to the turn it blocked (§7.2). An agent that
    // ends its turn while one is outstanding is not waiting on it any more —
    // so the request stops being answerable, and stops reading as a question
    // somebody could still answer. It is *not* an answer: nothing was sent, and
    // the panel says nobody answered it.
    let inspector = Inspector::new();
    inspector
        .start(&abandoning(FOUR_OPTIONS))
        .await
        .expect("the script starts");

    // The turn is driven to its end before anything is read: the request and
    // the stop reason arrive together, so what this is about is the state they
    // leave behind rather than the moment between them.
    let stop = inspector.prompt("anything").await.expect("the turn ran");
    assert_eq!(stop, v1::StopReason::EndTurn);

    let request = requests(&inspector)
        .pop()
        .expect("the request is on the timeline, answered or not");

    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));
    assert_eq!(request.state(), PermissionState::Abandoned);
    assert!(!request.is_waiting());
    assert!(
        inspector.pending_permissions().is_empty(),
        "nobody is waiting on the user any more"
    );
    assert_eq!(
        request.select(&v1::PermissionOptionId::new("yes")).await,
        Err(CallError::NotWaiting),
        "and answering it now is refused rather than sent to an agent that moved on"
    );
    assert!(
        !sent(&inspector)
            .iter()
            .any(|frame| frame.contains("outcome")),
        "a request nobody answered is a request nobody answered"
    );
}

#[tokio::test]
async fn a_request_v1_cannot_read_is_shown_and_refused() {
    // The raw-first rule where it meets a method this client *does* service
    // (§8): a `session/request_permission` v1 cannot decode is an unrecognized
    // entry carrying its own JSON, and the agent is told its parameters were
    // the problem — not that the method does not exist, which would be untrue.
    const MALFORMED: &str = r#"{"sessionId":"s1"}"#;

    let inspector = Inspector::new();
    inspector
        .start(&blocking_on(MALFORMED))
        .await
        .expect("the script starts");

    let turn = prompting(&inspector, "anything");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the malformed request to be shown", || {
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Unrecognized(_)))
    })
    .await;

    let entries: Vec<TimelineEntry> = inspector.timeline().entries();
    let entry = entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::Unrecognized(_)))
        .expect("the entry the wait was for");
    let EntryKind::Unrecognized(Unrecognized::Undecodable { method, problem }) = &entry.kind else {
        panic!("a request v1 could not read is an unrecognized entry: {entry:?}");
    };
    assert_eq!(method.as_deref(), Some("session/request_permission"));
    assert!(!problem.is_empty(), "and it says what could not be read");
    assert!(
        entry.frames[0].frame.as_str().contains(MALFORMED),
        "verbatim: {:?}",
        entry.frames[0]
    );

    assert!(
        requests(&inspector).is_empty(),
        "nothing this client could not read becomes a panel with buttons on it"
    );

    let mut frames = inspector.trace().changes();
    until(&mut frames, "the refusal to be sent", || {
        sent(&inspector).iter().any(|frame| frame.contains("error"))
    })
    .await;
    let refusal = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains("error"))
        .expect("the refusal");
    let refused: serde_json::Value = serde_json::from_str(&refusal).expect("valid JSON-RPC");
    assert_eq!(refused["id"], "ask", "answered to its face: {refusal}");
    assert_eq!(
        refused["error"]["code"], -32602,
        "with invalid params, because the method is one this client services: {refusal}"
    );

    inspector.disconnect();
    let _ = turn.await;
}
