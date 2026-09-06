//! Conformance annotations (`docs/architecture.md` §15 q7): the three places the
//! specification says **MUST** and the frames the trace holds can say whether it
//! was met.
//!
//! **The admission rule is the subject of these tests as much as the rules
//! are.** An annotation exists only where the specification states a MUST *and*
//! the violation is decidable from captured frames, so the tests that matter
//! most are the ones asserting that nothing is annotated: a conformant agent, a
//! turn nobody cancelled, and a load of a session nothing is known to have been
//! said in.
//!
//! Testy is conformant on the first rule and supplies its negative case; on the
//! second it is the *violation*, driven and confirmed rather than taken from
//! reading its source (§15 q5). Everything neither of them can produce comes
//! from `/bin/sh` one-liners, the way `misbehaviour.rs` gets its misbehaviour
//! (§12).

mod common;

use acp_inspector_core::{
    AgentCommand, Annotation, Direction, Ending, EntryKind, Inspector, OptionSet, Replay, Restore,
    TimelineEntry, TurnState, v1,
};

use common::{QUIET, said_in, testy, until};

/// Testy's scenarios are prompt-driven, like the rest of the suite drives them.
const WAIT_FOR_CANCEL: &str = r#"{"command":"run_scenario","scenario":"wait_for_cancel"}"#;
const ECHO: &str = r#"{"command":"echo","message":"nothing to cancel"}"#;

/// The three ways a scripted agent here answers a prompt it was asked to
/// cancel, as the body of a JSON-RPC answer: none of them is `cancelled`, which
/// is what makes each of them the same MUST unmet.
const ENDED: &str = r#""result":{"stopReason":"end_turn"}"#;
const REFUSED: &str = r#""result":{"stopReason":"refusal"}"#;
const FELL_OVER: &str = r#""error":{"code":-32603,"message":"the turn fell over"}"#;

fn testy_agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

fn shell(script: &str) -> AgentCommand {
    AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\n{script}"),
        ..AgentCommand::default()
    }
}

/// The two answers every scripted agent here owes before there is a turn to
/// cancel. It answers by counting rather than by reading ids, which works
/// because this client numbers its calls from one, in order.
const HANDSHAKE: &str = concat!(
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'; "#,
    r#"read _; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'; "#,
);

