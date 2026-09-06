//! Opening a session the agent already has (`docs/architecture.md` §7.5):
//! `session/load` and `session/resume`, which are one operation with a replay
//! property rather than two unrelated calls.
//!
//! **The property is what these are about.** The two requests are
//! field-for-field identical and the only observable difference is an ordering:
//! load replays the conversation as `session/update` notifications before it
//! answers, resume does not replay it at all. So what is asserted here is which
//! call an agent's advertisement entitles the inspector to make, that load wins
//! where both are offered, and that a session opened either way is the live one
//! — with everything a switch costs (§7.5) costing exactly what creating a
//! session costs, because it is the same switch.
//!
//! Driven through the public inspector API only, the way the shell drives it
//! (§12). Testy is the agent wherever it can produce the case — it advertises
//! both and implements both, which the first test here pins — and `/bin/sh`
//! one-liners cover what it cannot: an agent that genuinely replays on load,
//! and an agent whose advertisement is a lie.

mod common;

use acp_inspector_core::{
    AgentCommand, CallError, Inspector, PermissionState, Restore, TurnState, v1,
};

use common::{
    advertising, blocked, listed, position, prompting, running, said_in, sent, spoken_in, testy,
};

/// Testy's scenarios are prompt-driven: the prompt text is the command.
const ECHO: &str = r#"{"command":"echo","message":"in the session that was left"}"#;
const AGAIN: &str = r#"{"command":"echo","message":"in the session that was opened"}"#;
/// Accepts the prompt and then waits, so opening a session has a live turn to
/// walk away from.
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
async fn testy_implements_both_of_the_calls_it_advertises() {
    // **A finding, driven rather than read** (§15 q5). #81 expected Testy to
    // advertise `session/resume` without implementing it — a free fixture for
    // the rule that affordances gate on the advertisement and not on the tool's
    // opinion of state. It does not: rust-sdk `main` answers both, so the lying
    // advertisement here is a scripted one
    // (`an_agent_that_advertised_resume_without_implementing_it_is_still_offered_it`)
    // and Testy is the conformant fixture for both calls instead.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let info = listed(&inspector, &session).await;

    assert_eq!(
        inspector.restore_session(&info, Restore::Load, None).await,
        Ok(session.clone()),
        "it answers `session/load`"
    );
    assert_eq!(
        inspector
            .restore_session(&info, Restore::Resume, None)
            .await,
        Ok(session),
        "and it answers `session/resume`, which it also advertises"
    );
}

