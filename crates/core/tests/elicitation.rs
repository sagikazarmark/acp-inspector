//! Elicitations, answered where the agent asked them (`docs/architecture.md`
//! §7.8): an `elicitation/create` arrives, becomes a pending request in the
//! timeline, and stays pending until somebody accepts, declines or cancels it.
//!
//! **Three answers, and every one is a claim about a reader**, which is what
//! most of this file is about: what goes on the wire when somebody answers, what
//! happens when nobody can, and the two places this client refuses to answer at
//! all because nothing was ever drawn for anybody to answer.
//!
//! Two agents drive it, for the reasons `crates/core/tests/permission.rs` gives.
//! **Testy** is the conformant path: its `elicitations` scenario sends the five
//! shapes the protocol has — a form tied to a tool call, a form declined, a form
//! scoped to a request rather than a session, a URL with the completion that
//! follows it, and a URL scoped to a request — and `callbacks` and `full` are
//! where the turn finally runs to `end_turn`, which no scenario it ships had
//! ever done in this tool. **Scripted agents** cover what a conformant agent
//! will not send: a reused `elicitationId`, a mode nobody advertised, params v1
//! cannot read, a completion for an id nobody issued, an ask naming a session
//! that is not the live one, and an ask the turn walks away from.

mod common;

use acp_inspector_core::{
    AgentCommand, Annotation, CallError, ElicitationAnswer, ElicitationState, EntryId, EntryKind,
    Inspector, Reuse, TurnState, Unrecognized, v1,
};
use serde_json::{Map, Value, json};

use common::{accepting_elicitations, blocked, elicited, received, sent, shell, testy, until};

/// Testy's dedicated scenario: five elicitations, in one order, every time.
const ELICITATIONS: &str = r#"{"command":"run_scenario","scenario":"elicitations"}"#;

/// Its `callbacks` scenario: a permission request, then `fs/*` and `terminal/*`,
/// then the same five.
const CALLBACKS: &str = r#"{"command":"run_scenario","scenario":"callbacks"}"#;

/// And every stable scenario in order, `callbacks` among them — the same ending,
/// reached across the whole surface rather than on its own.
const FULL: &str = r#"{"command":"run_scenario","scenario":"full"}"#;

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

/// The handshake every scripted agent here answers: `initialize`, then
/// `session/new`. It answers by counting reads rather than by matching ids,
/// which works because this client numbers its calls from one, in order.
const HANDSHAKE: &str = concat!(
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
);

/// Waits for whatever this client says next, then asks `params` under `id`.
///
/// The first one in a script consumes the prompt; each one after it consumes the
/// answer to the ask before it, which is what makes a run of them sequential the
/// way a real agent's are.
fn ask(id: &str, params: &str) -> String {
    format!(
        r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":"{id}","method":"elicitation/create","params":{params}}}'; "#
    )
}

/// Waits for the last answer and ends the turn the prompt opened.
fn ends() -> String {
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'; "#
        .to_owned()
}

fn asking(steps: &str) -> AgentCommand {
    shell(&format!("{HANDSHAKE}{steps}cat > /dev/null"))
}

/// A form the agent asks in the live session.
fn form(message: &str) -> String {
    format!(
        r#"{{"sessionId":"s1","mode":"form","message":"{message}","requestedSchema":{{"type":"object","properties":{{"age":{{"type":"integer","minimum":0,"maximum":120}}}},"required":["age"]}}}}"#
    )
}

/// A URL elicitation under `elicitation_id`.
fn url(elicitation_id: &str) -> String {
    format!(
        r#"{{"sessionId":"s1","mode":"url","message":"Sign in","elicitationId":"{elicitation_id}","url":"https://example.com/{elicitation_id}"}}"#
    )
}

/// An `elicitation/complete` for `elicitation_id`, sent without waiting for
/// anything — which is a real ordering: the interaction happens in a browser
/// this tool cannot see, so an agent can know it finished before anybody here
/// has clicked.
fn completes(elicitation_id: &str) -> String {
    format!(
        r#"printf '%s\n' '{{"jsonrpc":"2.0","method":"elicitation/complete","params":{{"elicitationId":"{elicitation_id}"}}}}'; "#
    )
}