/// An agent that takes a prompt, sits on it, and answers it with each of
/// `answers` in turn once the cancel has been sent — the shape of every
/// violation here.
///
/// **It reads one line more than it is answering before it says anything**, and
/// that is what makes these tests deterministic rather than a race: the turn
/// can be put provably into [`TurnState::Cancelling`] and *then* let go, by
/// cancelling a second time (see [`cancelled_turn`]). Nothing here sleeps.
///
/// It stays alive afterwards, so what a test waits for is the frame and never
/// the exit.
fn answering_after_cancel(answers: &[&str]) -> AgentCommand {
    let turns: String = answers
        .iter()
        .enumerate()
        .map(|(turn, answer)| {
            // The prompt, the cancel, the second cancel — and only then the
            // answer, addressed to the prompt's own id: `initialize` and
            // `session/new` took 1 and 2, so the first turn is 3.
            let id = turn + 3;
            format!(r#"read _; read _; read _; printf '%s\n' '{{"jsonrpc":"2.0","id":{id},{answer}}}'; "#)
        })
        .collect();

    shell(&format!("{HANDSHAKE}{turns}cat > /dev/null"))
}

/// An agent that takes the prompt and never answers it, cancel or no cancel.
fn never_answering() -> AgentCommand {
    shell(&format!("{HANDSHAKE}cat > /dev/null"))
}

/// Every annotation on the timeline, with the frames each is about.
fn annotated(inspector: &Inspector) -> Vec<(Annotation, Vec<String>)> {
    inspector
        .timeline()
        .entries()
        .iter()
        .filter_map(entry_annotation)
        .collect()
}

fn entry_annotation(entry: &TimelineEntry) -> Option<(Annotation, Vec<String>)> {
    let EntryKind::Annotation(annotation) = &entry.kind else {
        return None;
    };
    Some((
        annotation.clone(),
        entry
            .frames
            .iter()
            .map(|recorded| recorded.frame.as_str().to_owned())
            .collect(),
    ))
}

/// How many prompts this inspector has put on the wire, counted out of the
/// trace — which is the only place that knows a frame was really handed over.
fn prompts_sent(inspector: &Inspector) -> usize {
    inspector
        .trace()
        .entries()
        .iter()
        .filter(|entry| {
            entry.direction == Direction::ToAgent
                && entry
                    .frame
                    .as_str()
                    .contains(r#""method":"session/prompt""#)
        })
        .count()
}

/// Opens a session if there is none, and gets `text` onto the wire as a prompt,
/// so that what happens next happens to a turn that is really running.
async fn prompted(
    inspector: &Inspector,
    text: &str,
) -> tokio::task::JoinHandle<Result<v1::StopReason, ()>> {
    if inspector.session().is_none() {
        inspector.initialize().await.expect("it answered");
        inspector
            .new_session("/tmp", None)
            .await
            .expect("it answered");
    }

    let sent = prompts_sent(inspector);
    let mut frames = inspector.trace().changes();
    let prompting = tokio::spawn({
        let inspector = inspector.clone();
        let text = text.to_owned();
        async move { inspector.prompt(&text).await.map_err(|_| ()) }
    });

    // On the wire, not merely in flight in a task: what the trace has recorded,
    // the transport has been handed.
    until(&mut frames, "the prompt to be sent", || {
        prompts_sent(inspector) > sent
    })
    .await;

    prompting
}

/// Prompts, cancels, and hands back the turn once it is provably cancelling.
///
/// **The second cancel is the starting pistol**, not a retry: an
/// [`answering_after_cancel`] agent answers only after a third line, so a test
/// that has already asserted [`TurnState::Cancelling`] is the thing that lets
/// it answer. The order the rule is judged on is therefore the order the test
/// asserts, with nothing timed and nothing to lose a race to.
async fn cancelled_turn(
    inspector: &Inspector,
) -> tokio::task::JoinHandle<Result<v1::StopReason, ()>> {
    let prompting = prompted(inspector, "anything").await;

    inspector.cancel().await.expect("the pipe is open");
    assert_eq!(
        inspector.turn(),
        TurnState::Cancelling,
        "the turn is waiting on an agent that has been asked to stop"
    );
    inspector.cancel().await.expect("the pipe is open");

    prompting
}

#[tokio::test]
async fn a_cancelled_turn_that_resolves_as_cancelled_draws_no_annotation() {
    // The conformant case, driven against the conformant agent (§12). This is
    // the assertion the admission rule lives or dies by: an inspector that
    // annotated here would be grading rather than reporting.
    let inspector = Inspector::new();
    inspector.connect(&testy_agent().factory());

    // Testy's own scenario, cancelled once: it holds the turn open until it is
    // asked to stop, so one cancel is the whole of what this needs.
    let prompting = prompted(&inspector, WAIT_FOR_CANCEL).await;
    inspector.cancel().await.expect("Testy is alive");
    let stop = prompting
        .await
        .expect("the prompt task")
        .expect("it resolved");

    assert_eq!(stop, v1::StopReason::Cancelled);
    assert_eq!(
        annotated(&inspector),
        [],
        "an agent that did what the specification requires is not annotated for doing it"
    );
}

#[tokio::test]
async fn a_turn_nobody_cancelled_draws_no_annotation() {
    // The rule is about cancelled turns and about nothing else: an agent that
    // ends a turn with `end_turn` has ended it the way it was supposed to.
    let inspector = Inspector::new();
    inspector.connect(&testy_agent().factory());
    inspector.initialize().await.expect("Testy initializes");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a new session");

    let stop = inspector.prompt(ECHO).await.expect("the turn ran");

    assert_eq!(stop, v1::StopReason::EndTurn);
    assert_eq!(annotated(&inspector), []);
}

#[tokio::test]
async fn a_cancelled_turn_that_resolves_any_other_way_is_annotated() {
    // The violation the specification states as a MUST, and the whole reason
    // this mechanism exists: the agent takes the cancel and ends the turn as if
    // nothing had been asked of it.
    let inspector = Inspector::new();
    inspector.connect(&answering_after_cancel(&[ENDED]).factory());

    let stop = cancelled_turn(&inspector)
        .await
        .await
        .expect("the prompt task")
        .expect("it resolved");
    assert_eq!(stop, v1::StopReason::EndTurn);
    assert_eq!(inspector.turn(), TurnState::Ended(v1::StopReason::EndTurn));

    let annotations = annotated(&inspector);
    assert_eq!(
        annotations.len(),
        1,
        "one annotation, for the one rule it broke: {annotations:?}"
    );
    assert_eq!(
        annotations[0].0,
        Annotation::Cancellation(Ending::Stopped(v1::StopReason::EndTurn))
    );
}

#[tokio::test]
async fn a_cancelled_turn_answered_with_an_error_is_annotated() {
    // "Resolves with any other stop reason" includes giving none: the
    // specification asks for one answer to a cancelled prompt, and an error is
    // not it. Decidable from the same two frames as every other shape of this
    // rule.
    let inspector = Inspector::new();
    inspector.connect(&answering_after_cancel(&[FELL_OVER]).factory());

    let resolved = cancelled_turn(&inspector).await.await;
    assert!(resolved.expect("the prompt task").is_err());

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    let Annotation::Cancellation(Ending::Failed(error)) = &annotations[0].0 else {
        panic!("an answer that was no stop reason at all: {annotations:?}");
    };
    assert!(
        annotations[0].0.observed().contains("the turn fell over"),
        "and what was observed is the agent's own words: {}",
        annotations[0].0.observed()
    );
    assert!(matches!(error, acp_inspector_core::CallError::Rejected(_)));
}

#[tokio::test]
async fn a_cancelled_turn_that_never_resolves_is_annotated() {
    // The other half of the same MUST: a prompt that is never answered has not
    // resolved with `cancelled` either. It is decidable from captured frames at
    // the moment the connection ends and not a moment before — until then the
    // agent still has time, which is what `TurnState::Cancelling` says on
    // screen.
    let inspector = Inspector::new();
    inspector.connect(&never_answering().factory());

    let prompting = prompted(&inspector, "anything").await;
    inspector.cancel().await.expect("the pipe is open");
    assert_eq!(inspector.turn(), TurnState::Cancelling);
    assert_eq!(
        annotated(&inspector),
        [],
        "an agent that has been asked to stop and has not yet is not in violation of anything"
    );

    inspector.disconnect();
    assert!(prompting.await.expect("the prompt task").is_err());

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    assert_eq!(
        annotations[0].0,
        Annotation::Cancellation(Ending::Unresolved)
    );
}

#[tokio::test]
async fn an_annotation_says_what_the_specification_requires_and_what_was_observed() {
    // A specification claim rather than an opinion: it points at what the
    // specification asks for, and at what this agent did instead. Both are
    // computed in core, so both are assertable without a window.
    let ended = Annotation::Cancellation(Ending::Stopped(v1::StopReason::EndTurn));

    assert!(
        ended.requires().contains("cancelled"),
        "it names the stop reason the specification requires: {}",
        ended.requires()
    );
    assert!(
        ended.observed().contains("end_turn"),
        "and the one that was observed instead: {}",
        ended.observed()
    );

    let never = Annotation::Cancellation(Ending::Unresolved);
    assert_eq!(
        never.requires(),
        ended.requires(),
        "one rule, one requirement, however it was broken"
    );
    assert!(
        never.observed().contains("never resolved"),
        "{}",
        never.observed()
    );
}

#[tokio::test]
async fn an_annotation_carries_the_traffic_it_is_about() {
    // Attached to the traffic rather than filed beside it: the frames that
    // decide the rule are on the annotation, so whatever renders it renders the
    // evidence with it.
    let inspector = Inspector::new();
    inspector.connect(&answering_after_cancel(&[REFUSED]).factory());

    cancelled_turn(&inspector)
        .await
        .await
        .expect("the prompt task")
        .expect("resolved");

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    let (_, frames) = &annotations[0];

    assert!(
        frames
            .iter()
            .any(|frame| frame.contains(r#""method":"session/cancel""#)),
        "the cancel it is about: {frames:?}"
    );
    assert!(
        frames.iter().any(|frame| frame.contains(r#""refusal""#)),
        "and the answer that broke the rule: {frames:?}"
    );

    // Every one of them is in the trace as well, byte for byte: an annotation
    // decorates the record and never stands in for it (§8).
    let traced: Vec<String> = inspector
        .trace()
        .entries()
        .iter()
        .map(|entry| entry.frame.as_str().to_owned())
        .collect();
    for frame in frames {
        assert!(traced.contains(frame), "{frame} is in the trace");
    }
}

#[tokio::test]
async fn nothing_aggregates_the_annotations_into_a_verdict() {
    // Two violations in one session are two annotations where the traffic was,
    // and nothing anywhere is counting them: no score, no severity, no summary
    // that a reader could mistake for a grade (§15 q7).
    let inspector = Inspector::new();
    inspector.connect(&answering_after_cancel(&[ENDED, ENDED]).factory());

    for _ in 0..2 {
        cancelled_turn(&inspector)
            .await
            .await
            .expect("the prompt task")
            .expect("resolved");
    }

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 2, "one per turn: {annotations:?}");

    // And nothing else turned up in the quiet after them.
    tokio::time::sleep(QUIET).await;
    assert_eq!(annotated(&inspector).len(), 2);

    // **Two violations and still nothing that adds up.** The export is the one
    // artifact that leaves the program (§10), and it carries frames and a
    // header counting frames — no annotation, no tally of them, and nothing a
    // reader could take for a grade. An aggregate verdict would have to show up
    // here or in a store, and there is no store for one to be in.
    let taken = inspector.trace().export().to_string();
    for word in ["conformance", "annotation", "verdict", "violation", "score"] {
        assert!(
            !taken.contains(word),
            "the export says nothing about `{word}`: {taken}"
        );
    }
}

/// The session as the listing describes it, which is what opening one is handed
/// (§7.5).
///
/// Stated rather than listed here: what these tests are about is the ordering
/// of a load and its answer, and the path from a listed row to this value is
/// asserted where the affordance is (`restore.rs`).
fn info(session: &v1::SessionId) -> v1::SessionInfo {
    v1::SessionInfo::new(session.clone(), "/tmp")
}

#[tokio::test]
async fn testy_answers_a_load_without_replaying_the_conversation_it_held() {
    // **The finding this rule's first real subject was not taken on faith for**
    // (#81, §15 q5): Testy's handler *reads* as though it answers a load
    // without replaying, and driving it says so. A session it has just streamed
    // a message in is loaded back, and the answer arrives with nothing between
    // the request and it — which is the MUST unmet, and which this annotates.
    let inspector = Inspector::new();
    let session = inspector.start(&testy_agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a conversation in it");
    assert_eq!(annotated(&inspector), [], "nothing so far");

    inspector
        .restore_session(&info(&session), Restore::Load, None)
        .await
        .expect("Testy loads the session it just answered in");

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    assert_eq!(annotations[0].0, Annotation::Replay(Replay::Nothing));

    // The claim and its evidence together: the line the agent said in that
    // session, the load that asked for it back, and the answer that came
    // without it.
    let (_, frames) = &annotations[0];
    assert_eq!(frames.len(), 3, "{frames:?}");
    assert!(
        frames[0].contains(r#""method":"session/update""#),
        "{frames:?}"
    );
    assert!(
        frames[1].contains(r#""method":"session/load""#),
        "{frames:?}"
    );
    assert!(
        frames[2].contains(&session.to_string()) || frames[2].contains(r#""result""#),
        "and the answer it is judged on: {frames:?}"
    );
}

#[tokio::test]
async fn a_load_of_a_session_nothing_was_ever_said_in_draws_no_annotation() {
    // The admission rule's own boundary, and the reason the rule is written the
    // way it is: a load that replayed nothing into a session nothing is known
    // to have been said in replayed the entire conversation. The trace cannot
    // tell that from an agent withholding one, so nothing is claimed —
    // and silence is not approval (§15 q7).
    let inspector = Inspector::new();
    let session = inspector.start(&testy_agent()).await.expect("Testy starts");

    inspector
        .restore_session(&info(&session), Restore::Load, None)
        .await
        .expect("Testy loads it");

    assert_eq!(
        annotated(&inspector),
        [],
        "the same agent, the same silent answer, and nothing to claim about it"
    );
}

#[tokio::test]
async fn a_resume_that_replays_nothing_draws_no_annotation() {
    // The rule is `session/load`'s, and only its: `session/resume` restores a
    // session *without* replaying, so an agent that replays nothing into one
    // has done what resume is for. The same agent, the same conversation, the
    // other call.
    let inspector = Inspector::new();
    let session = inspector.start(&testy_agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a conversation in it");

    inspector
        .restore_session(&info(&session), Restore::Resume, None)
        .await
        .expect("Testy resumes it");

    assert_eq!(annotated(&inspector), []);
}

/// An agent that advertises load, opens `s1`, says something in it, and then
/// answers a load of it with `answer` — preceded by whatever `replay` is.
///
/// Testy cannot replay, so the conformant ordering and the refusal are both
/// scripted. The updates and the answer are one write, so the order on the wire
/// is the order the rule is judged on, with nothing timed.
fn loading(replay: &str, answer: &str) -> AgentCommand {
    shell(&format!(
        concat!(
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1,"agentCapabilities":{{"loadSession":true}}}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"sessionId":"s1"}}}}' {}; "#,
            r#"read _; printf '%s\n' {}'{}'; "#,
            "cat > /dev/null",
        ),
        said_in("s1", "what was said before"),
        replay,
        answer,
    ))
}

/// What a load answers with when it answers at all.
const LOADED: &str = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;

#[tokio::test]
async fn a_load_that_replays_before_answering_draws_no_annotation() {
    // The conformant ordering, which no agent in the suite can produce for
    // free: the conversation crosses as `session/update` notifications and the
    // answer comes after it. An inspector that annotated here would be grading
    // rather than reporting.
    let inspector = Inspector::new();
    inspector.connect(&loading(&said_in("s1", "and here it is again"), LOADED).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Load, None)
        .await
        .expect("it loaded the session");

    assert!(
        !inspector.timeline().is_empty(),
        "the replay is on the timeline"
    );
    assert_eq!(annotated(&inspector), []);
}

#[tokio::test]
async fn a_load_the_agent_refused_draws_no_annotation() {
    // A load that answered with an error restored no session, so there was no
    // conversation it owed anybody: the MUST is on a load that *answers*, and
    // the refusal is the agent's own account of itself, in the trace where the
    // caller and the screen both read it.
    let inspector = Inspector::new();
    inspector.connect(
        &loading(
            "",
            r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32603,"message":"no such session"}}"#,
        )
        .factory(),
    );
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let refused = inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Load, None)
        .await;

    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(annotated(&inspector), []);
}

#[tokio::test]
async fn the_second_rule_says_what_the_specification_requires_and_what_was_observed() {
    // The same two sentences the first rule owes, from the same two accessors:
    // adding a rule is a variant in core and no change anywhere a surface can
    // see (§15 q7).
    let silent = Annotation::Replay(Replay::Nothing);

    assert!(
        silent.requires().contains("session/update"),
        "it names what the specification asks to be replayed: {}",
        silent.requires()
    );
    assert!(
        silent.requires().contains("must not answer until"),
        "and the ordering, which is the whole of the rule: {}",
        silent.requires()
    );
    assert!(
        silent.observed().contains("replayed nothing"),
        "{}",
        silent.observed()
    );
    assert_ne!(
        silent.requires(),
        Annotation::Cancellation(Ending::Unresolved).requires(),
        "two rules, two requirements"
    );
}

#[tokio::test]
async fn the_two_rules_are_judged_apart_and_still_add_up_to_nothing() {
    // The rule list grew and nothing else did. One session, both rules armed in
    // it, and each judged on its own traffic: the cancelled turn Testy ends the
    // way the specification says draws nothing, the load it answers without
    // replaying draws one, and there is no count, no severity and nothing that
    // adds the two rules up (§15 q7).
    let inspector = Inspector::new();
    let session = inspector.start(&testy_agent()).await.expect("Testy starts");
    inspector.prompt(ECHO).await.expect("a conversation in it");

    let prompting = prompted(&inspector, WAIT_FOR_CANCEL).await;
    inspector.cancel().await.expect("Testy is alive");
    assert_eq!(
        prompting.await.expect("the prompt task"),
        Ok(v1::StopReason::Cancelled)
    );
    assert_eq!(annotated(&inspector), [], "which is what it had to do");

    inspector
        .restore_session(&info(&session), Restore::Load, None)
        .await
        .expect("Testy loads it");

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    assert_eq!(annotations[0].0, Annotation::Replay(Replay::Nothing));

    tokio::time::sleep(QUIET).await;
    assert_eq!(annotated(&inspector).len(), 1);
}

/// Two select config options, as a session setup response publishes them.
///
/// Two rather than one because the rule's boundary needs both: an answer can
/// omit the option that was just set, which is the violation, or omit the other
/// one, which is an agent saying its session has one fewer option and is not
/// this rule's business.
fn offered() -> String {
    format!(r#""configOptions":[{VERBOSITY},{MODEL}]"#)
}

/// A successful answer to the `call`th call, carrying `options` as the whole
/// option set.
///
/// The call id is a parameter because a rule is judged per set: an agent that
/// breaks it twice is answering two calls, and these scripts answer by counting
/// rather than by reading ids.
fn answered(call: u8, options: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{call},"result":{{"configOptions":[{options}]}}}}"#)
}

/// The option that is set in these tests, as the agent lists it.
const VERBOSITY: &str = concat!(
    r#"{"id":"verbosity","name":"Verbosity","type":"select","currentValue":"verbose","#,
    r#""options":[{"value":"normal","name":"Normal"},{"value":"verbose","name":"Verbose"}]}"#,
);
/// And the one beside it, which is never set and is the shrinking rule's
/// subject.
const MODEL: &str = concat!(
    r#"{"id":"model","name":"Model","type":"select","currentValue":"haiku","#,
    r#""options":[{"value":"haiku","name":"Haiku"},{"value":"opus","name":"Opus"}]}"#,
);

/// The answer that breaks the rule: a set that succeeded, answered with an
/// option set the id just written is not in.
fn without_the_one_set(call: u8) -> String {
    answered(call, MODEL)
}

/// The answer that does not: a set that succeeded, answered with a *smaller*
/// option set that still contains the id just written.
fn without_the_other(call: u8) -> String {
    answered(call, VERBOSITY)
}

/// And the refusal the same call can draw, which owed no option list at all.
const SET_REFUSED: &str =
    r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"unsupported verbosity value"}}"#;