#[tokio::test]
async fn a_listed_session_can_be_opened_and_becomes_the_live_one() {
    // The affordance the ticket is about: a session in the listing stops being
    // text you can read and becomes a way into that session. Opened from what
    // the listing said about it, and live enough to prompt into afterwards.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn in the first");

    let second = inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");
    assert_eq!(inspector.session(), Some(second.clone()));

    let info = listed(&inspector, &first).await;
    let opened = inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("Testy opens the session it listed");

    assert_eq!(opened, first, "the session that was asked for");
    assert_eq!(inspector.session(), Some(first.clone()));

    // The composer prompts into what was just opened, which is the whole of
    // what "the live session" means here.
    inspector.prompt(AGAIN).await.expect("a turn in it");
    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));
    assert!(
        sent(&inspector)
            .iter()
            .rev()
            .find(|frame| frame.contains(r#""method":"session/prompt""#))
            .is_some_and(|frame| frame.contains(&first.to_string())),
        "and the turn is the opened session's: {:?}",
        sent(&inspector)
    );
}

#[tokio::test]
async fn opening_a_session_the_agent_has_already_opened_is_permitted() {
    // Nothing here models what the agent has open (§7.5's gating rule): the
    // affordance is offered because the agent advertised the method, the live
    // session is opened again because that is what was asked, and whatever the
    // agent does with it is reported without comment.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let info = listed(&inspector, &session).await;

    let reopened = inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("Testy loads the session it already has open");

    assert_eq!(reopened, session);
    assert_eq!(inspector.session(), Some(session));
    assert_eq!(inspector.turn(), TurnState::Idle);
}

#[tokio::test]
async fn opening_a_session_mid_turn_cancels_that_turn_first() {
    // The switch #80 settled, reused rather than restated: walking away from a
    // live turn would leave the agent believing a turn is running with a client
    // that stopped listening. The order is the assertion — the two frames are
    // on the wire in it.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let info = listed(&inspector, &session).await;

    let turn = prompting(&inspector, WAIT_FOR_CANCEL);
    running(&inspector).await;

    inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("the session opens");

    let cancelled = position(&inspector, r#""method":"session/cancel""#)
        .expect("the turn was cancelled on the way out");
    let loaded =
        position(&inspector, r#""method":"session/load""#).expect("the session was loaded");
    assert!(
        cancelled < loaded,
        "the cancel goes out before the load: {:?}",
        sent(&inspector)
    );

    assert_eq!(
        turn.await.expect("the prompt task"),
        Ok(v1::StopReason::Cancelled),
        "and the agent got what it was owed"
    );
    assert_eq!(
        inspector.turn(),
        TurnState::Idle,
        "the session that just opened has had no turn"
    );
}

#[tokio::test]
async fn opening_a_session_answers_every_waiting_permission_request_cancelled() {
    // What a client owes a request it walks away from (§7.2), and the only way
    // an agent blocked on one gets to end its turn.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let info = listed(&inspector, &session).await;

    let turn = prompting(&inspector, CALLBACKS);
    let request = blocked(&inspector).await;

    inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("the session opens");

    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
        "answered, not abandoned: the agent was told"
    );
    assert!(
        inspector.pending_permissions().is_empty(),
        "and nothing is waiting on the user in the session that just opened"
    );

    let _ = turn.await;
}

#[tokio::test]
async fn the_trace_keeps_every_frame_from_the_session_that_was_switched_away_from() {
    // The timeline is a view and the trace is the record (§7.5). Opening a
    // session costs the first and never the second.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");
    let info = listed(&inspector, &session).await;

    let recorded: Vec<_> = inspector
        .trace()
        .entries()
        .into_iter()
        .map(|entry| entry.frame)
        .collect();
    assert!(!recorded.is_empty());

    inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("the session opens");

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
}

/// What the agent said about `s1`, as the listing would have described it.
///
/// The scripted agents here do not list — a listing is a capability of its own
/// (§7.5) and these are about the two that reopen — so the session info the
/// screen would have clicked is stated instead.
fn s1() -> v1::SessionInfo {
    v1::SessionInfo::new("s1", "/tmp")
}

#[tokio::test]
async fn where_both_are_advertised_load_is_preferred() {
    // The policy a shipped ACP client already uses, and the one the protocol's
    // own direction points at: load is the call that can rebuild the
    // conversation, so it is the one offered wherever it is on offer.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    let described = inspector.initialize().await.expect("Testy initializes");

    assert!(
        described.agent_capabilities.load_session
            && described
                .agent_capabilities
                .session_capabilities
                .resume
                .is_some(),
        "Testy advertises both, which is what makes it the fixture for this"
    );
    assert_eq!(inspector.advertised_restore(), Some(Restore::Load));
}

#[tokio::test]
async fn an_agent_that_advertises_only_resume_offers_resume() {
    // The other half of the preference, and the case the screen has something
    // to say about: this agent can reopen the session and cannot show what was
    // said in it. Core answers *which call*; saying so in words is the
    // window's, beside the button (`crates/app/src/agent.rs`).
    let inspector = Inspector::new();
    inspector.connect(&advertising(r#"{"sessionCapabilities":{"resume":{}}}"#, &[]).factory());
    assert_eq!(
        inspector.advertised_restore(),
        None,
        "nothing is advertised until `initialize` has answered"
    );

    inspector.initialize().await.expect("it answered");

    assert_eq!(inspector.advertised_restore(), Some(Restore::Resume));
    assert!(
        !Restore::Resume.replays(),
        "and resume is the one that does not replay, which is the whole difference"
    );
}

#[tokio::test]
async fn an_agent_that_advertises_neither_offers_no_way_in() {
    // Gated on the advertisement, like every affordance here: an agent that
    // claimed no way to reopen a session is offered none, and its listing stays
    // the text it always was.
    let inspector = Inspector::new();
    inspector.connect(&advertising("{}", &[]).factory());
    inspector.initialize().await.expect("it answered");

    assert_eq!(inspector.advertised_restore(), None);
}

/// The answer a load or a resume gets when it gets one.
const OPENED: &str = r#"'{"jsonrpc":"2.0","id":3,"result":{}}'"#;

#[tokio::test]
async fn a_load_replays_the_conversation_into_the_timeline_before_it_answers() {
    // The property the screen is built around (§7.5), asserted where it is
    // observable: by the time the call answers, the conversation is *in* the
    // timeline. Which is also why the view is discarded before the ask and not
    // after it — a timeline emptied afterwards would be emptied of this, and
    // the line the session said before the load is what proves it was emptied
    // at all.
    //
    // Testy cannot produce this: it answers a load having replayed nothing,
    // which is the finding `conformance.rs` pins. So the conformant ordering is
    // scripted — the two updates and the answer in one write, so the order on
    // the wire is the order this asserts.
    let replay = format!(
        "{}{}{OPENED}",
        said_in("s1", "what was said before"),
        said_in("s1", "and what was answered"),
    );
    let inspector = Inspector::new();
    inspector.connect(&advertising(r#"{"loadSession":true}"#, &[&replay]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    spoken_in(&inspector).await;

    inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect("it loaded the session");

    let said: Vec<_> = inspector
        .timeline()
        .entries()
        .into_iter()
        .flat_map(|entry| entry.frames)
        .map(|recorded| recorded.frame.as_str().to_owned())
        .collect();
    assert_eq!(said.len(), 2, "the replay, and only the replay: {said:?}");
    assert!(said[0].contains("what was said before"), "{said:?}");
    assert!(said[1].contains("and what was answered"), "{said:?}");
}

#[tokio::test]
async fn a_resume_opens_the_session_without_replaying_anything() {
    // Which is not a failure and not a silence to be explained away: resume
    // restores the session and says nothing about what was in it, so an empty
    // timeline is the correct rendering of a resumed session.
    let inspector = Inspector::new();
    inspector
        .connect(&advertising(r#"{"sessionCapabilities":{"resume":{}}}"#, &[OPENED]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    spoken_in(&inspector).await;

    inspector
        .restore_session(&s1(), Restore::Resume, None)
        .await
        .expect("it resumed the session");

    let asked = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""method":"session/resume""#))
        .expect("resume is what was sent, not load");
    assert!(
        asked.contains(r#""sessionId":"s1""#) && asked.contains(r#""cwd":"/tmp""#),
        "carrying the session the listing described: {asked}"
    );
    assert!(
        inspector.timeline().is_empty(),
        "the view of the session that was left goes with it, and nothing replaced it: {:?}",
        inspector.timeline().entries()
    );
}

#[tokio::test]
async fn an_agent_that_advertised_resume_without_implementing_it_is_still_offered_it() {
    // #75's capability lie, in the shape this ring meets it: the affordance is
    // gated on the advertisement and never on the inspector's opinion of what
    // the agent can really do — so the call goes out, and the refusal is the
    // finding. Evidence first-class either way: the answer is the caller's, the
    // frames are in the trace, and the session that could not be replaced is
    // still the live one.
    let inspector = Inspector::new();
    inspector.connect(
        &advertising(
            r#"{"sessionCapabilities":{"resume":{}}}"#,
            &[r#"'{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"Method not found"}}'"#],
        )
        .factory(),
    );
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    assert_eq!(
        inspector.advertised_restore(),
        Some(Restore::Resume),
        "it said it resumes sessions, which is the whole of what the affordance gates on"
    );
    let refused = inspector
        .restore_session(&s1(), Restore::Resume, None)
        .await;

    assert!(
        matches!(refused, Err(CallError::Rejected(_))),
        "the agent's own refusal, whole: {refused:?}"
    );
    assert_eq!(
        inspector.session(),
        Some(live),
        "and the session it could not replace is still the live one"
    );
    assert!(
        sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""method":"session/resume""#)),
        "the call it refused is in the trace: {:?}",
        sent(&inspector)
    );
    assert!(
        inspector
            .trace()
            .entries()
            .iter()
            .any(|entry| entry.frame.as_str().contains("Method not found")),
        "and so is the refusal itself"
    );
}

#[tokio::test]
async fn opening_a_session_on_a_connection_that_has_gone_away_says_so() {
    // The affordance needs an agent to answer it, and a connection that went
    // away between the render and the click is a failure to report rather than
    // a button that does nothing.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");
    let info = listed(&inspector, &session).await;
    inspector.disconnect();

    assert_eq!(
        inspector.restore_session(&info, Restore::Load, None).await,
        Err(CallError::Disconnected)
    );
    assert_eq!(
        inspector.session(),
        Some(session),
        "and nothing was switched away from"
    );
    assert!(
        !inspector.timeline().is_empty(),
        "including the view: a switch that could never be asked for spends nothing, and what the \
         agent said is still readable after it has gone (§5)"
    );
}
