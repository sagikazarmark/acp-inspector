//! The live session's Session Settings (`docs/architecture.md` §7.6): the modes
//! and the config options an agent published for the session that is open, and
//! what it says about them afterwards.
//!
//! **Advertisement here is presence of data, not a capability** — the `modes`
//! and `configOptions` fields of a session setup response — so these assert what
//! an agent *published*, never what the inspector would like to offer: an agent
//! that published no modes advertises none, and one that published the same
//! setting twice published it twice.
//!
//! Driven through the public inspector API only, the way the shell drives it
//! (§12). Testy is the agent wherever a conformant one can produce the case — it
//! publishes two modes and one select option on every session setup response —
//! and `/bin/sh` one-liners cover the shapes it has no way to send.

mod common;

use acp_inspector_core::{AgentCommand, CallError, EntryKind, Inspector, ModeChange, Restore, v1};

use common::{position, prompting, received, running, said_at, sent, shell, testy, until};

/// What a conformant agent answers `initialize` with, claiming nothing.
const DESCRIBED: &str =
    r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}"#;
/// The same, from an agent that advertises both ways of opening a session it
/// already has.
const DESCRIBED_REOPENING: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}}}"#;

/// Two modes with the first one current, as a setup response publishes them.
const MODES: &str = r#""modes":{"currentModeId":"chat","availableModes":[{"id":"chat","name":"Chat"},{"id":"plan","name":"Plan"}]}"#;
/// One select config option with three values, as a setup response publishes
/// them.
const OPTIONS: &str = r#""configOptions":[{"id":"verbosity","name":"Verbosity","type":"select","currentValue":"normal","options":[{"value":"brief","name":"Brief"},{"value":"normal","name":"Normal"},{"value":"verbose","name":"Verbose"}]}]"#;
/// A config option whose id is literally `mode` — the shape a real agent
/// publishes beside its modes (§7.6).
const MODE_OPTION: &str = r#""configOptions":[{"id":"mode","name":"Mode","type":"select","currentValue":"chat","options":[{"value":"chat","name":"Chat"},{"value":"plan","name":"Plan"}]}]"#;
/// One boolean config option, turned off.
///
/// The shape a conformant agent may send **only** to a client that claimed
/// `clientCapabilities.session.configOptions.boolean` (§7.3, §7.6), which is
/// the one claim this client makes about itself. Testy publishes none and
/// refuses a boolean payload outright, so every boolean case here is a scripted
/// agent's.
const BOOLEAN: &str =
    r#""configOptions":[{"id":"fast","name":"Fast","type":"boolean","currentValue":false}]"#;
/// A select option this client can read, beside one whose kind is a *third*
/// string — the shape §15 q10 is about.
const THIRD_KIND: &str = r#""configOptions":[{"id":"verbosity","name":"Verbosity","type":"select","currentValue":"normal","options":[{"value":"normal","name":"Normal"}]},{"id":"temperature","name":"Temperature","type":"slider","currentValue":0.7,"minimum":0,"maximum":1}]"#;

/// What a `session/set_mode` is answered with: `{}`, and nothing else the
/// protocol has a shape for. The third call this client makes, after
/// `initialize` and `session/new`.
const MODE_SET: &str = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;
/// And the refusal the same call can draw, which is what an agent says about a
/// mode it does not have.
const MODE_REFUSED: &str =
    r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"unsupported mode `architect`"}}"#;
/// Accepts the prompt and then waits, so a set has a live turn to land in.
const WAIT_FOR_CANCEL: &str = r#"{"command":"run_scenario","scenario":"wait_for_cancel"}"#;

/// The whole option set an agent answers a single write with: a different
/// option, with a different value, and nothing of the one that was asked about.
const ANSWERED_ELSEWHERE: &str = r#"{"jsonrpc":"2.0","id":3,"result":{"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"haiku","options":[{"value":"haiku","name":"Haiku"},{"value":"opus","name":"Opus"}]}]}}"#;
/// The whole option set an agent answers a boolean write with: the option that
/// was written, turned on — and one it never published before, because the
/// answer is the *complete replacement set* and the store takes it whole rather
/// than patching the option it asked about.
const TURNED_ON: &str = r#"{"jsonrpc":"2.0","id":3,"result":{"configOptions":[{"id":"fast","name":"Fast","type":"boolean","currentValue":true},{"id":"thinking","name":"Thinking","type":"boolean","currentValue":false}]}}"#;
/// And the refusal the same write can draw, from an agent that publishes the
/// option and will not have it changed.
const BOOLEAN_REFUSED: &str = r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"`fast` cannot be turned on here"}}"#;
/// And the refusal Testy answers a value it does not have with.
const VALUE_REFUSED: &str = "unsupported verbosity value";
/// The refusal it answers an option it does not have with.
const OPTION_REFUSED: &str = "unsupported config option";

/// The agent announcing that the session has changed mode.
const NOW_PLANNING: &str = r#"{"sessionUpdate":"current_mode_update","currentModeId":"plan"}"#;
/// The agent restating its whole option set, with one fewer value and a
/// different current one.
const NOW_VERBOSE: &str = r#"{"sessionUpdate":"config_option_update","configOptions":[{"id":"verbosity","name":"Verbosity","type":"select","currentValue":"verbose","options":[{"value":"brief","name":"Brief"},{"value":"verbose","name":"Verbose"}]}]}"#;

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

/// One `read`-and-`printf`: the answer to whatever was asked, and anything the
/// agent says after sending it.
fn exchange(frames: &[&str]) -> String {
    let printed: String = frames.iter().map(|frame| format!(" '{frame}'")).collect();
    format!("read _; printf '%s\\n'{printed}; ")
}

/// A `session/new` answer publishing `settings` beside the session id.
///
/// `call` is the id it answers under, and the scripts count rather than read
/// them: this client numbers its calls from one, in order, so the first
/// `session/new` is the second call and the one a switch makes is the third.
fn opened_as(call: u8, session: &str, settings: &[&str]) -> String {
    let published: String = settings.iter().map(|field| format!(",{field}")).collect();
    format!(r#"{{"jsonrpc":"2.0","id":{call},"result":{{"sessionId":"{session}"{published}}}}}"#)
}

/// The `session/new` answer that opens the session a script starts with.
fn opened(settings: &[&str]) -> String {
    opened_as(2, "s1", settings)
}

/// And the one that opens the session a switch creates.
fn opened_again(settings: &[&str]) -> String {
    opened_as(3, "s2", settings)
}

/// A `session/load` answer, which carries the same two fields and no id.
fn reopened(settings: &[&str]) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":3,"result":{{{}}}}}"#,
        settings.join(",")
    )
}

/// And the refusal the same call can draw, from an agent that replayed a
/// conversation and then would not hand the session back.
const REOPEN_REFUSED: &str =
    r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"no such session"}}"#;

/// One `session/update` in a session the scripts name.
fn said_of(session: &str, update: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"{session}","update":{update}}}}}"#
    )
}

/// One `session/update` in the session the scripts open.
fn said(update: &str) -> String {
    said_of("s1", update)
}

/// An agent that publishes `settings` on the session it opens, and then says
/// each of `updates`.
fn publishing(settings: &[&str], updates: &[&str]) -> AgentCommand {
    let answer = opened(settings);
    let mut frames = vec![answer.as_str()];
    frames.extend(updates);

    shell(&format!(
        "{}{}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&frames),
    ))
}