/// The option this client asks these agents to change, and the value it asks
/// for.
fn verbosity() -> v1::SessionConfigId {
    v1::SessionConfigId::new("verbosity")
}

fn verbose() -> v1::SessionConfigOptionValue {
    v1::SessionConfigOptionValue::value_id("verbose")
}

/// The ids of the config options the store holds, which is what says what the
/// answer to a set did to the surface.
fn published(inspector: &Inspector) -> Vec<String> {
    inspector
        .session_settings()
        .config_options
        .as_ref()
        .expect("the answer carried a set")
        .iter()
        .map(|option| option.id.to_string())
        .collect()
}

/// An agent that publishes [`offered`] on the session it opens and answers
/// whatever is asked of it next with each of `answers` in turn.
///
/// Testy publishes one config option and answers a set with its whole list, so
/// it is the conformant agent for this rule and cannot be its subject: the
/// violation is a scripted one-liner, the way `misbehaviour.rs` gets its
/// misbehaviour (§12).
fn offering(answers: &[&str]) -> AgentCommand {
    let turns: String = answers
        .iter()
        .map(|answer| format!(r#"read _; printf '%s\n' '{answer}'; "#))
        .collect();

    shell(&format!(
        concat!(
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1}}}}'; "#,
            r#"read _; printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"sessionId":"s1",{}}}}}'; "#,
            "{}cat > /dev/null",
        ),
        offered(),
        turns,
    ))
}

