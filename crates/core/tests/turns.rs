//! What a turn was, after it is over (`CONTEXT.md`, *Turn*).
//!
//! **The turn state is a position and these are a record.**
//! [`Inspector::turn`](acp_inspector_core::Inspector::turn) says where the live
//! turn stands and is replaced by the next prompt; what these assert is the
//! other half, which the store did not keep until now: which turn each entry
//! arrived in, and what each turn came to. A session of six exchanges is six
//! turns with six outcomes, and not one status about the last of them.
//!
//! **And traffic that belongs to no turn stays that way.** An agent that talks
//! before anything was prompted, or goes on talking after it said the turn was
//! over, is producing exactly what a reader of an inspector is looking for — so
//! it is stamped with no turn rather than tidied into whichever one it fell
//! between.
//!
//! Driven through the public API only (§12), Testy where a conformant agent can
//! produce the case and a scripted `/bin/sh` agent where it cannot.

mod common;

use acp_inspector_core::{AgentCommand, Inspector, TurnOutcome, v1};

use common::{shell, testy, until};

/// Testy's scenarios are prompt-driven: the prompt text is the command.
const ECHO: &str = r#"{"command":"echo","message":"one"}"#;
const AGAIN: &str = r#"{"command":"echo","message":"two"}"#;

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

#[tokio::test]
async fn a_turn_is_recorded_with_what_it_came_to() {
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    assert!(
        inspector.timeline().turns().is_empty(),
        "a session nobody has prompted has had no turns"
    );

    inspector.prompt(ECHO).await.expect("a turn");

    let turns = inspector.timeline().turns();
    assert_eq!(turns.len(), 1, "one prompt is one turn: {turns:?}");
    assert_eq!(turns[0].id, 1, "counting from one");
    assert_eq!(
        turns[0].outcome,
        Some(TurnOutcome::Ended(v1::StopReason::EndTurn)),
        "and it kept the reason the agent gave it"
    );

    // Everything the agent said in it is stamped with it, which is what makes
    // the entries a group rather than a run somebody has to infer.
    let entries = inspector.timeline().entries();
    assert!(!entries.is_empty(), "the turn said something");
    assert!(
        entries.iter().all(|entry| entry.turn == Some(1)),
        "every entry belongs to the turn it arrived in: {entries:?}"
    );
}

#[tokio::test]
async fn two_prompts_are_two_turns_each_keeping_its_own() {
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    inspector.prompt(ECHO).await.expect("the first turn");
    let after_first = inspector.timeline().entries().len();
    inspector.prompt(AGAIN).await.expect("the second turn");

    let turns = inspector.timeline().turns();
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert_eq!([turns[0].id, turns[1].id], [1, 2]);
    assert!(
        turns.iter().all(|turn| turn.outcome.is_some()),
        "both are over, and both say how: {turns:?}"
    );

    // The second turn's entries are the second turn's. The first turn's keep
    // the stamp they were made with — a record of what happened, not a moving
    // window onto the latest thing.
    let entries = inspector.timeline().entries();
    assert!(
        entries[..after_first]
            .iter()
            .all(|entry| entry.turn == Some(1)),
        "{entries:?}"
    );
    assert!(
        entries[after_first..]
            .iter()
            .all(|entry| entry.turn == Some(2)),
        "{entries:?}"
    );
}

/// An agent that opens a session, says something in it before anything is
/// prompted, and then holds the connection open.
fn talks_first() -> AgentCommand {
    shell(concat!(
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}'; "#,
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}' "#,
        r#"'{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"before anybody asked"}}}}'; "#,
        "cat > /dev/null",
    ))
}

#[tokio::test]
async fn what_the_agent_says_before_any_prompt_belongs_to_no_turn() {
    // Which is a fact about the traffic and not a gap in the record: an agent
    // that starts talking on its own is the kind of thing this tool exists to
    // show, and a stamp of *turn one* would be the inspector making it look
    // like an answer to something.
    let inspector = Inspector::new();
    inspector.start(&talks_first()).await.expect("it opens one");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the unprompted line", || {
        !inspector.timeline().is_empty()
    })
    .await;

    let entries = inspector.timeline().entries();
    assert!(
        entries.iter().all(|entry| entry.turn.is_none()),
        "nothing was prompted, so nothing is in a turn: {entries:?}"
    );
    assert!(
        inspector.timeline().turns().is_empty(),
        "and no turn is invented to hold it"
    );
}

/// An agent that ends the turn and then keeps talking in the session it just
/// said it was finished with.
fn talks_after_the_turn() -> AgentCommand {
    shell(concat!(
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}'; "#,
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"in the turn"}}}}' "#,
        r#"'{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}' "#,
        r#"'{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"after the turn"}}}}'; "#,
        "cat > /dev/null",
    ))
}

#[tokio::test]
async fn what_the_agent_says_after_it_ended_the_turn_belongs_to_no_turn_either() {
    // The case worth building all of this for. An agent that goes on streaming
    // into a turn it has resolved is misbehaving in a way that is invisible on
    // a flat list — the rows look exactly like the rows above them — and is
    // obvious the moment the record says which turn each arrived in.
    let inspector = Inspector::new();
    inspector
        .start(&talks_after_the_turn())
        .await
        .expect("it opens one");

    inspector.prompt("go").await.expect("the turn ends");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the line after the turn", || {
        inspector.timeline().len() >= 2
    })
    .await;

    let entries = inspector.timeline().entries();
    assert_eq!(
        entries.first().map(|entry| entry.turn),
        Some(Some(1)),
        "what arrived while the turn was open is the turn's: {entries:?}"
    );
    assert_eq!(
        entries.last().map(|entry| entry.turn),
        Some(None),
        "and what arrived after it ended is nobody's: {entries:?}"
    );

    let turns = inspector.timeline().turns();
    assert_eq!(turns.len(), 1, "the agent talking is not a second turn");
    assert_eq!(
        turns[0].outcome,
        Some(TurnOutcome::Ended(v1::StopReason::EndTurn)),
        "which ended when the agent said it did, whatever it said next"
    );
}

#[tokio::test]
async fn switching_sessions_takes_the_turns_with_the_timeline() {
    // The turns are this session's, and a switch replaces the session the store
    // is a view of (§7.5). What the record of a session that is gone was is in
    // the trace, where every frame behind it still is.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn");
    assert_eq!(inspector.timeline().turns().len(), 1);

    inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");

    assert!(
        inspector.timeline().turns().is_empty(),
        "the new session has had no turns"
    );
}