/// An agent that publishes two modes, and answers the set it is then sent with
/// `answers` — in whatever order it wants to say them.
///
/// Testy is the agent for the set itself; this is for the one thing Testy has
/// no way to do, which is *announce* the change it made.
fn setting(answers: &[&str]) -> AgentCommand {
    shell(&format!(
        "{}{}{}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[MODES]).as_str()]),
        exchange(answers),
    ))
}

/// An agent that advertises both ways of opening a session it already has,
/// publishes modes and config options on the one it creates, and then says
/// `answers` — the replay and the setup response of the session that is being
/// opened next, in the order it wants them read.
///
/// The switch is what it exists for: the session it hands out first is the one
/// being left, and it published both halves of the settings so that a surface
/// still showing either of them after the switch is a surface describing a
/// session that is not open.
fn reopening(answers: &[&str]) -> AgentCommand {
    shell(&format!(
        "{}{}{}cat > /dev/null",
        exchange(&[DESCRIBED_REOPENING]),
        exchange(&[opened(&[MODES, OPTIONS]).as_str()]),
        exchange(answers),
    ))
}

/// An agent that publishes one select config option, and answers the set it is
/// then sent with `answers`.
///
/// Testy is the agent for the ordinary set; this is for the answers a
/// conformant one has no reason to give.
fn offering(answers: &[&str]) -> AgentCommand {
    shell(&format!(
        "{}{}{}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[OPTIONS]).as_str()]),
        exchange(answers),
    ))
}

/// An agent that publishes one *boolean* config option, and answers the set it
/// is then sent with `answers`.
///
/// [`offering`]'s other kind, and scripted for a different reason: Testy has no
/// way to reach this path at all — it publishes no boolean option, and its
/// handler refuses a boolean payload outright — so what this client's own claim
/// buys (§7.3) is only assertable against an agent written to send one.
fn toggling(answers: &[&str]) -> AgentCommand {
    shell(&format!(
        "{}{}{}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[BOOLEAN]).as_str()]),
        exchange(answers),
    ))
}

/// The config options the agent published, in the order it published them.
fn options(settings: &acp_inspector_core::SessionSettings) -> &[v1::SessionConfigOption] {
    settings
        .config_options
        .as_deref()
        .expect("the agent published config options")
}

/// The ids of those options, which is the list an agent's answer is checked
/// against.
fn published_options(settings: &acp_inspector_core::SessionSettings) -> Vec<String> {
    options(settings)
        .iter()
        .map(|option| option.id.to_string())
        .collect()
}

/// One published config option, by id.
fn option_named<'a>(
    settings: &'a acp_inspector_core::SessionSettings,
    id: &str,
) -> &'a v1::SessionConfigOption {
    let options = options(settings);
    options
        .iter()
        .find(|option| option.id.to_string() == id)
        .unwrap_or_else(|| panic!("{id} is among the options: {options:?}"))
}

/// The current value of a select config option, by id.
fn selected(settings: &acp_inspector_core::SessionSettings, id: &str) -> String {
    match &option_named(settings, id).kind {
        v1::SessionConfigKind::Select(select) => select.current_value.to_string(),
        other => panic!("{id} is a select option: {other:?}"),
    }
}

/// The current value of a boolean config option, by id.
///
/// [`selected`]'s other kind, and read the same way: what the option is set to
/// is whatever the agent last stated it as.
fn switched(settings: &acp_inspector_core::SessionSettings, id: &str) -> bool {
    match &option_named(settings, id).kind {
        v1::SessionConfigKind::Boolean(boolean) => boolean.current_value,
        other => panic!("{id} is a boolean option: {other:?}"),
    }
}

#[tokio::test]
async fn the_settings_on_a_session_setup_response_stop_being_discarded() {
    // What the ring is about: the typed client read nothing out of a session
    // setup response, which is exactly where a session's modes and config
    // options ride (§7.6). They are the store's first source now.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let settings = inspector.session_settings();
    let modes = settings.modes.as_ref().expect("Testy publishes modes");
    assert_eq!(modes.current_mode_id.to_string(), "chat");
    assert_eq!(
        modes
            .available_modes
            .iter()
            .map(|mode| mode.id.to_string())
            .collect::<Vec<_>>(),
        ["chat", "plan"],
        "the set it offered, in the order it offered it"
    );

    let options = settings
        .config_options
        .as_ref()
        .expect("and one config option");
    assert_eq!(
        options
            .iter()
            .map(|option| option.id.to_string())
            .collect::<Vec<_>>(),
        ["verbosity"],
    );
    assert_eq!(
        selected(&settings, "verbosity"),
        "normal",
        "with the value it is currently set to"
    );
}

#[tokio::test]
async fn an_agent_that_published_no_modes_advertises_none() {
    // The gate is presence of the data and nothing else (§7.6): an absent
    // `modes` field is the agent's answer, so there is nothing for a surface to
    // draw a mode control out of.
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[OPTIONS], &[]))
        .await
        .expect("the agent opens a session");

    let settings = inspector.session_settings();
    assert_eq!(settings.modes, None, "no modes were published");
    assert!(
        settings.config_options.is_some(),
        "and the half that was published is there: {settings:?}"
    );
}

#[tokio::test]
async fn an_agent_that_published_no_config_options_advertises_none() {
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[MODES], &[]))
        .await
        .expect("the agent opens a session");

    let settings = inspector.session_settings();
    assert_eq!(settings.config_options, None);
    assert!(settings.modes.is_some(), "{settings:?}");
}

#[tokio::test]
async fn an_agent_that_published_nothing_advertises_nothing() {
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[], &[]))
        .await
        .expect("the agent opens a session");

    assert!(
        inspector.session_settings().is_empty(),
        "the session carries no settings at all: {:?}",
        inspector.session_settings()
    );
}

#[tokio::test]
async fn a_current_mode_update_settles_the_mode_the_surface_shows() {
    // The store's third source, and the agent that announces its change being
    // rewarded with a surface that agrees with it (§7.6).
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[MODES], &[&said(NOW_PLANNING)]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.session_settings_changes();
    until(&mut changes, "the announced mode to settle", || {
        inspector
            .session_settings()
            .modes
            .is_some_and(|modes| modes.current_mode_id.to_string() == "plan")
    })
    .await;

    let modes = inspector.session_settings().modes.expect("still published");
    assert_eq!(
        modes.available_modes.len(),
        2,
        "and the set it offered is untouched: an announcement names a mode, not a menu"
    );
}

#[tokio::test]
async fn a_config_option_update_replaces_the_option_set() {
    // The notification carries the *complete* set, so the store takes it whole
    // rather than folding it into what was published: an option set that
    // legitimately shrinks is the agent's word about its session (§7.6).
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[OPTIONS], &[&said(NOW_VERBOSE)]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.session_settings_changes();
    until(&mut changes, "the restated option set to land", || {
        inspector
            .session_settings()
            .config_options
            .is_some_and(|options| {
                options.iter().any(|option| match &option.kind {
                    v1::SessionConfigKind::Select(select) => {
                        select.current_value.to_string() == "verbose"
                    }
                    _ => false,
                })
            })
    })
    .await;

    let settings = inspector.session_settings();
    let options = settings.config_options.as_ref().expect("still published");
    assert_eq!(options.len(), 1);
    match &options[0].kind {
        v1::SessionConfigKind::Select(select) => match &select.options {
            v1::SessionConfigSelectOptions::Ungrouped(values) => assert_eq!(
                values.len(),
                2,
                "the values are the restated ones, not the published ones merged with them"
            ),
            other => panic!("an ungrouped select: {other:?}"),
        },
        other => panic!("a select option: {other:?}"),
    }
}