#[tokio::test]
async fn a_set_answered_with_an_option_list_missing_the_id_just_set_is_annotated() {
    // The third rule, and the whole of what makes it decidable (§15 q7): the
    // option provably exists, because this client had just set it and the agent
    // said the set succeeded — so a list without it cannot be the complete set
    // the specification requires. Two frames decide it and no model of what the
    // agent is doing is involved.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[&without_the_one_set(3)]))
        .await
        .expect("the agent opens a session");
    assert_eq!(annotated(&inspector), [], "nothing so far");

    inspector
        .set_config_option(&verbosity(), &verbose())
        .await
        .expect("the agent answered the set");

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    assert_eq!(
        annotations[0].0,
        Annotation::OptionSet(OptionSet::Missing(verbosity()))
    );
}

#[tokio::test]
async fn the_third_rule_carries_the_traffic_it_is_about() {
    // Attached to the traffic rather than filed beside it, exactly as the two
    // rules before it are: the two frames that decide it — the set this client
    // sent, and the answer that said it succeeded without the option in it —
    // are on the annotation, so whatever renders it renders the evidence with
    // it.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[&without_the_one_set(3)]))
        .await
        .expect("the agent opens a session");

    inspector
        .set_config_option(&verbosity(), &verbose())
        .await
        .expect("the agent answered the set");

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    let (_, frames) = &annotations[0];
    assert_eq!(frames.len(), 2, "{frames:?}");
    assert!(
        frames[0].contains(r#""method":"session/set_config_option""#)
            && frames[0].contains(r#""configId":"verbosity""#),
        "the ask, naming the option the answer owed: {frames:?}"
    );
    assert!(
        frames[1].contains(r#""configOptions""#) && !frames[1].contains(r#""verbosity""#),
        "and the answer that broke the rule: {frames:?}"
    );

    // Every one of them is in the trace as well, byte for byte: an annotation
    // decorates the record and never stands in for it (§8).
    let traced: Vec<String> = inspector
        .trace()
        .entries()
        .iter()
        .map(|entry| entry.frame.as_str().to_owned())
        .collect();
    for frame in frames {
        assert!(traced.contains(frame), "{frame} is in the trace");
    }
}

#[tokio::test]
async fn a_set_answered_with_a_list_the_option_is_in_draws_no_annotation() {
    // The conformant case, driven against the conformant agent (§12), and the
    // assertion this rule lives or dies by as much as the first one does: Testy
    // publishes one select option and answers a set with its whole list. An
    // inspector that annotated here would be grading rather than reporting.
    let inspector = Inspector::new();
    inspector.start(&testy_agent()).await.expect("Testy starts");

    inspector
        .set_config_option(&verbosity(), &verbose())
        .await
        .expect("Testy accepts a value it published");

    assert_eq!(
        annotated(&inspector),
        [],
        "an agent that answered with the complete set is not annotated for doing it"
    );
}

#[tokio::test]
async fn an_option_set_that_shrank_without_losing_the_one_just_set_draws_no_annotation() {
    // **The looser formulation, rejected and asserted as rejected** (§15 q7).
    // The agent drops an option it advertised a moment ago and keeps the one
    // that was written: from the trace alone that is indistinguishable from an
    // agent whose option set legitimately shrank, so nothing is claimed. Only
    // the option that was just set is provably still there to be listed.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[&without_the_other(3)]))
        .await
        .expect("the agent opens a session");

    inspector
        .set_config_option(&verbosity(), &verbose())
        .await
        .expect("the agent answered the set");

    // The set really did shrink, so the silence is about the rule rather than
    // about nothing having happened: the store took the agent's word whole and
    // the option it stopped publishing is gone.
    assert_eq!(published(&inspector), ["verbosity"]);

    assert_eq!(annotated(&inspector), []);
}

#[tokio::test]
async fn a_set_the_agent_refused_draws_no_annotation() {
    // A set that failed owed no option list at all: the MUST is on the response
    // to a set that *succeeded*, and an agent that said no said no. The refusal
    // is the agent's own account of itself, in the trace and beside the surface
    // where the user reads it (§7.6) — reporting it as a broken rule as well
    // would be inventing one.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[SET_REFUSED]))
        .await
        .expect("the agent opens a session");

    let refused = inspector.set_config_option(&verbosity(), &verbose()).await;

    assert!(refused.is_err(), "{refused:?}");
    assert!(
        inspector.session_settings().config_refusal.is_some(),
        "the refusal is said beside the surface, which is where it belongs"
    );
    assert_eq!(annotated(&inspector), []);
}

#[tokio::test]
async fn a_session_set_mode_the_agent_says_nothing_more_about_draws_no_annotation() {
    // **Permitted silence is not a fault** (§15 q7). `session/set_mode` answers
    // `{}` and no MUST anywhere obliges the agent to restate the mode
    // afterwards, so there is no rule for this to break — Testy accepts the
    // mode, answers empty, and never announces. What the surface says about it
    // is that the set was acknowledged and not confirmed (§7.6), which is a
    // statement about what is known rather than a complaint.
    let inspector = Inspector::new();
    inspector.start(&testy_agent()).await.expect("Testy starts");

    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("Testy accepts one of the modes it published");

    assert!(
        inspector.session_settings().mode_change.is_some(),
        "the set is reported as what it was, beside the surface"
    );
    assert_eq!(
        annotated(&inspector),
        [],
        "and silence the specification permits is not reported as a fault"
    );
}

/// The option that was set, in a shape v1 cannot read: the agent's own id, and
/// a kind that is a third string (§15 q10).
const VERBOSITY_UNREADABLE: &str =
    r#"{"id":"verbosity","name":"Verbosity","type":"slider","currentValue":0.7}"#;

#[tokio::test]
async fn an_option_this_client_could_not_read_is_still_an_option_the_answer_carried() {
    // **The rule is decided from the frame, not from the decoded view** (§8).
    // The typed layer skips option entries it cannot deserialize, so an option
    // the agent really listed can be missing from the store while sitting in
    // the trace — and a rule that read the store would claim the agent omitted
    // something the record shows it sent. Under-reporting is the direction a
    // rule decided from frames is allowed to fail in; contradicting the trace
    // is not.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[&answered(
            3,
            &format!("{VERBOSITY_UNREADABLE},{MODEL}"),
        )]))
        .await
        .expect("the agent opens a session");

    inspector
        .set_config_option(&verbosity(), &verbose())
        .await
        .expect("the agent answered the set");

    // The decoded view lost it, which is the limit §7.6 already names.
    assert_eq!(published(&inspector), ["model"]);

    // And the frame did not, so there is nothing to claim.
    assert_eq!(
        annotated(&inspector),
        [],
        "the option is in the answer the agent sent, whatever this client could read of it"
    );
}

