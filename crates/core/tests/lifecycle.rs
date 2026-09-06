//! Closing and deleting a session (`docs/architecture.md` §7.5): the end of the
//! lifecycle ring, and the corners the specification leaves undefined.
//!
//! Driven through the public inspector API only, the way the shell drives it
//! (§12). Testy is the agent wherever it can produce the case — it advertises
//! close and delete and answers both — and `/bin/sh` one-liners cover what it
//! cannot.

mod common;

use acp_inspector_core::{
    AgentCommand, Annotation, CallError, EntryKind, Inspector, PermissionState, TurnState, v1,
};

use common::{
    advertising, blocked, prompting, received, running, said_at, said_in, sent, shell, spoken_in,
    testy,
};

/// Testy's scenarios are prompt-driven: the prompt text is the command.
const ECHO: &str = r#"{"command":"echo","message":"in the session that was closed"}"#;

/// Whether the agent's last listing named this session.
fn listed(inspector: &Inspector, session: &v1::SessionId) -> bool {
    inspector
        .listing()
        .expect("the agent has been asked for its sessions")
        .sessions
        .iter()
        .any(|info| &info.session_id == session)
}

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

#[tokio::test]
async fn a_listed_session_can_be_closed() {
    // The affordance this ticket is about: a session in the listing can be
    // closed, and what the agent answers is the answer.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");
    let second = inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");

    assert_eq!(inspector.close_session(&first).await, Ok(()));
    assert_eq!(
        inspector.session(),
        Some(second),
        "the session that was live is untouched: it is not the one that was closed"
    );
}

#[tokio::test]
async fn a_listed_session_can_be_deleted() {
    // The other half of the affordance, and the thing worth watching about it:
    // whether the session then leaves a subsequent listing is the agent's
    // answer, not the inspector's.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");
    inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");
    inspector.list_sessions(None).await.expect("Testy lists");
    assert!(
        listed(&inspector, &first),
        "the session it is about to delete"
    );

    assert_eq!(inspector.delete_session(&first).await, Ok(()));

    inspector.list_sessions(None).await.expect("Testy lists");
    assert!(
        !listed(&inspector, &first),
        "Testy's own answer: a deleted session leaves its listing"
    );
}

#[tokio::test]
async fn closing_the_live_session_leaves_the_inspector_with_no_live_session() {
    // Permitted, and the specification says nothing about it (§7.5): the
    // affordance gates on the advertisement, so the session the inspector is
    // sitting in can be closed like any other. What is left has to read as
    // itself — no session open — rather than as an empty one.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");
    assert!(!inspector.timeline().is_empty());

    assert_eq!(inspector.close_session(&session).await, Ok(()));

    assert_eq!(inspector.session(), None, "there is no live session");
    assert!(
        inspector.timeline().is_empty(),
        "and nothing on screen is a view of one: {:?}",
        inspector.timeline().entries()
    );
    assert_eq!(inspector.turn(), TurnState::Idle, "and no turn is running");
}

#[tokio::test]
async fn deleting_the_live_session_leaves_the_inspector_with_no_live_session() {
    // Deleting the session the inspector is sitting in is the corner #75 named
    // first, and it is offered for the reason all of them are: refusing to send
    // it would be the inspector deciding what may be observed (§7.5).
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");

    assert_eq!(inspector.delete_session(&session).await, Ok(()));

    assert_eq!(inspector.session(), None);
    assert!(inspector.timeline().is_empty());
    assert_eq!(inspector.turn(), TurnState::Idle);
}

#[tokio::test]
async fn the_trace_keeps_every_frame_of_a_session_that_was_closed() {
    // The rule the whole ring runs on: the timeline is a view and the trace is
    // the record. Ending the live session costs the first and never the second.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");

    let recorded: Vec<_> = inspector
        .trace()
        .entries()
        .into_iter()
        .map(|entry| entry.frame)
        .collect();
    assert!(!recorded.is_empty());

    inspector
        .close_session(&session)
        .await
        .expect("Testy closes it");

    let kept: Vec<_> = inspector
        .trace()
        .entries()
        .into_iter()
        .map(|entry| entry.frame)
        .collect();
    assert_eq!(
        kept[..recorded.len()],
        recorded[..],
        "every frame of the session that was closed, unmoved and unrenumbered"
    );
    assert!(
        kept.len() > recorded.len(),
        "with the close's own frames after them"
    );
}