#[tokio::test]
async fn a_restated_option_set_is_taken_even_where_the_setup_response_carried_none() {
    // The asymmetry with a mode announcement, and the protocol's reason for it
    // (§7.6): this notification carries the *whole list*, so taking it is
    // taking the agent's word about its own session rather than inferring an
    // affordance from traffic. Refusing it would be a surface showing less than
    // the agent sent, which is the one thing this layer may not do (§8).
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[], &[&said(NOW_VERBOSE)]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.session_settings_changes();
    until(&mut changes, "the option set to land", || {
        inspector.session_settings().config_options.is_some()
    })
    .await;

    let settings = inspector.session_settings();
    assert_eq!(
        selected(&settings, "verbosity"),
        "verbose",
        "the list the agent sent, whole"
    );
    assert_eq!(
        settings.modes, None,
        "and nothing it did not send: an announcement about one kind says nothing about the other"
    );
}

#[tokio::test]
async fn a_mode_announced_by_an_agent_that_published_none_advertises_nothing() {
    // The gate is the advertisement and the advertisement is the setup
    // response's (§7.6). Inferring an affordance from an announcement would be
    // the inspector deciding on the agent's behalf what it supports — so the
    // store stays empty, and the announcement stays where it happened.
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[], &[&said(NOW_PLANNING)]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the announcement to arrive", || {
        !inspector.timeline().is_empty()
    })
    .await;

    assert!(
        inspector.session_settings().is_empty(),
        "an announced mode is not an advertised one: {:?}",
        inspector.session_settings()
    );
}

#[tokio::test]
async fn reading_a_settings_update_leaves_it_on_the_timeline() {
    // The surface says what a setting *is*; the timeline says *when* it
    // changed and whether the agent announced it (§7.6). This layer reads the
    // update on the way past rather than in place of it, so both are kept.
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[MODES], &[&said(NOW_PLANNING)]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the announcement to arrive", || {
        !inspector.timeline().is_empty()
    })
    .await;

    let announced = inspector
        .timeline()
        .entries()
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                EntryKind::Update(v1::SessionUpdate::CurrentModeUpdate(_))
            )
        })
        .count();
    assert_eq!(announced, 1, "the entry is where it was");
}

#[tokio::test]
async fn ending_the_live_session_leaves_no_settings_behind() {
    // The settings are the live session's account of itself, so they go the way
    // the timeline goes when there is no longer a session for them to describe
    // (§7.5, §7.6).
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    assert!(!inspector.session_settings().is_empty(), "Testy published");

    inspector
        .close_session(&session)
        .await
        .expect("Testy closes it");

    assert_eq!(inspector.session(), None);
    assert!(
        inspector.session_settings().is_empty(),
        "nothing is open for a surface to describe: {:?}",
        inspector.session_settings()
    );
}

#[tokio::test]
async fn a_setting_published_as_both_a_mode_and_a_config_option_is_held_as_both() {
    // A duplicated control is a fact about that agent and not a mess to tidy
    // away (§7.6): a real agent opens its config option list with an entry whose
    // id is literally `mode`, and the store keeps both halves of what it
    // published.
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[MODES, MODE_OPTION], &[]))
        .await
        .expect("the agent opens a session");

    let settings = inspector.session_settings();
    assert!(
        settings.modes.is_some(),
        "the modes it published: {settings:?}"
    );
    assert_eq!(
        settings
            .config_options
            .as_ref()
            .expect("and the options it published")
            .iter()
            .map(|option| option.id.to_string())
            .collect::<Vec<_>>(),
        ["mode"],
        "including the one that is about modes"
    );
}

#[tokio::test]
async fn a_session_opened_again_is_filled_from_its_own_answer() {
    // Every session setup response is a source, not just `session/new`'s (§7.6)
    // — and all three of them, because `session/load` and `session/resume` are
    // one operation with a property and carry the same two fields. Both are
    // driven, because both are decoded as their own type.
    for how in [Restore::Load, Restore::Resume] {
        let inspector = Inspector::new();
        let agent = shell(&format!(
            "{}{}{}cat > /dev/null",
            exchange(&[DESCRIBED_REOPENING]),
            exchange(&[opened(&[]).as_str()]),
            exchange(&[reopened(&[MODES, OPTIONS]).as_str()]),
        ));

        inspector.start(&agent).await.expect("a session to leave");
        assert!(
            inspector.session_settings().is_empty(),
            "the session it opened published nothing"
        );

        inspector
            .restore_session(&v1::SessionInfo::new("s1", "/tmp"), how, None)
            .await
            .unwrap_or_else(|error| panic!("the agent answers {}: {error}", how.method()));

        let settings = inspector.session_settings();
        assert!(
            settings.modes.is_some() && settings.config_options.is_some(),
            "the reopened session's own answer fills the surface after {}: {settings:?}",
            how.method()
        );
    }
}