#[tokio::test]
async fn the_third_rule_says_what_the_specification_requires_and_what_was_observed() {
    // The same two sentences the rules before it owe, from the same two
    // accessors: a rule is a variant in core and no change anywhere a surface
    // can see (§15 q7).
    let missing = Annotation::OptionSet(OptionSet::Missing(verbosity()));

    assert!(
        missing.requires().contains("complete set"),
        "it names what the specification asks the answer to carry: {}",
        missing.requires()
    );
    assert!(
        missing.observed().contains("verbosity"),
        "and what was observed is about the agent's own option: {}",
        missing.observed()
    );
    assert_ne!(
        missing.requires(),
        Annotation::Replay(Replay::Nothing).requires(),
        "three rules, three requirements"
    );
}

#[tokio::test]
async fn two_sets_broken_the_same_way_are_two_annotations_and_no_verdict() {
    // The rule list grew and nothing else did (§15 q7). Two violations in one
    // session are two annotations where the traffic was, and nothing anywhere
    // is counting them: no score, no severity, no summary a reader could
    // mistake for a grade.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[
            &without_the_one_set(3),
            &without_the_one_set(4),
        ]))
        .await
        .expect("the agent opens a session");

    for _ in 0..2 {
        inspector
            .set_config_option(&verbosity(), &verbose())
            .await
            .expect("the agent answered the set");
    }

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 2, "one per set: {annotations:?}");

    // And nothing else turned up in the quiet after them.
    tokio::time::sleep(QUIET).await;
    assert_eq!(annotated(&inspector).len(), 2);

    // The export is the one artifact that leaves the program (§10), and it
    // still carries frames and a header counting frames — no annotation, no
    // tally of them, and nothing a reader could take for a grade.
    let taken = inspector.trace().export().to_string();
    for word in ["conformance", "annotation", "verdict", "violation", "score"] {
        assert!(
            !taken.contains(word),
            "the export says nothing about `{word}`: {taken}"
        );
    }
}