/// The same notification, sent after the answer — the other ordering, and the
/// one Testy uses.
fn completes_once_answered(elicitation_id: &str) -> String {
    format!("read _; {}", completes(elicitation_id))
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

/// The answers this client put on the wire, in order.
fn answers(inspector: &Inspector) -> Vec<String> {
    sent(inspector)
        .into_iter()
        .filter(|frame| frame.contains(r#""action""#))
        .collect()
}

/// One field's worth of content, which is all any of these tests sends.
fn content(field: &str, value: Value) -> Map<String, Value> {
    let mut content = Map::new();
    content.insert(field.to_owned(), value);
    content
}

#[tokio::test]
async fn an_elicitation_blocks_where_the_agent_asked_and_carries_the_agents_own_schema() {
    // First contact with a real agent's elicitation traffic: Testy asks a form,
    // ties it to a tool call, and waits — which is exactly the state the panel
    // has to be able to render.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, ELICITATIONS);
    let request = elicited(&inspector).await;

    assert_eq!(inspector.turn(), TurnState::InFlight);
    assert!(request.is_waiting(), "nobody has answered it yet");
    assert_eq!(request.state(), ElicitationState::Waiting);
    assert!(!request.completed(), "and nothing has finished out of band");

    // What the panel renders: the agent's own message, the schema it sent, and
    // the scope — including the tool call it raised the question inside.
    assert_eq!(
        request.message(),
        "Accept the Testy session-scoped form elicitation"
    );
    assert!(
        request.session_id().is_some(),
        "the scope names the session it was asked in: {request:?}"
    );
    assert!(
        request.tool_call_id().is_some(),
        "the scope names the tool call the question came from: {request:?}"
    );
    assert!(request.url().is_none(), "a form is not a URL");

    let schema = request.form().expect("a form elicitation carries a schema");
    let mut properties: Vec<_> = schema.properties.keys().cloned().collect();
    properties.sort();
    assert_eq!(
        properties,
        [
            "age",
            "available_at",
            "birthday",
            "confidence",
            "confirmed",
            "email",
            "homepage",
            "name",
            "priority",
            "tags"
        ],
        "the agent's own schema, decoded rather than summarized"
    );
    assert_eq!(
        schema.required.as_deref(),
        Some(
            ["name", "confidence", "age", "confirmed"]
                .map(str::to_owned)
                .as_slice()
        ),
        "and what it said was required, in the order it said it"
    );

    // It is on the timeline where the agent asked it, keeping the frame it
    // arrived in like every other entry (§8).
    let entries = inspector.timeline().entries();
    let entry = entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::Elicitation(_)))
        .expect("the elicitation is an entry of its own");
    assert_eq!(entry.id, EntryId::Elicitation(request.id().clone()));
    assert_eq!(entry.frames.len(), 1);
    assert!(
        entry.frames[0]
            .frame
            .as_str()
            .contains(r#""method":"elicitation/create""#),
        "verbatim: {:?}",
        entry.frames[0]
    );

    // Nothing has been answered, so nothing has been sent about it.
    assert!(answers(&inspector).is_empty(), "an unanswered ask is one");

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn accepting_sends_the_content_the_reader_produced_and_the_agent_goes_on() {
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, ELICITATIONS);
    let request = elicited(&inspector).await;
    let asked_at = inspector.timeline().len();

    let filled = content("name", json!("Ada"));
    request
        .accept(Some(filled.clone()))
        .await
        .expect("Testy is listening");

    assert_eq!(
        request.state(),
        ElicitationState::Answered(ElicitationAnswer::Accepted(Some(filled))),
        "the request holds what it was answered with"
    );
    assert!(
        inspector.pending_elicitations().is_empty(),
        "and the agent is no longer waiting on it"
    );

    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(
        answer.contains(r#""action":"accept""#) && answer.contains(r#""name":"Ada""#),
        "with the content the reader produced, on the wire: {answer}"
    );

    // The turn goes on: Testy asks the next of its five.
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the agent to ask again", || {
        inspector.timeline().len() > asked_at
    })
    .await;

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn declining_and_cancelling_are_two_different_answers() {
    // The specification asks a client for clear decline *and* cancel controls,
    // because they are different sentences: one is a reader saying no, the other
    // is a reader saying nothing.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}{}",
            ask("first", &form("Say no")),
            ask("second", &form("Say nothing")),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");

    let first = elicited(&inspector).await;
    first.decline().await.expect("the agent is listening");
    assert_eq!(
        first.state(),
        ElicitationState::Answered(ElicitationAnswer::Declined)
    );

    let second = elicited(&inspector).await;
    second.cancel().await.expect("the agent is listening");
    assert_eq!(
        second.state(),
        ElicitationState::Answered(ElicitationAnswer::Cancelled)
    );

    let wire = answers(&inspector);
    assert!(
        wire[0].contains(r#""action":"decline""#) && !wire[0].contains("content"),
        "a decline carries no content: {}",
        wire[0]
    );
    assert!(
        wire[1].contains(r#""action":"cancel""#),
        "and a cancel says so in the agent's own vocabulary: {}",
        wire[1]
    );

    let _ = turn.await;
}

#[tokio::test]
async fn content_the_requested_schema_forbids_is_sent_anyway() {
    // §7.8's rule, and the reason the answer path is raw: the schema asks for an
    // integer between 0 and 120, and a tool that could not send `"old"` could
    // not find out what the agent does with it. Constraints are reported by the
    // panel; nothing here enforces one.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}",
            ask("ask", &form("How old are you")),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    request
        .accept(Some(content("age", json!("old"))))
        .await
        .expect("the agent is listening");

    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(
        answer.contains(r#""age":"old""#),
        "a value the agent's own schema forbids, exactly as the reader wrote it: {answer}"
    );

    let _ = turn.await;
}

#[tokio::test]
async fn a_url_elicitation_carries_its_url_and_is_completed_by_the_agents_notification() {
    // Accepting a URL elicitation is a reader consenting, not an interaction
    // finishing — and the agent saying it finished is a second, separate fact
    // that arrives later and may never arrive at all.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}",
            ask("ask", &url("e1")),
            completes_once_answered("e1")
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    assert_eq!(request.url(), Some("https://example.com/e1"));
    assert_eq!(
        request.elicitation_id(),
        Some(&v1::ElicitationId::new("e1"))
    );
    assert!(request.form().is_none(), "a URL is not a form");
    assert!(!request.completed(), "nothing has finished yet");

    request.accept(None).await.expect("the agent is listening");
    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(
        answer.contains(r#""action":"accept""#) && !answer.contains("content"),
        "consent, and normally no content with it: {answer}"
    );

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the agent to say it completed", || {
        request.completed()
    })
    .await;

    // The completion is not a row of its own: it is a frame the entry it named
    // is now made of (§11 seam 3).
    let entry = inspector
        .timeline()
        .entries()
        .into_iter()
        .find(|entry| entry.id == EntryId::Elicitation(request.id().clone()))
        .expect("the elicitation's entry");
    assert_eq!(
        entry.frames.len(),
        2,
        "the ask and the completion: {entry:?}"
    );
    assert!(
        entry.frames[1]
            .frame
            .as_str()
            .contains(r#""method":"elicitation/complete""#),
        "verbatim, like every frame an entry is made of: {:?}",
        entry.frames[1]
    );

    // The agent said its piece and stopped talking; what this test is about is
    // over, and the turn it happened in is not its subject.
    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn a_completion_that_arrives_before_anybody_answered_still_leaves_the_answer_owed() {
    // Consent and completion are independent, and this is the ordering that
    // proves it (§7.8): the interaction happens somewhere this tool cannot see,
    // so an agent can say it finished while the panel is still waiting for a
    // reader. A model with completion *after* the answer would have to drop one
    // of the two facts here.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}{}",
            ask("ask", &url("e1")),
            completes("e1"),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the agent to say it completed", || {
        request.completed()
    })
    .await;

    assert_eq!(
        request.state(),
        ElicitationState::Waiting,
        "the request is still owed an answer, whatever finished elsewhere"
    );
    assert_eq!(inspector.pending_elicitations(), vec![request.clone()]);

    request.accept(None).await.expect("the agent is listening");
    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(answer.contains(r#""action":"accept""#), "{answer}");
    assert!(
        request.completed(),
        "and it is still completed: {request:?}"
    );

    let _ = turn.await;
}

#[tokio::test]
async fn a_completion_for_an_id_nobody_issued_is_shown_and_acted_on_by_nothing() {
    // "Clients MUST ignore unknown or already-completed ids" — and ignoring is
    // about state, never about the record (§8).
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}{}",
            ask("ask", &url("e1")),
            completes("somebody-elses"),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the stray completion to be shown", || {
        inspector.timeline().entries().iter().any(|entry| {
            matches!(
                entry.kind,
                EntryKind::Unrecognized(Unrecognized::Unsolicited)
            )
        })
    })
    .await;

    assert!(
        !request.completed(),
        "and the elicitation it did not name is untouched"
    );
    assert!(request.is_waiting(), "still waiting on a reader");

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn a_second_elicitation_under_an_outstanding_id_draws_the_annotation() {
    // The one MUST this ring adds, decided from two frames: the create that
    // minted the id and the create that reused it while the first was still
    // outstanding.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}{}",
            ask("first", &url("e1")),
            ask("second", &url("e1")),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let first = elicited(&inspector).await;
    // Accepted, and *not* completed, which is what keeps its id outstanding:
    // consent is not completion, and only the agent's own notification ends
    // one. Refusing it instead would free the id — which is
    // `an_id_the_reader_refused_is_free_for_the_agent_to_use_again`.
    first.accept(None).await.expect("the agent is listening");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the reuse to be annotated", || {
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Annotation(_)))
    })
    .await;

    let entries = inspector.timeline().entries();
    let entry = entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::Annotation(_)))
        .expect("the annotation");
    let EntryKind::Annotation(annotation) = &entry.kind else {
        unreachable!("just matched")
    };
    assert_eq!(
        annotation,
        &Annotation::ElicitationId(Reuse::Outstanding(v1::ElicitationId::new("e1")))
    );
    assert_eq!(
        entry.frames.len(),
        2,
        "read beside both frames that decide it: {entry:?}"
    );

    // And both asks are still there to be answered: the tool reports the rule
    // and takes nothing away.
    assert_eq!(
        inspector
            .timeline()
            .entries()
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::Elicitation(_)))
            .count(),
        2
    );

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn an_id_the_reader_refused_is_free_for_the_agent_to_use_again() {
    // *Outstanding* is the rule's own word and it is not a synonym for
    // unanswered (§7.8). A declined URL elicitation is an interaction that never
    // happened, so the agent may mint the id again — and a client still holding
    // it would report a MUST that was not broken, which is the one thing this
    // tool must never do (§15 q7).
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}{}",
            ask("first", &url("e1")),
            ask("second", &url("e1")),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let first = elicited(&inspector).await;
    first.decline().await.expect("the agent is listening");

    let second = elicited(&inspector).await;
    assert_eq!(second.elicitation_id(), Some(&v1::ElicitationId::new("e1")));

    // Long enough for an annotation to have been drawn, if one were going to be.
    let mut changes = inspector.timeline().changes();
    let _ = tokio::time::timeout(
        common::QUIET,
        until(&mut changes, "nothing in particular", || false),
    )
    .await;
    assert!(
        !inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Annotation(_))),
        "a refused elicitation left the id free: {:?}",
        inspector.timeline().entries()
    );

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn a_mode_this_client_never_advertised_is_refused_rather_than_answered() {
    // It decodes — the schema keeps an escape hatch for modes it has no name for
    // — so this is a decision. Accept would claim an interaction happened, and
    // decline and cancel are both claims about a reader, so the only honest
    // answer is the error the specification names (§7.8).
    let inspector = Inspector::new();
    inspector
        .start(&asking(&ask(
            "ask",
            r#"{"sessionId":"s1","mode":"_holographic","message":"Wave at it"}"#,
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the unadvertised mode to be shown", || {
        inspector.timeline().entries().iter().any(|entry| {
            matches!(
                entry.kind,
                EntryKind::Unrecognized(Unrecognized::Unadvertised { .. })
            )
        })
    })
    .await;

    let entries = inspector.timeline().entries();
    let entry = entries
        .iter()
        .find(|entry| {
            matches!(
                entry.kind,
                EntryKind::Unrecognized(Unrecognized::Unadvertised { .. })
            )
        })
        .expect("the ask is an entry of its own");
    let EntryKind::Unrecognized(Unrecognized::Unadvertised { method, detail }) = &entry.kind else {
        unreachable!("just matched")
    };
    assert_eq!(method, "elicitation/create");
    assert!(
        detail.contains("_holographic"),
        "named in the agent's own spelling: {detail}"
    );

    assert!(
        inspector.pending_elicitations().is_empty(),
        "nothing was drawn, so nothing is waiting on a reader"
    );

    until(&mut changes, "the refusal to be sent", || {
        !answers(&inspector).is_empty()
            || sent(&inspector)
                .iter()
                .any(|frame| frame.contains("-32602"))
    })
    .await;

    let refusal = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""id":"ask""#))
        .expect("it was answered to its face");
    let refused: Value = serde_json::from_str(&refusal).expect("valid JSON-RPC");
    assert_eq!(
        refused["error"]["code"], -32602,
        "invalid params, which is the error the specification names: {refusal}"
    );
    assert!(
        !refusal.contains(r#""action""#),
        "and never an answer about what a reader did: {refusal}"
    );

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn params_v1_cannot_read_are_shown_and_refused() {
    // A method this client *does* service, carrying something it cannot read:
    // invalid params rather than method-not-found, because the method exists
    // here and saying otherwise would be a lie about this client.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&ask("ask", r#"{"sessionId":"s1"}"#)))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the unreadable ask to be shown", || {
        inspector.timeline().entries().iter().any(|entry| {
            matches!(
                entry.kind,
                EntryKind::Unrecognized(Unrecognized::Undecodable { .. })
            )
        })
    })
    .await;

    until(&mut changes, "the refusal to be sent", || {
        sent(&inspector).iter().any(|frame| frame.contains("error"))
    })
    .await;

    let refusal = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""id":"ask""#))
        .expect("it was answered to its face");
    let refused: Value = serde_json::from_str(&refusal).expect("valid JSON-RPC");
    assert_eq!(refused["error"]["code"], -32602, "{refusal}");

    inspector.disconnect();
    let _ = turn.await;
}

#[tokio::test]
async fn cancelling_a_turn_cancels_the_elicitations_it_left_waiting() {
    // The debt a `session/cancel` owes, which is the permission request's rule
    // (§7.2) and the same one here: an agent left blocked on a question the
    // reader has withdrawn is an agent nobody will ever answer.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}",
            ask("ask", &form("Fill me in")),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    inspector.cancel().await.expect("the turn is cancelled");

    assert_eq!(
        request.state(),
        ElicitationState::Answered(ElicitationAnswer::Cancelled),
        "answered, not abandoned: the agent is told"
    );
    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(answer.contains(r#""action":"cancel""#), "{answer}");

    let _ = turn.await;
}

#[tokio::test]
async fn a_request_scoped_elicitation_outlives_the_turn_a_session_scoped_one_dies_with() {
    // Scope decides the lifetime (§7.8). The session-scoped one belongs to the
    // turn it blocked; the request-scoped one was never tied to a conversation,
    // its row survives the turn boundary like every row, and the agent may well
    // still be waiting on it.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            concat!(
                "{}",
                // Both asks and the turn's end, without waiting for an answer to
                // either: Testy waits for every callback it sends, and this is
                // the shape that decides what an unanswered ask becomes.
                r#"printf '%s\n' '{{"jsonrpc":"2.0","id":"scoped","method":"elicitation/create","params":{}}}' "#,
                r#"'{{"jsonrpc":"2.0","id":3,"result":{{"stopReason":"end_turn"}}}}'; "#,
            ),
            ask("loose", r#"{"requestId":"outside","mode":"form","message":"Log in first","requestedSchema":{"type":"object","properties":{"token":{"type":"string"}}}}"#),
            form("Fill me in"),
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");

    // The request-scoped one first, then the session-scoped one and the end of
    // the turn behind it.
    let loose = elicited(&inspector).await;
    assert_eq!(loose.session_id(), None, "it names a call, not a session");
    assert_eq!(
        loose.request_scope(),
        Some(&v1::RequestId::Str("outside".to_owned()))
    );

    let mut turns = inspector.turn_changes();
    until(&mut turns, "the turn to end", || {
        !inspector.turn().is_running()
    })
    .await;

    let scoped = inspector
        .timeline()
        .entries()
        .into_iter()
        .filter_map(|entry| match entry.kind {
            EntryKind::Elicitation(request) if request.session_id().is_some() => Some(request),
            _ => None,
        })
        .next()
        .expect("the session-scoped ask is on the timeline");

    assert_eq!(
        scoped.state(),
        ElicitationState::Abandoned,
        "the turn ended without answering it, and nobody can now"
    );
    assert_eq!(
        loose.state(),
        ElicitationState::Waiting,
        "and the one the turn never owned is still answerable"
    );
    assert_eq!(
        inspector.pending_elicitations(),
        vec![loose.clone()],
        "which is what the list says too"
    );

    // Nothing was sent about either: an abandoned ask is not an answer, and the
    // loose one has not been answered yet.
    assert!(answers(&inspector).is_empty());

    let _ = turn.await;
}

#[tokio::test]
async fn leaving_the_session_answers_everything_including_what_the_turn_did_not_own() {
    // The screen decides who can pay the debt (§7.8): a session switch discards
    // the timeline whole, so an elicitation left waiting would be a debt with
    // nowhere to answer it from — request-scoped included.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&ask(
            "loose",
            r#"{"requestId":"outside","mode":"form","message":"Log in first","requestedSchema":{"type":"object"}}"#,
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let loose = elicited(&inspector).await;

    // Opening another session is what pays the debt, and the debt is paid on the
    // way out rather than when the agent answers — so this watches the request
    // rather than the call, which this scripted agent never answers.
    let switching = tokio::spawn({
        let inspector = inspector.clone();
        async move {
            let _ = inspector
                .new_session(std::path::PathBuf::from("/tmp"), None)
                .await;
        }
    });
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the debt to be paid on the way out", || {
        !loose.is_waiting()
    })
    .await;

    assert_eq!(
        loose.state(),
        ElicitationState::Answered(ElicitationAnswer::Cancelled),
        "answered on the way out rather than left standing"
    );
    assert!(inspector.pending_elicitations().is_empty());
    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(answer.contains(r#""action":"cancel""#), "{answer}");

    inspector.disconnect();
    switching.abort();
    let _ = turn.await;
}

#[tokio::test]
async fn an_answer_the_agent_already_has_is_not_sent_twice() {
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}",
            ask("ask", &form("Fill me in")),
            ends()
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    request.decline().await.expect("the agent is listening");
    assert!(
        matches!(request.cancel().await, Err(CallError::NotWaiting)),
        "a second answer is refused by the request itself"
    );
    assert_eq!(answers(&inspector).len(), 1, "and one frame went out");

    let _ = turn.await;
}

#[tokio::test]
async fn the_agent_left_holding_one_when_the_connection_goes_is_given_up_on() {
    let inspector = Inspector::new();
    inspector
        .start(&asking(&ask("ask", &form("Fill me in"))))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");
    let request = elicited(&inspector).await;

    inspector.disconnect();

    assert_eq!(
        request.state(),
        ElicitationState::Abandoned,
        "nobody was told anything, and nobody can be now"
    );
    assert!(inspector.pending_elicitations().is_empty());
    assert!(
        answers(&inspector).is_empty(),
        "abandoning is not an answer: nothing went into a closed pipe"
    );

    let _ = turn.await;
}

#[tokio::test]
async fn an_elicitation_can_arrive_before_there_is_a_session_to_ask_in() {
    // The case the request scope exists for (§7.8): an agent asking something
    // during the handshake, when there is no conversation to ask inside. The
    // timeline draws it because the timeline is where blocking requests are
    // answered — not only where a session's updates are — and answering it is
    // the alternative to this client saying, on nobody's behalf, that a reader
    // dismissed a question it never drew.
    let inspector = Inspector::new();
    inspector.connect(
        &shell(concat!(
            r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}' "#,
            r#"'{"jsonrpc":"2.0","id":"early","method":"elicitation/create","params":{"requestId":"handshake","mode":"form","message":"Log in first","requestedSchema":{"type":"object"}}}'; "#,
            "cat > /dev/null",
        ))
        .factory(),
    );
    inspector.initialize().await.expect("the agent initializes");

    let request = elicited(&inspector).await;
    assert_eq!(request.session_id(), None);
    assert!(
        inspector.session().is_none(),
        "and there is no live session for it to have been asked in"
    );
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Elicitation(_))),
        "it is drawn where it would be answered"
    );

    request.decline().await.expect("the agent is listening");
    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(answer.contains(r#""action":"decline""#), "{answer}");
}

#[tokio::test]
async fn an_elicitation_naming_a_session_that_is_not_the_live_one_is_drawn_like_any_other() {
    // **Neither scope is filtered against the live session's id** (§7.8). A
    // permission request has carried one since the second ring and this client
    // has never checked it, and a filter here would be the tool deciding which
    // of its subject's traffic is legitimate — on the one screen where deciding
    // wrong means answering, or refusing to, for a reader who was never shown
    // the question.
    let inspector = Inspector::new();
    inspector
        .start(&asking(&format!(
            "{}{}",
            ask(
                "stray",
                r#"{"sessionId":"s2","mode":"form","message":"About the other one","requestedSchema":{"type":"object"}}"#,
            ),
            ends(),
        )))
        .await
        .expect("the scripted agent starts");

    let turn = prompting(&inspector, "ask me");

    let request = elicited(&inspector).await;
    assert_eq!(
        request.session_id().map(ToString::to_string),
        Some("s2".to_owned()),
        "it names the session the agent named"
    );
    assert_ne!(
        request.session_id().cloned(),
        inspector.session(),
        "which is not the one this client has open"
    );
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Elicitation(_))),
        "and it is drawn where every other one is"
    );

    request
        .accept(Some(content("age", json!(7))))
        .await
        .expect("the agent is listening");
    let answer = answers(&inspector).pop().expect("the answer was sent");
    assert!(answer.contains(r#""action":"accept""#), "{answer}");

    let _ = turn.await;
}

#[tokio::test]
async fn testys_callbacks_scenario_runs_to_end_turn() {
    // The ring's end-to-end proof, and the sentence the README could not say
    // before it: `callbacks` blocks on a permission request, calls `fs/*` and
    // `terminal/*` — still declined (§7.3) — and then asks five elicitations,
    // including two in URL mode that it will not even attempt against a client
    // that has not claimed the capability. Answer them and the turn ends the way
    // the agent meant it to.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, CALLBACKS);

    let permission = blocked(&inspector).await;
    let chosen = permission.options()[0].option_id.clone();
    permission
        .select(&chosen)
        .await
        .expect("Testy is listening");

    let answering = accepting_elicitations(&inspector);
    let mut turns = inspector.turn_changes();
    until(&mut turns, "the turn to end", || {
        !inspector.turn().is_running()
    })
    .await;
    answering.abort();

    assert_eq!(
        inspector.turn(),
        TurnState::Ended(v1::StopReason::EndTurn),
        "the agent's own stop reason, on a scenario no client here had ever finished"
    );

    // And every one of the five was answered on the wire, in URL mode too.
    assert_eq!(answers(&inspector).len(), 5);
    assert!(
        received(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""method":"elicitation/complete""#)),
        "including the completion Testy sends for the URL it accepted"
    );

    let _ = turn.await;
}

#[tokio::test]
async fn testys_full_scenario_runs_to_end_turn() {
    // The other half of the same proof (§7.8), and the one that says the ring
    // did not fix `callbacks` in isolation: `full` runs every stable scenario in
    // order — updates, content, tool calls, the declined `terminal/*` among them
    // — and then the callbacks and the five elicitations, so the turn it ends is
    // one that crossed the whole surface on its way there.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let turn = prompting(&inspector, FULL);

    let permission = blocked(&inspector).await;
    let chosen = permission.options()[0].option_id.clone();
    permission
        .select(&chosen)
        .await
        .expect("Testy is listening");

    let answering = accepting_elicitations(&inspector);
    let mut turns = inspector.turn_changes();
    until(&mut turns, "the turn to end", || {
        !inspector.turn().is_running()
    })
    .await;
    answering.abort();

    assert_eq!(
        inspector.turn(),
        TurnState::Ended(v1::StopReason::EndTurn),
        "the agent's own stop reason, on the scenario that runs all the others"
    );
    assert_eq!(answers(&inspector).len(), 5, "the same five, answered");

    let _ = turn.await;
}