#[tokio::test]
async fn a_mode_the_agent_accepts_and_never_restates_is_acknowledged_and_unconfirmed() {
    // The middle case of the ring, and the one the specification leaves room
    // for: `session/set_mode` answers `{}`, and no MUST obliges the agent to
    // say anything afterwards (§7.6). Testy proves it — its handler mutates its
    // state and answers without announcing — so the surface is left holding a
    // set it cannot confirm.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("Testy accepts one of the modes it published");

    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .modes
            .as_ref()
            .expect("Testy published modes")
            .current_mode_id
            .to_string(),
        "chat",
        "the surface stays at the last mode the *agent* stated: showing the \
         requested one would be the inspector stating a configuration its agent \
         never claimed"
    );
    assert_eq!(
        settings.mode_change,
        Some(ModeChange::Acknowledged(v1::SessionModeId::new("plan"))),
        "and the set is reported as what it was — acknowledged, and not restated"
    );

    // The evidence for all of that, which is the point of the tool: a surface
    // saying a set was acknowledged is only as good as the frames behind it.
    assert!(
        sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""method":"session/set_mode""#)
                && frame.contains(r#""modeId":"plan""#)),
        "the ask, with the mode it asked for: {:?}",
        sent(&inspector)
    );
    assert!(
        said_at(&inspector, r#""result":{}"#).is_some(),
        "and the empty answer that is the whole of what the agent said back: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn a_set_config_option_is_rebuilt_from_the_set_the_agent_answered_with() {
    // The other half of the ring's asymmetry (§7.6): `session/set_config_option`
    // answers with the **complete replacement option set**, so there is nothing
    // to infer and nothing to compute — the store is rebuilt from the list the
    // agent returned. Testy publishes one select option with three values and
    // answers a set with its whole list.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    assert_eq!(
        selected(&inspector.session_settings(), "verbosity"),
        "normal"
    );

    inspector
        .set_config_option(
            &v1::SessionConfigId::new("verbosity"),
            &v1::SessionConfigOptionValue::value_id("verbose"),
        )
        .await
        .expect("Testy accepts a value it published");

    let settings = inspector.session_settings();
    assert_eq!(
        selected(&settings, "verbosity"),
        "verbose",
        "the value the agent's own answer says the option is set to: {settings:?}"
    );
    assert_eq!(
        settings.config_refusal, None,
        "and nothing was refused: {settings:?}"
    );

    // The evidence, which is the point of the tool: the ask names the option
    // under the parameter the schema requires and carries a bare value id.
    let ask = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""method":"session/set_config_option""#))
        .unwrap_or_else(|| panic!("the ask is in the trace: {:?}", sent(&inspector)));
    assert!(ask.contains(r#""configId":"verbosity""#), "{ask}");
    assert!(ask.contains(r#""value":"verbose""#), "{ask}");
    assert!(
        !ask.contains(r#""type":"#),
        "a select's value goes out as a bare value id, with no discriminator over it: {ask}"
    );
    assert!(
        said_at(&inspector, r#""currentValue":"verbose""#).is_some(),
        "and the answer the store was rebuilt from is in the trace beside it: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn an_answer_that_moved_more_than_the_option_that_was_set_is_reflected_whole() {
    // The rule stated as broadly as it is coded (§7.6): the inspector never
    // merges, patches or computes an option set locally. An agent is entitled
    // to answer a single write with a wholly different set — and if it does,
    // that is what the surface shows, because it is what the agent said about
    // its own session.
    let inspector = Inspector::new();
    inspector
        .start(&offering(&[ANSWERED_ELSEWHERE]))
        .await
        .expect("the agent opens a session");
    assert_eq!(
        selected(&inspector.session_settings(), "verbosity"),
        "normal"
    );

    inspector
        .set_config_option(
            &v1::SessionConfigId::new("verbosity"),
            &v1::SessionConfigOptionValue::value_id("verbose"),
        )
        .await
        .expect("the agent answers the set");

    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .config_options
            .as_ref()
            .expect("the answer carried a set")
            .iter()
            .map(|option| option.id.to_string())
            .collect::<Vec<_>>(),
        ["model"],
        "the list the agent returned, whole — including the disappearance of the \
         option that was set: {settings:?}"
    );
    assert_eq!(
        selected(&settings, "model"),
        "haiku",
        "with the value it returned for it"
    );
}

#[tokio::test]
async fn a_boolean_config_option_goes_out_with_the_type_discriminator_the_schema_requires() {
    // What this client's own claim buys (§7.3, §7.6). A boolean option exists
    // on the wire at all only because the inspector advertised
    // `clientCapabilities.session.configOptions.boolean`, and its write is not
    // a select's with a different value in it: it carries a `type`
    // discriminator naming the *shape of the value*, beside the value itself.
    // A bare value id in its place is a shape a real client got wrong in a way
    // that fails against a real agent, which is why the payload is asserted on
    // the wire rather than taken on the type system's word.
    let inspector = Inspector::new();
    inspector
        .start(&toggling(&[TURNED_ON]))
        .await
        .expect("the agent opens a session");
    assert!(
        !switched(&inspector.session_settings(), "fast"),
        "the agent published it turned off"
    );

    inspector
        .set_config_option(
            &v1::SessionConfigId::new("fast"),
            &v1::SessionConfigOptionValue::boolean(true),
        )
        .await
        .expect("the agent answers the set");

    let ask = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""method":"session/set_config_option""#))
        .unwrap_or_else(|| panic!("the ask is in the trace: {:?}", sent(&inspector)));
    assert!(ask.contains(r#""configId":"fast""#), "{ask}");
    assert!(
        ask.contains(r#""type":"boolean""#),
        "with the discriminator that says what shape the value is: {ask}"
    );
    assert!(
        ask.contains(r#""value":true"#),
        "and the boolean itself, unquoted — not the string a value id would be: {ask}"
    );
}

#[tokio::test]
async fn a_set_boolean_config_option_is_rebuilt_from_the_set_the_agent_answered_with() {
    // The select's rule reached by the other route, and it is the same rule
    // (§7.6): `session/set_config_option` answers with the **complete
    // replacement option set** whatever the kind of the option written, so the
    // store is rebuilt from the list the agent returned. This agent answers a
    // single write with the option flipped *and* one it had never published,
    // which is a set that cannot be arrived at by patching the option that was
    // asked about.
    let inspector = Inspector::new();
    inspector
        .start(&toggling(&[TURNED_ON]))
        .await
        .expect("the agent opens a session");
    assert_eq!(published_options(&inspector.session_settings()), ["fast"]);

    inspector
        .set_config_option(
            &v1::SessionConfigId::new("fast"),
            &v1::SessionConfigOptionValue::boolean(true),
        )
        .await
        .expect("the agent answers the set");

    let settings = inspector.session_settings();
    assert_eq!(
        published_options(&settings),
        ["fast", "thinking"],
        "the list the agent returned, whole: {settings:?}"
    );
    assert!(
        switched(&settings, "fast"),
        "with the value its own answer says the option is set to: {settings:?}"
    );
    assert_eq!(
        settings.config_refusal, None,
        "and nothing was refused: {settings:?}"
    );
    assert!(
        said_at(&inspector, r#""currentValue":true"#).is_some(),
        "the answer the store was rebuilt from is in the trace beside the ask: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn a_refused_boolean_config_option_costs_the_attempt_and_nothing_else() {
    // A rejected boolean set behaves as a rejected select set does (§7.6), and
    // it is the same code path saying so: nothing was applied optimistically,
    // so the option is at the value the agent last stated, and what the agent
    // said about the ask is beside the surface rather than left in the trace
    // for the reader to find. The refusal names the value that was asked for,
    // which for this kind is a boolean rather than a value id.
    let inspector = Inspector::new();
    inspector
        .start(&toggling(&[BOOLEAN_REFUSED]))
        .await
        .expect("the agent opens a session");

    let refused = inspector
        .set_config_option(
            &v1::SessionConfigId::new("fast"),
            &v1::SessionConfigOptionValue::boolean(true),
        )
        .await
        .expect_err("the agent refuses the set");
    assert!(
        matches!(&refused, CallError::Rejected(error)
            if error.code == v1::ErrorCode::InvalidParams),
        "the agent's own refusal, carried whole: {refused:?}"
    );

    let settings = inspector.session_settings();
    assert!(
        !switched(&settings, "fast"),
        "the option is at the value the agent last stated: {settings:?}"
    );
    assert_eq!(
        settings.config_refusal,
        Some(acp_inspector_core::ConfigRefusal {
            option: v1::SessionConfigId::new("fast"),
            value: v1::SessionConfigOptionValue::boolean(true),
            error: refused,
        }),
        "and what the agent said about the ask is beside it: {settings:?}"
    );

    assert!(
        position(&inspector, r#""method":"session/set_config_option""#).is_some(),
        "the ask is in the trace: {:?}",
        sent(&inspector)
    );
    assert!(
        said_at(&inspector, "cannot be turned on").is_some(),
        "and so is the refusal — evidence, not a disappearance: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn a_boolean_payload_at_an_agent_that_published_a_select_is_refused_like_any_other_set() {
    // Why the boolean cases above are scripted, driven rather than assumed
    // (§12): Testy publishes exactly one config option and it is a select, so
    // the one path this ring's payload shape exists for is a path Testy cannot
    // put the inspector on. What it does with the payload anyway is a refusal
    // like any other — `invalid params`, naming the value it could not take —
    // which is the shape a *conformant* agent answers a write it did not
    // publish an option for, and it costs the attempt and nothing else.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let refused = inspector
        .set_config_option(
            &v1::SessionConfigId::new("verbosity"),
            &v1::SessionConfigOptionValue::boolean(true),
        )
        .await
        .expect_err("Testy refuses a shape its option cannot take");
    assert!(
        matches!(&refused, CallError::Rejected(error)
            if error.code == v1::ErrorCode::InvalidParams),
        "the agent's own refusal, carried whole: {refused:?}"
    );

    let settings = inspector.session_settings();
    assert_eq!(
        selected(&settings, "verbosity"),
        "normal",
        "the option is at the value the agent last stated: {settings:?}"
    );
    assert_eq!(
        settings.config_refusal,
        Some(acp_inspector_core::ConfigRefusal {
            option: v1::SessionConfigId::new("verbosity"),
            value: v1::SessionConfigOptionValue::boolean(true),
            error: refused,
        }),
        "and the refusal names the ask as it went out, boolean and all: {settings:?}"
    );
}

#[tokio::test]
async fn an_option_kind_that_is_neither_select_nor_boolean_never_reaches_this_crate() {
    // §15 q10, driven rather than read — which is what settles whether the
    // read-only row on the surface is a fixture a scripted agent can arm or a
    // forward-compatibility arm (§7.6).
    //
    // It is the arm. `SessionConfigKind` is an internally-tagged enum with
    // exactly `select` and `boolean` and no catch-all, and every option list on
    // the wire is deserialized with the item-skipping combinator — so an option
    // whose `type` is a *third* string does not arrive as an unknown kind. It
    // vanishes from the list before this crate sees it, and the option beside
    // it survives.
    //
    // Which is the limit §7.6 already names rather than a frame going missing:
    // the trace has what the agent sent, complete, and the surface says it is a
    // decoded view (§8).
    let inspector = Inspector::new();
    inspector
        .start(&publishing(&[THIRD_KIND], &[]))
        .await
        .expect("the agent opens a session");

    let settings = inspector.session_settings();
    assert_eq!(
        published_options(&settings),
        ["verbosity"],
        "the option this client could read, and only it: {settings:?}"
    );
    assert!(
        said_at(&inspector, r#""type":"slider""#).is_some(),
        "and the one it could not is in the trace, whole: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn a_refused_mode_costs_the_attempt_and_nothing_else() {
    // Nothing was applied optimistically, so nothing is rolled back (§7.6): the
    // row is where the agent's last statement left it, and the refusal is
    // reported beside it rather than left in the trace for the reader to find.
    // Testy refuses a mode id it does not publish, with invalid params.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    let refused = inspector
        .set_mode(&v1::SessionModeId::new("architect"))
        .await
        .expect_err("Testy refuses a mode it never published");

    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .modes
            .as_ref()
            .expect("Testy published modes")
            .current_mode_id
            .to_string(),
        "chat",
        "the mode is the one the agent last stated, refusal or no refusal"
    );
    assert_eq!(
        settings.mode_change,
        Some(ModeChange::Refused {
            mode: v1::SessionModeId::new("architect"),
            error: refused,
        }),
        "and what the agent said about it is beside the surface: {settings:?}"
    );

    assert!(
        position(&inspector, r#""method":"session/set_mode""#).is_some(),
        "the ask is in the trace: {:?}",
        sent(&inspector)
    );
    assert!(
        said_at(&inspector, "unsupported mode").is_some(),
        "and so is the refusal — evidence, not a disappearance: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn a_refused_config_option_costs_the_attempt_and_nothing_else() {
    // Ring 4's rule for the other setter, and the same two refusals Testy
    // answers with: an option it does not publish, and a value the option it
    // does publish cannot take (§7.6). Nothing was applied optimistically, so
    // nothing is rolled back — the row is at the value the agent last stated,
    // and what it said about the ask is beside the surface rather than left in
    // the trace for the reader to find.
    for (option, value, said) in [
        ("colour", "green", OPTION_REFUSED),
        ("verbosity", "shouting", VALUE_REFUSED),
    ] {
        let inspector = Inspector::new();
        inspector.start(&agent()).await.expect("Testy starts");

        let refused = inspector
            .set_config_option(
                &v1::SessionConfigId::new(option),
                &v1::SessionConfigOptionValue::value_id(value),
            )
            .await
            .expect_err("Testy refuses what it never published");
        assert!(
            matches!(&refused, CallError::Rejected(error)
                if error.code == v1::ErrorCode::InvalidParams),
            "the agent's own refusal, carried whole: {refused:?}"
        );

        let settings = inspector.session_settings();
        assert_eq!(
            selected(&settings, "verbosity"),
            "normal",
            "the option is at the value the agent last stated: {settings:?}"
        );
        assert_eq!(
            settings.config_refusal,
            Some(acp_inspector_core::ConfigRefusal {
                option: v1::SessionConfigId::new(option),
                value: v1::SessionConfigOptionValue::value_id(value),
                error: refused,
            }),
            "and what the agent said about the ask is beside it: {settings:?}"
        );

        assert!(
            position(&inspector, r#""method":"session/set_config_option""#).is_some(),
            "the ask is in the trace: {:?}",
            sent(&inspector)
        );
        assert!(
            said_at(&inspector, said).is_some(),
            "and so is the refusal — evidence, not a disappearance: {:?}",
            received(&inspector)
        );
    }
}

#[tokio::test]
async fn switching_sessions_leaves_no_ask_behind() {
    // Switching discards and rebuilds the settings, and what this client asked
    // for goes with them (§7.6): a set acknowledged in the session that was
    // left says nothing about the one that is open now.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("Testy accepts it");
    inspector
        .set_config_option(
            &v1::SessionConfigId::new("colour"),
            &v1::SessionConfigOptionValue::value_id("green"),
        )
        .await
        .expect_err("and refuses an option it never published");
    assert!(inspector.session_settings().mode_change.is_some());
    assert!(inspector.session_settings().config_refusal.is_some());

    inspector
        .new_session("/tmp", None)
        .await
        .expect("Testy opens another session");

    let settings = inspector.session_settings();
    assert_eq!(
        settings.mode_change, None,
        "the new session has been asked for nothing: {settings:?}"
    );
    assert_eq!(
        settings.config_refusal, None,
        "and nothing has been refused in it either: {settings:?}"
    );
    assert!(
        settings.modes.is_some(),
        "and it published modes of its own, which is what refilled the surface"
    );
}

#[tokio::test]
async fn a_set_answered_after_the_session_was_left_says_nothing_about_the_new_one() {
    // The settings are the *live* session's, so an answer that arrives after
    // the switch reaches whoever asked for it and reaches nothing on screen —
    // the rule every store above the connection follows (§7.5).
    let inspector = Inspector::new();
    let agent = shell(&format!(
        "{}{}read _; {}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[MODES]).as_str()]),
        // The `session/new` first, and only then the answer to the set it was
        // asked before it.
        exchange(&[
            r#"{"jsonrpc":"2.0","id":4,"result":{"sessionId":"s2",MODES_HERE}}"#,
            MODE_SET,
        ])
        .replace("MODES_HERE", MODES),
    ));
    inspector.start(&agent).await.expect("it opens a session");

    let setting = tokio::spawn({
        let inspector = inspector.clone();
        async move { inspector.set_mode(&v1::SessionModeId::new("plan")).await }
    });
    let mut frames = inspector.trace().changes();
    until(&mut frames, "the set to reach the agent", || {
        position(&inspector, r#""method":"session/set_mode""#).is_some()
    })
    .await;

    inspector
        .new_session("/tmp", None)
        .await
        .expect("the agent opens another session");
    setting
        .await
        .expect("the set task")
        .expect("it is answered");

    let settings = inspector.session_settings();
    assert_eq!(
        settings.mode_change, None,
        "the ask belonged to the session that was left: {settings:?}"
    );
}

#[tokio::test]
async fn an_option_set_answered_after_the_session_was_left_rebuilds_nothing() {
    // The same rule for the setter whose answer is a whole option set, where it
    // costs more: an answer taken after the switch would rebuild the *new*
    // session's options out of the old session's answer, which is the surface
    // describing a session that is not open (§7.5, §7.6).
    let inspector = Inspector::new();
    let agent = shell(&format!(
        "{}{}read _; {}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[OPTIONS]).as_str()]),
        // The `session/new` first, and only then the answer to the set it was
        // asked before it.
        exchange(&[
            r#"{"jsonrpc":"2.0","id":4,"result":{"sessionId":"s2",OPTIONS_HERE}}"#,
            ANSWERED_ELSEWHERE,
        ])
        .replace("OPTIONS_HERE", OPTIONS),
    ));
    inspector.start(&agent).await.expect("it opens a session");

    let setting = tokio::spawn({
        let inspector = inspector.clone();
        async move {
            inspector
                .set_config_option(
                    &v1::SessionConfigId::new("verbosity"),
                    &v1::SessionConfigOptionValue::value_id("verbose"),
                )
                .await
        }
    });
    let mut frames = inspector.trace().changes();
    until(&mut frames, "the set to reach the agent", || {
        position(&inspector, r#""method":"session/set_config_option""#).is_some()
    })
    .await;

    inspector
        .new_session("/tmp", None)
        .await
        .expect("the agent opens another session");
    setting
        .await
        .expect("the set task")
        .expect("it is answered");

    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .config_options
            .as_ref()
            .expect("the new session published options of its own")
            .iter()
            .map(|option| option.id.to_string())
            .collect::<Vec<_>>(),
        ["verbosity"],
        "the new session's own answer, and not the answer to an ask made in the \
         one that was left: {settings:?}"
    );
}

#[tokio::test]
async fn a_mode_can_be_set_while_a_turn_is_in_flight() {
    // **Affordances gate on the advertisement, never on state** (§7.6). The
    // specification is silent on what an agent should do with a change it was
    // not expecting, and an inspector that withheld the call would be deciding
    // on the agent's behalf what may be observed — so it is sent, and what
    // comes back is reported. What Testy does with one: it answers the set from
    // the same connection while its prompt goes on waiting, and says nothing
    // about the mode afterwards.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    let turn = prompting(&inspector, WAIT_FOR_CANCEL);
    running(&inspector).await;

    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("Testy answers a set with a turn of its own still running");

    assert_eq!(
        inspector.session_settings().mode_change,
        Some(ModeChange::Acknowledged(v1::SessionModeId::new("plan"))),
        "reported the same way as any other set: acknowledged, and not restated"
    );
    assert!(
        inspector.turn().is_running(),
        "and the turn it landed in is still the turn that was running: {:?}",
        inspector.turn()
    );

    inspector.cancel().await.expect("the turn is asked to stop");
    assert_eq!(
        turn.await.expect("the prompt task"),
        Ok(v1::StopReason::Cancelled),
        "which then ends the way it was going to end"
    );
}

#[tokio::test]
async fn a_config_option_can_be_set_while_a_turn_is_in_flight() {
    // **Affordances gate on the advertisement, never on state** (§7.6), for
    // this setter on ring 4's reasoning exactly: the specification is silent on
    // what an agent should do with a change it was not expecting, and an
    // inspector that withheld the call would be deciding on the agent's behalf
    // what may be observed. What Testy does with one is driven rather than
    // guessed — it answers the set from the same connection, with its whole
    // option set, and its prompt goes on waiting.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    let turn = prompting(&inspector, WAIT_FOR_CANCEL);
    running(&inspector).await;

    inspector
        .set_config_option(
            &v1::SessionConfigId::new("verbosity"),
            &v1::SessionConfigOptionValue::value_id("brief"),
        )
        .await
        .expect("Testy answers a set with a turn of its own still running");

    assert_eq!(
        selected(&inspector.session_settings(), "verbosity"),
        "brief",
        "reported the same way as any other set: from the set the agent answered with"
    );
    assert!(
        inspector.turn().is_running(),
        "and the turn it landed in is still the turn that was running: {:?}",
        inspector.turn()
    );

    inspector.cancel().await.expect("the turn is asked to stop");
    assert_eq!(
        turn.await.expect("the prompt task"),
        Ok(v1::StopReason::Cancelled),
        "which then ends the way it was going to end"
    );
}

#[tokio::test]
async fn a_config_option_set_at_an_agent_that_has_gone_says_so_beside_the_surface() {
    // The settings outlive the connection that filled them (§5), so a control
    // pressed after the agent has left has to read as an attempt that failed
    // rather than as a button with nothing behind it — and an answer that will
    // never arrive is one of those failures, refused by the teardown the way a
    // turn that was running is ended by it (§6.1).
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector.disconnect();

    let failure = inspector
        .set_config_option(
            &v1::SessionConfigId::new("verbosity"),
            &v1::SessionConfigOptionValue::value_id("brief"),
        )
        .await
        .expect_err("there is nobody to ask");

    assert_eq!(failure, CallError::Disconnected);
    let settings = inspector.session_settings();
    assert_eq!(
        settings.config_refusal,
        Some(acp_inspector_core::ConfigRefusal {
            option: v1::SessionConfigId::new("verbosity"),
            value: v1::SessionConfigOptionValue::value_id("brief"),
            error: CallError::Disconnected,
        }),
        "and the surface says so: {settings:?}"
    );
    assert_eq!(
        selected(&settings, "verbosity"),
        "normal",
        "beside the options the agent published, which are still what it said"
    );
}

#[tokio::test]
async fn a_config_option_set_the_agent_dies_under_is_refused_by_the_teardown() {
    let inspector = Inspector::new();
    let agent = shell(&format!(
        "{}{}read _;",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[OPTIONS]).as_str()]),
    ));
    inspector.start(&agent).await.expect("it opens a session");

    inspector
        .set_config_option(
            &v1::SessionConfigId::new("verbosity"),
            &v1::SessionConfigOptionValue::value_id("brief"),
        )
        .await
        .expect_err("the agent read the set and left without answering");

    let settings = inspector.session_settings();
    assert_eq!(
        settings.config_refusal,
        Some(acp_inspector_core::ConfigRefusal {
            option: v1::SessionConfigId::new("verbosity"),
            value: v1::SessionConfigOptionValue::value_id("brief"),
            error: CallError::Disconnected,
        }),
        "{settings:?}"
    );
}

#[tokio::test]
async fn a_mode_set_at_an_agent_that_has_gone_says_so_beside_the_surface() {
    // The settings outlive the connection that filled them (§5), so a surface
    // is still describing the session after its agent has left — and a control
    // pressed there has to read as an attempt that failed rather than as a
    // button with nothing behind it. The same rule `authenticate` follows.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector.disconnect();

    let failure = inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect_err("there is nobody to ask");

    assert_eq!(failure, CallError::Disconnected);
    let settings = inspector.session_settings();
    assert_eq!(
        settings.mode_change,
        Some(ModeChange::Refused {
            mode: v1::SessionModeId::new("plan"),
            error: CallError::Disconnected,
        }),
        "and the surface says so: {settings:?}"
    );
    assert!(
        settings.modes.is_some(),
        "beside the modes the agent published, which are still what it said"
    );
}

#[tokio::test]
async fn a_set_the_agent_dies_under_is_refused_by_the_teardown() {
    // An answer that will never arrive is not a control left waiting: the
    // connection ending is what says so, the same way it ends a turn that was
    // in flight (§6.1).
    let inspector = Inspector::new();
    let agent = shell(&format!(
        "{}{}read _;",
        exchange(&[DESCRIBED]),
        exchange(&[opened(&[MODES]).as_str()]),
    ));
    inspector.start(&agent).await.expect("it opens a session");

    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect_err("the agent read the set and left without answering");

    let settings = inspector.session_settings();
    assert_eq!(
        settings.mode_change,
        Some(ModeChange::Refused {
            mode: v1::SessionModeId::new("plan"),
            error: CallError::Disconnected,
        }),
        "{settings:?}"
    );
}

#[tokio::test]
async fn an_announcement_does_not_take_back_a_refusal() {
    // What an announcement settles is a set the agent never *followed up on*,
    // and a refusal is not one: the agent answered it, and that it answered no
    // stays true however many modes it states afterwards (§7.6). Erasing it
    // would leave a user who asked for a mode they cannot have with nothing on
    // screen to say so.
    let inspector = Inspector::new();
    inspector
        .start(&setting(&[MODE_REFUSED, &said(NOW_PLANNING)]))
        .await
        .expect("the agent opens a session");

    let refused = inspector
        .set_mode(&v1::SessionModeId::new("architect"))
        .await
        .expect_err("the agent refuses a mode it never published");

    let mut changes = inspector.session_settings_changes();
    until(&mut changes, "the announcement that follows it", || {
        inspector
            .session_settings()
            .modes
            .is_some_and(|modes| modes.current_mode_id.to_string() == "plan")
    })
    .await;

    assert_eq!(
        inspector.session_settings().mode_change,
        Some(ModeChange::Refused {
            mode: v1::SessionModeId::new("architect"),
            error: refused,
        }),
        "the refusal is still what the agent said about the ask"
    );
}

#[tokio::test]
async fn a_current_mode_update_after_the_set_settles_the_surface() {
    // The agent that *does* announce its change being rewarded with a surface
    // that agrees with it (§7.6): the mode moves to the one it announced, and
    // there is nothing left unconfirmed to say about the ask.
    let inspector = Inspector::new();
    inspector
        .start(&setting(&[MODE_SET, &said(NOW_PLANNING)]))
        .await
        .expect("the agent opens a session");

    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("the agent answers the set");

    let mut changes = inspector.session_settings_changes();
    until(&mut changes, "the announced mode to settle", || {
        inspector
            .session_settings()
            .modes
            .is_some_and(|modes| modes.current_mode_id.to_string() == "plan")
    })
    .await;

    assert_eq!(
        inspector.session_settings().mode_change,
        None,
        "the agent stated a mode, so nothing about the set is unconfirmed"
    );
}

#[tokio::test]
async fn switching_sessions_discards_the_settings_on_every_path_that_opens_one() {
    // Switching discards and rebuilds, the same rule as the timeline and for
    // the same reason: what is on screen describes the session that is open
    // (§7.6). There is no carry-over and no merge — a session that published
    // nothing leaves a surface saying so, however much the session before it
    // published — and it is one rule for all three ways of opening a session,
    // because they are one thing to a reader.
    for how in [None, Some(Restore::Load), Some(Restore::Resume)] {
        let answer = match how {
            None => opened_again(&[]),
            Some(_) => reopened(&[]),
        };
        let inspector = Inspector::new();
        inspector
            .start(&reopening(&[answer.as_str()]))
            .await
            .expect("a session to leave");
        assert!(
            !inspector.session_settings().is_empty(),
            "the session being left published both halves"
        );

        let opened = match how {
            None => inspector.new_session("/tmp", None).await,
            Some(how) => {
                inspector
                    .restore_session(&v1::SessionInfo::new("s2", "/tmp"), how, None)
                    .await
            }
        };
        opened.expect("the agent opens another session");

        let settings = inspector.session_settings();
        assert!(
            settings.is_empty(),
            "nothing of the session that was left survives into the one that is open: {settings:?}"
        );
    }
}

#[tokio::test]
async fn testy_publishes_its_modes_on_every_path_that_opens_a_session() {
    // The record of the question this ring answered by driving rather than by
    // reading: whether the upstream test agent can be made to publish modes on
    // one setup path and omit them on another. **It cannot** — it answers
    // `session/new`, `session/load` and `session/resume` with the same two
    // fields, off the same session state — so the case where a replay states a
    // mode the answer never offered is a scripted agent's, and
    // `a_replay_that_states_a_mode_the_answer_did_not_offer_yields_no_mode_control`
    // is where it is driven. Testy replays nothing either, which is the same
    // answer reached twice.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    assert!(
        inspector.session_settings().modes.is_some(),
        "on the session it creates"
    );

    for how in [Restore::Load, Restore::Resume] {
        inspector
            .restore_session(&v1::SessionInfo::new(session.clone(), "/tmp"), how, None)
            .await
            .unwrap_or_else(|error| panic!("Testy answers {}: {error}", how.method()));

        let settings = inspector.session_settings();
        assert!(
            settings.modes.is_some(),
            "and on the one it reopens with {}: {settings:?}",
            how.method()
        );
        assert_eq!(
            settings.unoffered_mode, None,
            "an agent that published the modes it states has nothing to explain: {settings:?}"
        );
    }
}

#[tokio::test]
async fn a_session_created_beside_a_configured_one_is_configured_as_itself() {
    // The discard rule driven against a real agent rather than a scripted one:
    // Testy remembers the mode a session was set to, so a second session that
    // opens in the mode it publishes for itself is the surface describing the
    // session that is open rather than the one that was left (§7.6).
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");
    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("Testy accepts the set");

    inspector
        .new_session("/tmp", None)
        .await
        .expect("Testy opens another session");

    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .modes
            .as_ref()
            .expect("the new session published modes of its own")
            .current_mode_id
            .to_string(),
        "chat",
        "the mode the session that is open is in, not the one the session that was left was set \
         to: {settings:?}"
    );
}

#[tokio::test]
async fn a_replay_that_states_a_mode_the_answer_did_not_offer_yields_no_mode_control() {
    // The case two of the ring's rules collide in (§7.6). `session/load` MUST
    // replay before it answers, so the announcement crosses first and the
    // answer that says whether any modes were published crosses last — and the
    // answer wins, absolutely: no advertisement, no affordance. Inferring one
    // from replayed traffic would be the inspector deciding on the agent's
    // behalf what it supports.
    let inspector = Inspector::new();
    inspector
        .start(&reopening(&[&said(NOW_PLANNING), &reopened(&[OPTIONS])]))
        .await
        .expect("a session to leave");

    inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Load, None)
        .await
        .expect("the agent loads it");

    let settings = inspector.session_settings();
    assert_eq!(
        settings.modes, None,
        "the answer published none, so there is nothing to gate a mode control on: {settings:?}"
    );
    assert_eq!(
        settings.unoffered_mode,
        Some(v1::SessionModeId::new("plan")),
        "and the asymmetry is named rather than swallowed: {settings:?}"
    );
    assert!(
        settings.config_options.is_some(),
        "the half the answer did publish is there: {settings:?}"
    );
}

#[tokio::test]
async fn a_mode_stated_ahead_of_a_resume_that_offered_none_is_named_the_same_way() {
    // The same two frames in the same order under the call that MUST NOT
    // replay at all (§7.5). An agent that did it under a `session/resume` did
    // what the load rule is about with a rule broken on top, so the store says
    // what crossed and leaves which call it was to the trace — a surface that
    // only noticed it under `session/load` would be reading the agent's
    // intentions rather than its frames.
    let inspector = Inspector::new();
    inspector
        .start(&reopening(&[&said(NOW_PLANNING), &reopened(&[])]))
        .await
        .expect("a session to leave");

    inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Resume, None)
        .await
        .expect("the agent resumes it");

    let settings = inspector.session_settings();
    assert_eq!(settings.modes, None, "no mode control: {settings:?}");
    assert_eq!(
        settings.unoffered_mode,
        Some(v1::SessionModeId::new("plan")),
        "and the same thing said about it: {settings:?}"
    );
}

#[tokio::test]
async fn a_replay_into_a_load_that_is_then_refused_leaves_the_live_session_where_it_was() {
    // The rule the ring is named for, in the case that would break it: a
    // session that could not be opened was not opened, so the session that is
    // still live is still what the surface describes (§7.5). The replay is the
    // *opening* session's account of itself — the answer that never came is
    // what would have made it the live one — so applying it here would leave a
    // user looking at a mode their session never stated, and looking at it for
    // good rather than in passing.
    let inspector = Inspector::new();
    inspector
        .start(&reopening(&[&said_of("s2", NOW_PLANNING), REOPEN_REFUSED]))
        .await
        .expect("the session that stays live");

    inspector
        .restore_session(&v1::SessionInfo::new("s2", "/tmp"), Restore::Load, None)
        .await
        .expect_err("the agent refuses to hand the other one back");

    assert_eq!(
        inspector.session().map(|id| id.to_string()),
        Some("s1".to_owned()),
        "the session it could not replace is still the live one"
    );
    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .modes
            .as_ref()
            .expect("which published modes of its own")
            .current_mode_id
            .to_string(),
        "chat",
        "and it is in the mode it stated, not the one another session was replayed in: {settings:?}"
    );
}

#[tokio::test]
async fn a_mode_stated_in_the_session_being_left_is_not_the_replay_of_the_one_being_opened() {
    // The rule is about a *replay*, which is what a load's own session id says
    // it is (§7.6): the session being walked away from is entitled to go on
    // talking — the last words of a turn the switch just cancelled, say — and
    // reporting those as a mode the opening session never offered would be the
    // inspector naming an asymmetry the agent never produced.
    let inspector = Inspector::new();
    inspector
        .start(&reopening(&[&said(NOW_PLANNING), &reopened(&[])]))
        .await
        .expect("a session to leave");

    inspector
        .restore_session(&v1::SessionInfo::new("s2", "/tmp"), Restore::Load, None)
        .await
        .expect("the agent loads the other one");

    let settings = inspector.session_settings();
    assert_eq!(
        settings.modes, None,
        "the answer published none either way: {settings:?}"
    );
    assert_eq!(
        settings.unoffered_mode, None,
        "and the mode that was stated was stated in another session: {settings:?}"
    );
}

#[tokio::test]
async fn the_replayed_announcement_the_answer_did_not_offer_is_still_on_the_timeline() {
    // The surface says what a setting *is* and the timeline says what the agent
    // said (§7.6). A mode the answer did not offer is not a control, and it is
    // still an announcement that happened — so it is where announcements are,
    // in the view of the session that replayed it.
    let inspector = Inspector::new();
    inspector
        .start(&reopening(&[&said(NOW_PLANNING), &reopened(&[])]))
        .await
        .expect("a session to leave");

    inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Load, None)
        .await
        .expect("the agent loads it");

    let announced = inspector
        .timeline()
        .entries()
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                EntryKind::Update(v1::SessionUpdate::CurrentModeUpdate(_))
            )
        })
        .count();
    assert_eq!(
        announced, 1,
        "the replay is in the view it was replayed into"
    );
}

#[tokio::test]
async fn every_frame_of_the_session_that_was_left_is_still_in_the_trace() {
    // A switch costs a view and never the evidence (§7.5): the settings the
    // session that was left published are gone from the surface, and the frame
    // that published them is exactly where it crossed.
    let inspector = Inspector::new();
    inspector
        .start(&reopening(&[&said(NOW_PLANNING), &reopened(&[])]))
        .await
        .expect("a session to leave");

    inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Load, None)
        .await
        .expect("the agent loads it");

    assert!(
        inspector.session_settings().is_empty(),
        "the session that is open published nothing: {:?}",
        inspector.session_settings()
    );
    assert!(
        said_at(&inspector, r#""sessionId":"s1""#).is_some(),
        "and the answer that opened the session that was left is still in the trace: {:?}",
        received(&inspector)
    );
    assert!(
        received(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""currentModeId":"chat""#)),
        "including the modes it published, which the surface no longer shows: {:?}",
        received(&inspector)
    );
}

#[tokio::test]
async fn a_mode_announced_before_the_answer_is_not_reported_as_unrestated() {
    // An agent may announce in the breath *before* it answers, and the one task
    // reading the connection has both frames in wire order. A set marked
    // acknowledged-and-unconfirmed afterwards would be contradicting an
    // announcement already read (§7.6).
    let inspector = Inspector::new();
    inspector
        .start(&setting(&[&said(NOW_PLANNING), MODE_SET]))
        .await
        .expect("the agent opens a session");

    inspector
        .set_mode(&v1::SessionModeId::new("plan"))
        .await
        .expect("the agent answers the set");

    let settings = inspector.session_settings();
    assert_eq!(
        settings
            .modes
            .as_ref()
            .expect("the agent published modes")
            .current_mode_id
            .to_string(),
        "plan",
        "the mode the agent announced"
    );
    assert_eq!(
        settings.mode_change, None,
        "and it announced it, so there is nothing unrestated to report: {settings:?}"
    );
}