#[tokio::test]
async fn a_set_answered_with_something_that_is_no_option_list_at_all_is_annotated() {
    // The same rule at the shape below it. `configOptions` is on the answer and
    // is not a list of options, so the id just written is not in it — which is
    // decidable from the frame exactly as an ordinary omission is, and cannot
    // be wrong about a conformant agent, because a conformant one sends the
    // list. The surface reaches the same verdict by another route: the typed
    // layer reads the unreadable field as *no options*, so the row disappears,
    // and an annotation is what says why.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[
            r#"{"jsonrpc":"2.0","id":3,"result":{"configOptions":"all of them"}}"#,
        ]))
        .await
        .expect("the agent opens a session");

    inspector
        .set_config_option(&verbosity(), &verbose())
        .await
        .expect("the agent answered the set");

    let annotations = annotated(&inspector);
    assert_eq!(annotations.len(), 1, "{annotations:?}");
    assert_eq!(
        annotations[0].0,
        Annotation::OptionSet(OptionSet::Missing(verbosity()))
    );
}

#[tokio::test]
async fn a_set_answered_with_no_option_list_at_all_draws_no_annotation() {
    // The rule's floor, and the boundary its statement names: an answer with no
    // `configOptions` on it carries no option set to be judged against. What
    // the surface does with an answer it could not read is say the set did not
    // happen (§7.6) — reporting a broken MUST on top of that would be claiming
    // a shortfall in a list the agent never sent.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[r#"{"jsonrpc":"2.0","id":3,"result":{}}"#]))
        .await
        .expect("the agent opens a session");

    let unread = inspector.set_config_option(&verbosity(), &verbose()).await;

    assert!(unread.is_err(), "{unread:?}");
    assert!(
        inspector.session_settings().config_refusal.is_some(),
        "the set is reported as one that did not happen, beside the surface"
    );
    assert_eq!(annotated(&inspector), []);
}