#[tokio::test]
async fn the_listing_refreshes_after_a_close() {
    // Which is the point of the affordance rather than a convenience beside it:
    // whether a closed session still appears in a listing is exactly the kind of
    // thing agents differ on, and no document will say. So the question is asked
    // again, and what comes back is the agent's answer.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");
    inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");
    inspector.list_sessions(None).await.expect("Testy lists");
    assert!(listed(&inspector, &first));

    inspector
        .close_session(&first)
        .await
        .expect("Testy closes it");

    assert!(
        !listed(&inspector, &first),
        "nobody asked for a listing and the listing has moved on: {:?}",
        inspector.listing()
    );
    let asked = sent(&inspector);
    let closed = asked
        .iter()
        .position(|frame| frame.contains(r#""method":"session/close""#))
        .expect("the close went out");
    assert!(
        asked[closed..]
            .iter()
            .any(|frame| frame.contains(r#""method":"session/list""#)),
        "asked again after the close, not remembered from before it: {asked:?}"
    );
}

#[tokio::test]
async fn the_listing_refreshes_after_a_delete() {
    // The same question after the other call, and the one #75 named as the thing
    // worth watching: whether deletion removes a session from a subsequent
    // listing.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");
    inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");
    inspector.list_sessions(None).await.expect("Testy lists");
    assert!(listed(&inspector, &first));

    inspector
        .delete_session(&first)
        .await
        .expect("Testy deletes it");

    assert!(
        !listed(&inspector, &first),
        "Testy's own answer, refreshed without being asked: {:?}",
        inspector.listing()
    );
}

#[tokio::test]
async fn testy_advertises_both_and_answers_both() {
    // The fixture the rest of these run on, pinned rather than assumed: Testy
    // claims close and delete in the shape v1 states them — the presence of an
    // object under `sessionCapabilities` — and answers when it is driven.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());

    assert!(
        !inspector.closes_sessions() && !inspector.deletes_sessions(),
        "nothing is advertised until `initialize` has answered"
    );

    inspector.initialize().await.expect("Testy initializes");

    assert!(inspector.closes_sessions() && inspector.deletes_sessions());
}

#[tokio::test]
async fn an_agent_that_advertised_neither_is_offered_neither() {
    // Gated on the advertisement and on nothing else (§7.5), which cuts both
    // ways: a claim that was never made is an affordance that is never drawn.
    let inspector = Inspector::new();
    inspector.connect(&advertising("{}", &[]).factory());
    inspector.initialize().await.expect("it answered");

    assert!(!inspector.closes_sessions());
    assert!(!inspector.deletes_sessions());
}

/// What an agent that will not do what it advertised answers with.
const REFUSED: &str =
    r#"'{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"Method not found"}}'"#;

#[tokio::test]
async fn an_agent_that_advertised_close_without_implementing_it_is_still_offered_it() {
    // #75's capability lie, in the shape this ticket meets it: the affordance is
    // gated on the advertisement and never on the inspector's opinion of what
    // the agent can really do, so the call goes out and the refusal is the
    // finding. First-class evidence means all of it — the answer to the caller,
    // the frames in the trace, and the session it would not end still live.
    let inspector = Inspector::new();
    inspector
        .connect(&advertising(r#"{"sessionCapabilities":{"close":{}}}"#, &[REFUSED]).factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    spoken_in(&inspector).await;

    assert!(
        inspector.closes_sessions(),
        "it said it closes sessions, which is the whole of what the affordance gates on"
    );
    let refused = inspector.close_session(&live).await;

    assert!(
        matches!(refused, Err(CallError::Rejected(_))),
        "the agent's own refusal, whole: {refused:?}"
    );
    assert_eq!(
        inspector.session(),
        Some(live),
        "a session the agent would not close is a session that is still open"
    );
    assert!(
        !inspector.timeline().is_empty(),
        "with what was said in it still on screen: {:?}",
        inspector.timeline().entries()
    );
    assert!(
        sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""method":"session/close""#)),
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
async fn an_agent_that_advertised_delete_without_implementing_it_is_still_offered_it() {
    // The same rule for the other call, and the same shape of finding.
    let inspector = Inspector::new();
    inspector
        .connect(&advertising(r#"{"sessionCapabilities":{"delete":{}}}"#, &[REFUSED]).factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    assert!(inspector.deletes_sessions());
    assert!(matches!(
        inspector.delete_session(&live).await,
        Err(CallError::Rejected(_))
    ));
    assert_eq!(inspector.session(), Some(live));
}

#[tokio::test]
async fn closing_a_session_on_a_connection_that_has_gone_away_says_so() {
    // The affordance needs an agent to answer it, and a connection that went
    // away between the render and the click is a failure to report rather than a
    // button that does nothing.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a turn to look at");
    inspector.disconnect();

    assert_eq!(
        inspector.close_session(&session).await,
        Err(CallError::Disconnected)
    );
    assert_eq!(
        inspector.session(),
        Some(session),
        "nothing was ended: the ask never reached anybody"
    );
    assert!(
        !inspector.timeline().is_empty(),
        "and what the agent said is still readable after it has gone (§5)"
    );
}

/// Accepts the prompt and then waits, so a close has a live turn to land on.
const WAIT_FOR_CANCEL: &str = r#"{"command":"run_scenario","scenario":"wait_for_cancel"}"#;

#[tokio::test]
async fn what_a_close_does_to_an_in_flight_prompt_is_observed_rather_than_assumed() {
    // **The question #82 asked to be settled by driving, and it is settled**
    // (§15 q5). The specification says nothing about the ordering of a
    // `session/close` against a running turn, and nothing here sends a
    // `session/cancel` first — that would answer the question on the agent's
    // behalf. What Testy does: it answers the close *first*, and then resolves
    // the prompt with `stopReason: "cancelled"`.
    //
    // Recorded, not graded. There is no MUST behind either half of it, so
    // nothing about it is annotated (`no_annotation_is_drawn_by_any_of_this`).
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let turn = prompting(&inspector, WAIT_FOR_CANCEL);
    running(&inspector).await;

    inspector
        .close_session(&session)
        .await
        .expect("Testy closes a session with a turn running in it");

    assert_eq!(
        turn.await.expect("the prompt task"),
        Ok(v1::StopReason::Cancelled),
        "Testy resolves the walked-away prompt with the cancelled stop reason"
    );

    // The close's answer is the empty one — a `session/close` response carries
    // `_meta` and nothing else, and it is the only frame here shaped like that.
    let closed = said_at(&inspector, r#""result":{}"#).expect("the close was answered");
    let prompted = said_at(&inspector, r#""stopReason":"cancelled""#)
        .expect("and so, afterwards, was the prompt");
    assert!(
        closed < prompted,
        "the close answers before the turn it ended: {:?}",
        received(&inspector)
    );
    assert_eq!(
        inspector.session(),
        None,
        "and the session that was closed is not the live one, turn or no turn"
    );
}

/// An agent that advertises close, blocks its prompt on a
/// `session/request_permission`, answers the close — and never answers the
/// prompt it was walked away from.
///
/// **Testy cannot produce this case, and what it produces instead is a race**
/// ([#94](https://github.com/sagikazarmark/dioxus-chat.orig/issues/94)). Its
/// `callbacks` scenario resolves the prompt off its own close, and a turn that
/// settles abandons the request that blocked it — so two producers reached the
/// same request and the test could only assert whichever usually got there
/// first. A prompt that is never answered leaves the close as the only thing
/// that can reach it: no `settle()`, so no turn-side `abandon()`, and a process
/// that stays alive, so no `disconnected()` either.
///
/// Like [`rejecting_the_walked_away_turn`], it answers by counting calls rather
/// than by reading ids, which works because this client numbers its calls from
/// one, in order — so the close it answers with id 4 is the call after the
/// prompt it never answers. Nothing of the inspector's own invention comes
/// between them: it advertises close and nothing else, and a listing refresh is
/// the one call this client makes unasked (§7.5,
/// `an_agent_that_never_claimed_listing_is_not_asked_for_one`).
fn blocking_on_a_request_and_answering_only_the_close() -> AgentCommand {
    shell(concat!(
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"close":{}}}}}'; "#,
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
        // The prompt, taken and blocked on a request only the client can answer.
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":"ask","method":"session/request_permission","#,
        r#""params":{"sessionId":"s1","toolCall":{"toolCallId":"call-1","title":"Remove the build directory","kind":"delete","status":"pending"},"#,
        r#""options":[{"optionId":"yes","name":"Allow once","kind":"allow_once"},{"optionId":"no","name":"Reject once","kind":"reject_once"}]}}'; "#,
        // The close, answered — and the prompt behind it never is.
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{}}'; "#,
        "cat > /dev/null",
    ))
}

#[tokio::test]
async fn closing_the_live_session_answers_every_waiting_permission_request_cancelled() {
    // What a client owes a request it stops being able to answer (§7.2), and the
    // only way an agent blocked on one gets to end its turn. Owed to the
    // *requests*, so it is sent whether or not there was a turn to cancel — and
    // sent after the close is answered, so it cannot be mistaken for this client
    // cancelling ahead of the call.
    //
    // Scripted rather than Testy so the close is the only producer that can
    // reach the request: the debt comes due when this client stops being the one
    // that answers it, and a turn ending is another way for that to happen. What
    // is asserted here is the close paying it, so the turn must not be able to
    // end underneath the assertion (see the agent above).
    let inspector = Inspector::new();
    inspector.connect(&blocking_on_a_request_and_answering_only_the_close().factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    let turn = prompting(&inspector, "a turn to block and walk away from");
    let request = blocked(&inspector).await;

    inspector.close_session(&live).await.expect("it closed");

    assert_eq!(
        request.state(),
        PermissionState::Answered(v1::RequestPermissionOutcome::Cancelled),
        "answered, not abandoned: the agent was told"
    );
    assert!(inspector.pending_permissions().is_empty());

    // Nothing waits for the prompt: an answer that never comes is the whole of
    // what makes this agent the one that can settle the question.
    turn.abort();
}

/// Every annotation the timeline holds (§15 q7).
fn annotations(inspector: &Inspector) -> Vec<Annotation> {
    inspector
        .timeline()
        .entries()
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Annotation(annotation) => Some(annotation.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn the_corners_the_specification_leaves_undefined_are_sent_and_their_answers_reported() {
    // **Permitted, deliberately** (§7.5). Closing a session that is closed
    // already, deleting one that was never opened, deleting the live one: the
    // specification defines none of them and calls some implementation-defined
    // outright, so the inspector sends what it was asked to send and reports
    // what came back. What Testy answers is `{}` to every one of them — it
    // objects to none of it — which is a fact about Testy and not a verdict
    // about any of the calls.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let never = v1::SessionId::new("a-session-that-was-never-opened");

    assert_eq!(inspector.close_session(&session).await, Ok(()));
    assert_eq!(
        inspector.close_session(&session).await,
        Ok(()),
        "closing a session that is closed already"
    );
    assert_eq!(
        inspector.delete_session(&session).await,
        Ok(()),
        "deleting one that has been closed"
    );
    assert_eq!(
        inspector.close_session(&never).await,
        Ok(()),
        "closing one this agent never opened"
    );
    assert_eq!(
        inspector.delete_session(&never).await,
        Ok(()),
        "and deleting it"
    );

    assert_eq!(
        sent(&inspector)
            .iter()
            .filter(|frame| frame.contains(r#""method":"session/close""#))
            .count(),
        3,
        "every one of them reached the agent: {:?}",
        sent(&inspector)
    );
}

#[tokio::test]
async fn no_annotation_is_drawn_by_any_of_this() {
    // **The admission rule, as an absence** (§15 q7): an annotation is admitted
    // only where the specification states a MUST and captured frames decide it,
    // and there is no MUST anywhere in this ticket. Not for a close against a
    // running turn, not for closing a session twice, not for deleting one that
    // was never opened. Silence is not approval, and it is not a rule either.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let turn = prompting(&inspector, WAIT_FOR_CANCEL);
    running(&inspector).await;

    inspector.close_session(&session).await.expect("it closes");
    let _ = turn.await;
    inspector.close_session(&session).await.expect("again");
    inspector.delete_session(&session).await.expect("and gone");

    assert!(
        annotations(&inspector).is_empty(),
        "nothing here is a specification claim: {:?}",
        annotations(&inspector)
    );
}

/// What a close is answered with when it is answered.
const CLOSED: &str = r#"'{"jsonrpc":"2.0","id":3,"result":{}}'"#;
/// An agent that advertises `session/close` and nothing else about sessions.
const CLOSES: &str = r#"{"sessionCapabilities":{"close":{}}}"#;

#[tokio::test]
async fn the_two_gates_are_two_claims() {
    // Each on its own field, so an agent that made one of the two claims is
    // offered one of the two affordances (§7.5). Asserted apart because they
    // are apart: two gates wired to one field would pass every test that only
    // ever advertised both.
    let inspector = Inspector::new();
    inspector.connect(&advertising(CLOSES, &[]).factory());
    inspector.initialize().await.expect("it answered");

    assert!(inspector.closes_sessions());
    assert!(!inspector.deletes_sessions(), "and only that one");

    let deleting = Inspector::new();
    deleting.connect(&advertising(r#"{"sessionCapabilities":{"delete":{}}}"#, &[]).factory());
    deleting.initialize().await.expect("it answered");

    assert!(deleting.deletes_sessions());
    assert!(!deleting.closes_sessions());
}

#[tokio::test]
async fn an_agent_that_never_claimed_listing_is_not_asked_for_one() {
    // The refresh is the one call this client makes that nobody clicked, so it
    // is the one place an advertisement gates the *sending* rather than the
    // affordance (§7.5). A user who asks for a listing still gets it sent; a
    // question of the inspector's own invention is not.
    let inspector = Inspector::new();
    inspector.connect(&advertising(CLOSES, &[CLOSED]).factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    inspector.close_session(&live).await.expect("it closed");

    assert!(
        !sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""method":"session/list""#)),
        "nothing asked an agent that never said it lists: {:?}",
        sent(&inspector)
    );
}

/// An agent that advertises close, takes a prompt without answering it, and
/// then — when the close arrives — answers the close and *rejects* the prompt
/// it was walked away from.
///
/// Testy cannot produce this: it resolves the walked-away prompt with the
/// cancelled stop reason. An agent that refuses it instead is the shape that
/// says whether a failure belonging to a session that is gone can reach the
/// screen.
fn rejecting_the_walked_away_turn() -> AgentCommand {
    shell(concat!(
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"close":{}}}}}'; "#,
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
        // The prompt, taken and left open.
        r#"read _; "#,
        // The close, answered — and then the prompt refused behind it.
        r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{}}' "#,
        r#"'{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"unknown session"}}'; "#,
        "cat > /dev/null",
    ))
}

#[tokio::test]
async fn a_turn_that_fails_after_its_session_ended_says_nothing_on_a_screen_that_has_none() {
    // The turn state is the *live* session's, and after a close there is no
    // live session — so the answer to a prompt nobody is listening for reaches
    // its caller and nothing else. A failure painted here would read as a turn
    // that ended badly in a session that is not open, which is neither of the
    // two things that are true.
    let inspector = Inspector::new();
    inspector.connect(&rejecting_the_walked_away_turn().factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let turn = prompting(&inspector, "a turn to walk away from");
    running(&inspector).await;
    inspector.close_session(&live).await.expect("it closed");

    assert!(
        matches!(
            turn.await.expect("the prompt task"),
            Err(CallError::Rejected(_))
        ),
        "the caller is told what became of the turn it asked for"
    );
    assert_eq!(inspector.session(), None);
    assert_eq!(
        inspector.turn(),
        TurnState::Idle,
        "and the screen has no turn, because it has no session"
    );
    assert!(
        received(&inspector)
            .iter()
            .any(|frame| frame.contains("unknown session")),
        "the refusal is in the trace, which is where it belongs: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn an_agent_that_goes_on_talking_about_a_session_it_closed_is_shown_doing_it() {
    // The raw-first rule outranks the tidy screen (§8): showing less than the
    // agent sent is the one thing this layer may not do, so an update about a
    // session that was closed is displayed like any other — an agent still
    // streaming into a session it agreed to close is exactly the sort of thing
    // this tool exists to catch. That there is *no live session* is said by the
    // session store and by the composer reading it, never by the timeline
    // happening to be empty, which is what lets both be true at once.
    //
    // The second close is the starting pistol, not the subject: the agent says
    // its line before answering that one, and frames are read in the order they
    // crossed, so what is asserted here is a fact rather than a race. (What an
    // agent says in the moment *between* the first answer and the discard is
    // discarded with the view — the same cost a switch already states.)
    let after = format!(
        "{} '{{\"jsonrpc\":\"2.0\",\"id\":4,\"result\":{{}}}}'",
        said_in("s1", "after the session was closed"),
    );
    let inspector = Inspector::new();
    inspector.connect(&advertising(CLOSES, &[CLOSED, &after]).factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    spoken_in(&inspector).await;

    inspector.close_session(&live).await.expect("it closed");
    assert_eq!(inspector.session(), None);
    assert!(
        inspector.timeline().is_empty(),
        "the view of the session went with the session: {:?}",
        inspector.timeline().entries()
    );

    inspector
        .close_session(&live)
        .await
        .expect("closing it again is permitted, and is the pistol");

    let said: Vec<_> = inspector
        .timeline()
        .entries()
        .into_iter()
        .flat_map(|entry| entry.frames)
        .map(|recorded| recorded.frame.as_str().to_owned())
        .collect();
    assert_eq!(
        said.len(),
        1,
        "what it said after the close, and only that: {said:?}"
    );
    assert!(said[0].contains("after the session was closed"), "{said:?}");
    assert_eq!(
        inspector.session(),
        None,
        "and it is still not a session, whatever the agent says about it"
    );
}
