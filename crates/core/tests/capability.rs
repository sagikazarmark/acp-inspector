//! What the agent claims about itself, and what the inspector does with the
//! claim (`docs/architecture.md` §7.1, §9): the store behind the capability
//! display, the gate the `session/list` affordance sits behind, authentication
//! as a state the screen can render rather than a failure it has to survive, and
//! the record of what happened when an advertisement was taken up (§7.7).
//!
//! Testy answers the conformant way — it advertises listing and an auth method,
//! and it authenticates — so the agents that do the other things are `/bin/sh`
//! one-liners: one that advertises nothing, one that pages, one that will not
//! open a session until somebody logs in. What is asserted is core's public
//! surface either way: the stores the display reads, the values the driven
//! methods answer with, and the frames that crossed (§12).

mod common;

use acp_inspector_core::{
    AgentCapability, AgentCommand, AuthState, CallError, ConnectionStatus, Driven, Inspector,
    Restore, TurnState, v1,
};

use common::{QUIET, advertising, listed, position, sent, shell, spoken_in, testy, until};

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

/// An agent that answers the calls this client makes, in order, and then holds
/// the connection open — so what a test waits for is an answer and never an
/// exit.
///
/// It answers by counting rather than by reading ids, which works because this
/// client numbers its calls from one, in order.
fn answering(results: &[&str]) -> AgentCommand {
    let script = results
        .iter()
        .map(|result| format!("read _; printf '%s\\n' '{result}'; "))
        .collect::<String>();

    AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\n{script}cat > /dev/null"),
        ..AgentCommand::default()
    }
}

#[tokio::test]
async fn the_listing_affordance_is_gated_on_the_advertisement() {
    // Gated on what the agent said it can do, not on whether there is anything
    // to show (§9): Testy with no session open yet still offers the button,
    // because the button is about the capability and the emptiness is about the
    // moment.
    let inspector = Inspector::new();
    assert!(
        !inspector.lists_sessions(),
        "nothing is advertised before an agent has said anything"
    );

    inspector.connect(&agent().factory());
    assert!(
        !inspector.lists_sessions(),
        "and nothing is advertised until `initialize` has answered"
    );

    inspector.initialize().await.expect("Testy initializes");

    assert!(
        inspector.lists_sessions(),
        "Testy advertises `sessionCapabilities.list`"
    );
    assert!(
        inspector.listing().is_none(),
        "and the affordance stands before anything has been listed"
    );
}

/// Every advertisement the record can be keyed on, read off an agent's own
/// account of itself the way the capability display reads them (§7.5, §7.7) —
/// the top-level boolean first, then the five whose claim is the presence of an
/// object under `sessionCapabilities`, and last the one claimed the same way
/// under `auth`.
///
/// **Core's list is [`AgentCapability`]**, since the fourth ring gave the record
/// of what was driven a key to hang off (§7.7) — so this reads that rather than
/// restating it, and the set stays two hand-written lists rather than three.
///
/// The display's own copy — the rows, their order and the field each is read
/// from — is pinned beside it, in `crates/app/src/agent.rs`'s
/// `every_session_capability_v1_defines_is_a_row` and
/// `the_two_gating_shapes_are_not_flattened_into_one`. It is not shared with
/// this one: core carries no rendering and the desktop crate carries no runtime
/// to drive an agent with (§5), so the two seams state the same set separately
/// and each fails on its own if a capability is dropped.
fn advertisements(described: &v1::InitializeResponse) -> Vec<(&'static str, bool)> {
    AgentCapability::ALL
        .into_iter()
        .map(|capability| {
            (
                capability.name(),
                capability.advertised(&described.agent_capabilities),
            )
        })
        .collect()
}

/// What this client advertised about *itself*, as it crossed the wire.
///
/// Read off the trace rather than off the value core builds it from, because
/// the claim is only a claim once an agent has been told it (§7.3, §7.6): a
/// constant that never reached an `initialize` request would advertise nothing
/// at all, and it is the request frame an agent gates its offer on.
fn claimed_by_this_client(inspector: &Inspector) -> Vec<serde_json::Value> {
    sent(inspector)
        .iter()
        .filter(|frame| frame.contains(r#""method":"initialize""#))
        .map(|frame| {
            let frame: serde_json::Value =
                serde_json::from_str(frame).expect("an initialize this client sent is JSON-RPC");
            frame["params"]["clientCapabilities"].clone()
        })
        .collect()
}

/// The whole of what this client claims, on every connection: boolean config
/// options, both elicitation modes, and the two services still declined.
///
/// Written out as the JSON an agent reads rather than assembled from the
/// builders core assembles it with — an expectation restated in core's own
/// terms would agree with core however wrong both were.
fn the_claim() -> serde_json::Value {
    serde_json::json!({
        "fs": {"readTextFile": false, "writeTextFile": false},
        "terminal": false,
        "session": {"configOptions": {"boolean": {}}},
        "elicitation": {"form": {}, "url": {}},
    })
}

#[tokio::test]
async fn the_client_advertises_what_it_will_honour_and_nothing_else() {
    // **What this client claims about itself** (§7.3, §7.6, §7.8): the data
    // shape that makes an agent offer this tool the option kind it offers a
    // real client, and the one service it performs — a form built from the
    // agent's own schema, and a URL handed to the reader's own browser. The
    // MVP's stance is narrowed twice over and not abandoned: `fs/*` and
    // `terminal/*` are still declined, in the same frame and to the same agent,
    // because those two are services this tool genuinely cannot perform.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");

    assert_eq!(
        claimed_by_this_client(&inspector),
        [the_claim()],
        "the claim is on the `initialize` frame in the trace, where an agent read it"
    );
}

#[tokio::test]
async fn every_connection_advertises_the_same_claim() {
    // **Fixed, with no way to turn it off** (§7.6): there is no spawn-form
    // switch and no per-connection state behind it, so a second agent is told
    // exactly what the first was. Driven rather than read, because "not
    // configurable" is a claim about what crosses the wire.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");

    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("and initializes again");

    assert_eq!(
        claimed_by_this_client(&inspector),
        [the_claim(), the_claim()],
        "two connections, and the same claim on both"
    );
}

/// The fields the schema crate defines on one of its capability objects, by the
/// names they carry on the wire.
///
/// Read off the JSON Schema the crate derives rather than off the Rust struct,
/// because that is the shape the question is about: a capability is a key an
/// agent may send, and a `#[cfg]`-gated field is absent from both the struct and
/// the schema unless its feature is on. Which features are on is the workspace's
/// answer and is decided by the canonical stable method list rather than by the
/// crate's packaging (§7.8, ADR 0008) — so `elicitation` is here, and everything
/// still behind an `unstable_*` feature is not, its traffic displayed rather
/// than decoded (§8).
fn defined<T: schemars::JsonSchema>() -> Vec<String> {
    let schema = schemars::schema_for!(T);
    let mut named: Vec<_> = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("a capability object has properties")
        .keys()
        .cloned()
        .collect();
    named.sort();
    named
}

#[test]
fn the_capability_set_the_display_names_is_the_whole_stable_set_v1_defines() {
    // **The one question no agent can be driven to answer.** Every other test
    // here asks an agent what it claimed; this one asks the protocol what there
    // is to claim. The display names its capabilities in a hand-written list
    // (`crates/app/src/agent.rs`'s `advertised`) and this one restates it — two
    // seams stating the same set separately (§5), which catches either of them
    // dropping a row but neither of them *missing* one, because a capability
    // upstream adds is absent from both by construction.
    //
    // Which is the failure the display exists to prevent, turned on itself: an
    // agent's answer about a capability nobody drew a row for reads as a
    // question nobody asked. `SessionCapabilities` is `#[non_exhaustive]`, so
    // no destructuring can be made to fail here — the schema is what knows.
    assert_eq!(
        defined::<v1::SessionCapabilities>(),
        [
            "_meta",
            "additionalDirectories",
            "close",
            "delete",
            "list",
            "resume"
        ],
        "a session capability v1 defines and the display does not name: add the row"
    );

    // And the shape the other half of the asymmetry is claimed in (§7.5). The
    // schema crate says the top-level `loadSession` boolean "will be unified in
    // future versions of the protocol" — so this is the tripwire for the day it
    // is, when the display's two-shape story stops being true and its caption
    // starts being a claim about a protocol that moved on.
    assert_eq!(
        defined::<v1::AgentCapabilities>(),
        [
            "_meta",
            "auth",
            "loadSession",
            "mcpCapabilities",
            "promptCapabilities",
            "sessionCapabilities"
        ],
        "the top-level capability set moved: the display's shapes are read from it"
    );

    // And the auth block, which the fourth ring turned from a row nothing could
    // reach into one an affordance drives (§7.7). It holds one claim today, so
    // a second one appearing is a capability the panel would draw no row for and
    // no button beside — the same failure as above, in the one group where a
    // missing row also means a missing affordance.
    assert_eq!(
        defined::<v1::AgentAuthCapabilities>(),
        ["_meta", "logout"],
        "an auth capability v1 defines and the display does not name: add the row"
    );
}

#[test]
fn the_record_is_keyed_on_the_session_lifecycle_and_logout_and_nothing_else() {
    // **Core's half of the two-list arrangement, pinned the way the display's
    // half is** (§7.7) — the display's own is
    // `every_session_capability_v1_defines_is_a_row`, and the question this
    // pair leaves to the schema is the test above. What is asserted here is
    // that core's list has not quietly lost a row.
    //
    // The record of what was driven keys on `AgentCapability`, so a row asks
    // about a capability by naming a variant rather than by matching a string:
    // a key matched loosely would render a capability upstream adds as
    // *unknown* instead of breaking a build, which is the exact failure the
    // tripwire above exists to prevent.
    //
    // Named against the wire's own names, which is where the two halves meet:
    // the display's rows say these, and so does this. `logout` is here for the
    // reason the six above it are — it is an advertisement something in this
    // tool can drive (§7.7) — and it is the one that is not a session's: the
    // call carries no session id, and the claim is made under `auth`.
    assert_eq!(
        AgentCapability::ALL.map(AgentCapability::name),
        [
            "session/load",
            "session/list",
            "session/resume",
            "session/close",
            "session/delete",
            "additionalDirectories",
            "logout",
        ],
        "a capability the record cannot be keyed on is one the panel cannot draw the fact for"
    );
}

#[test]
fn the_client_capability_set_the_display_names_is_the_whole_stable_set_v1_defines() {
    // The same question as the one above, asked about the *other* party (§7.6).
    // The display now audits both, and this client's own rows are a hand-written
    // list too (`crates/app/src/agent.rs`'s `claimed`) — so a client capability the
    // protocol adds is one this tool would be silently declining with nothing on
    // screen saying so, which for the party that is *this tool* is worse than
    // the agent case: the claim is ours to make.
    assert_eq!(
        defined::<v1::ClientCapabilities>(),
        ["_meta", "elicitation", "fs", "session", "terminal"],
        "a client capability v1 defines and the display does not name: add the row"
    );
    // The two modes, which are the two rows the display draws for the claim
    // (§7.8) — and the tripwire that says so if the protocol grows a third.
    assert_eq!(
        defined::<v1::ElicitationCapabilities>(),
        ["_meta", "form", "url"],
        "an elicitation mode v1 defines and this client does not answer: decide about the row"
    );
    assert_eq!(
        defined::<v1::FileSystemCapabilities>(),
        ["_meta", "readTextFile", "writeTextFile"],
        "the filesystem services this client declines: each is a row"
    );

    // And the one branch this client claims anything down: an object under an
    // object, whose presence is the claim at every step.
    assert_eq!(
        defined::<v1::ClientSessionCapabilities>(),
        ["_meta", "configOptions"],
        "the session-scoped client capabilities moved"
    );
    assert_eq!(
        defined::<v1::SessionConfigOptionsCapabilities>(),
        ["_meta", "boolean"],
        "a config option shape v1 lets a client claim and this one does not: decide, and say so"
    );
    assert_eq!(
        defined::<v1::BooleanConfigOptionCapabilities>(),
        ["_meta"],
        "the claim itself grew a field, so `{{}}` is no longer the whole of what it says"
    );
}

#[tokio::test]
async fn testy_advertises_every_capability_the_record_is_keyed_on() {
    // The fixture the display's rows are verified against (#77): an agent that
    // advertises all of them, in both shapes at once — the six the session
    // lifecycle is made of and the `logout` its auth block claims (§7.7).
    // Driven rather than read from Testy's source, because what the display
    // renders is what crossed the wire and was decoded as v1 — a capability the
    // schema names and the decode drops would read here as an agent that never
    // claimed it.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());

    let described = inspector.initialize().await.expect("Testy initializes");

    let missing: Vec<_> = advertisements(&described)
        .into_iter()
        .filter(|(_, advertised)| !advertised)
        .map(|(name, _)| name)
        .collect();
    assert!(
        missing.is_empty(),
        "Testy advertises every one of them; it did not advertise {missing:?}"
    );
    assert_eq!(
        inspector.agent().as_ref().map(advertisements),
        Some(advertisements(&described)),
        "and the store the display reads holds the same claim the call answered"
    );
}

#[tokio::test]
async fn an_agent_that_advertises_no_capability_advertises_none_of_them() {
    // The other half of the pair: every row a claim the agent did not make.
    // Which is a claim of its own — an agent that says nothing about
    // `session/close` has said it does not support it, and the display has a
    // row to say so with rather than an omission and a silence.
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#]).factory(),
    );

    let described = inspector.initialize().await.expect("it answered");

    let advertised: Vec<_> = advertisements(&described)
        .into_iter()
        .filter(|(_, advertised)| *advertised)
        .map(|(name, _)| name)
        .collect();
    assert!(
        advertised.is_empty(),
        "it claimed nothing, and claimed {advertised:?}"
    );
}

#[tokio::test]
async fn an_empty_session_capability_object_is_an_advertisement() {
    // What "presence" means on the wire, held against a real decode: `{}`
    // advertises and `null` does not. The display reads these as presence and
    // an agent that sent the empty object said yes with it.
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"close":{},"resume":null}}}}"#,
        ])
        .factory(),
    );

    let described = inspector.initialize().await.expect("it answered");

    assert_eq!(
        advertisements(&described)
            .into_iter()
            .filter(|(_, advertised)| *advertised)
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["session/close"],
        "the empty object is the claim; the null is the absence of one"
    );
}

#[tokio::test]
async fn an_agent_that_advertises_no_listing_is_not_offered_one() {
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#]).factory(),
    );

    inspector.initialize().await.expect("it answered");

    assert!(
        !inspector.lists_sessions(),
        "an agent that advertised nothing is offered nothing"
    );
}

#[tokio::test]
async fn listing_sessions_shows_what_the_agent_returned() {
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");

    let first = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    let second = inspector.new_session("/tmp", None).await.expect("another");

    let listing = inspector
        .list_sessions(None)
        .await
        .expect("Testy lists its sessions");

    let listed: Vec<_> = listing
        .sessions
        .iter()
        .map(|session| session.session_id.clone())
        .collect();
    assert!(
        listed.contains(&first) && listed.contains(&second),
        "both sessions are in the listing: {listed:?}"
    );
    assert_eq!(listing.next_page, None, "and Testy has no page to hand out");
    assert_eq!(
        inspector.listing(),
        Some(listing),
        "the store the display reads holds what the call answered"
    );
}

#[tokio::test]
async fn a_cursor_the_agent_hands_out_is_carried_back_verbatim() {
    // Paging is not interpreted (§7.1): no surveyed agent emits `nextCursor`,
    // so a cursor that turns up anyway is an opaque token this client hands
    // back exactly as it arrived — never parsed, never rebuilt.
    const CURSOR: &str = "after=%7B%22at%22:2%7D&page=2";

    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"list":{}}}}}"#,
            &format!(
                r#"{{"jsonrpc":"2.0","id":2,"result":{{"sessions":[{{"sessionId":"one","cwd":"/tmp"}}],"nextCursor":"{CURSOR}"}}}}"#
            ),
            r#"{"jsonrpc":"2.0","id":3,"result":{"sessions":[{"sessionId":"two","cwd":"/tmp"}]}}"#,
        ])
        .factory(),
    );
    inspector.initialize().await.expect("it answered");

    let first = inspector.list_sessions(None).await.expect("a first page");
    assert_eq!(first.sessions.len(), 1);
    let next_page = first.next_page.expect("the agent handed out a cursor");

    let listing = inspector
        .list_sessions(Some(next_page))
        .await
        .expect("a second page");

    assert_eq!(
        listing
            .sessions
            .iter()
            .map(|session| session.session_id.to_string())
            .collect::<Vec<_>>(),
        ["one", "two"],
        "the continuation adds to the listing it continues"
    );
    assert_eq!(listing.next_page, None, "and the agent ended the paging");
    assert_eq!(inspector.listing(), Some(listing));

    let asked = sent(&inspector);
    assert!(
        asked
            .last()
            .is_some_and(|frame| frame.contains(&format!(r#""cursor":"{CURSOR}""#))),
        "the cursor went back as it arrived: {asked:?}"
    );
}

#[tokio::test]
async fn auth_required_becomes_a_login_state_rather_than_a_failure() {
    // The state the spec asks the UI to surface (§7.1): `-32000 auth_required`
    // is an answer about the agent, not a crash — and the methods to name in it
    // are the ones `initialize` advertised.
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"authMethods":[{"id":"browser","name":"Log in with the browser","description":"the agent runs its own flow"}]}}"#,
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"authenticate first"}}"#,
        ])
        .factory(),
    );

    let described = inspector.initialize().await.expect("it answered");
    assert_eq!(
        described
            .auth_methods
            .iter()
            .map(v1::AuthMethod::name)
            .collect::<Vec<_>>(),
        ["Log in with the browser"],
        "what the login state has to name is on the agent's own account of itself"
    );
    assert_eq!(
        inspector.auth(),
        AuthState::Unasked,
        "and nothing has asked for a login yet"
    );

    let refused = inspector
        .new_session("/tmp", None)
        .await
        .expect_err("it wants a login first");

    let CallError::Rejected(error) = refused else {
        panic!("the agent's own refusal, carried whole: {refused:?}");
    };
    assert_eq!(error.code, v1::ErrorCode::AuthRequired);
    assert_eq!(
        inspector.auth(),
        AuthState::Required(error),
        "which the store says, so the screen can say it without having been the caller"
    );
}

#[tokio::test]
async fn authenticate_is_sent_with_the_chosen_method_and_its_outcome_is_visible() {
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    let described = inspector.initialize().await.expect("Testy initializes");

    let method = described
        .auth_methods
        .first()
        .expect("Testy advertises an auth method")
        .id()
        .clone();

    inspector
        .authenticate(&method)
        .await
        .expect("Testy authenticates");

    assert_eq!(inspector.auth(), AuthState::Authenticated(method.clone()));
    let asked = sent(&inspector);
    assert!(
        asked.iter().any(|frame| {
            frame.contains(r#""method":"authenticate""#)
                && frame.contains(&format!(r#""methodId":"{method}""#))
        }),
        "the method the user picked is the one on the wire: {asked:?}"
    );
}

#[tokio::test]
async fn an_authenticate_the_agent_refuses_says_what_the_agent_said() {
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");

    let method = v1::AuthMethodId::new("not-a-method-testy-has");
    let refused = inspector
        .authenticate(&method)
        .await
        .expect_err("Testy knows nothing of it");

    assert_eq!(
        inspector.auth(),
        AuthState::Refused {
            method,
            error: refused
        },
        "the outcome is visible whether or not anybody awaited the call"
    );
}

#[tokio::test]
async fn an_authenticate_refused_with_auth_required_is_still_a_login_state() {
    // The two ways an agent asks for a login: to some other call, and to the
    // login itself. Both leave the screen saying the same thing, because both
    // are the agent saying it will not go on.
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"authMethods":[{"id":"browser","name":"Log in with the browser"}]}}"#,
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"log in with the browser first"}}"#,
        ])
        .factory(),
    );
    inspector.initialize().await.expect("it answered");

    let method = v1::AuthMethodId::new("browser");
    inspector
        .authenticate(&method)
        .await
        .expect_err("it wants the login run elsewhere");

    let state = inspector.auth();
    assert!(
        matches!(&state, AuthState::Refused { method: refused, .. } if refused == &method),
        "the attempt is what the state is about: {state:?}"
    );
    assert!(
        state.needs_login(),
        "and the agent is still asking for a login: {state:?}"
    );
}

#[tokio::test]
async fn a_login_attempted_at_an_agent_that_is_gone_says_so_too() {
    // The panel outlives the agent it describes (§5), so its buttons outlive it
    // too — and a click that reaches nothing has to read as an attempt that
    // failed rather than as a button that did nothing. The rule `prompt`
    // already follows.
    let inspector = Inspector::new();
    let method = v1::AuthMethodId::new("browser");

    let disconnected = inspector
        .authenticate(&method)
        .await
        .expect_err("there is no agent to ask");

    assert_eq!(disconnected, CallError::Disconnected);
    assert_eq!(
        inspector.auth(),
        AuthState::Refused {
            method,
            error: CallError::Disconnected
        }
    );
}

// What became of driving what the agent advertised (§7.7). Testy answers what it
// claims, and the refusals are `/bin/sh` one-liners for the reason the tests
// above script theirs — an agent that advertises a method and will not do it is
// the case the record exists to report.

/// An agent that advertises `session/load` and nothing else.
const LOADS: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}"#;
/// The session it opens, which is the one the tests then ask it to load.
const OPENED: &str = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}"#;

/// An agent that claims these things about its sessions in the shape v1 states
/// such a claim in — the presence of an object under `sessionCapabilities` —
/// and implements none of them.
fn claiming(session_capabilities: &[&str]) -> String {
    let claims: Vec<_> = session_capabilities
        .iter()
        .map(|capability| format!(r#""{capability}":{{}}"#))
        .collect();

    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1,"agentCapabilities":{{"sessionCapabilities":{{{}}}}}}}}}"#,
        claims.join(",")
    )
}

/// A call answered with nothing in it, under the id it went out with — which is
/// the whole of what `session/load`, `session/close` and `session/delete` answer
/// when they succeed.
///
/// The scripted agents answer by counting reads rather than by matching ids, so
/// which id that is is the position of the call on the connection: the third,
/// after `initialize` and `session/new`.
fn answered(call: u8) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{call},"result":{{}}}}"#)
}

/// And a call refused, under the same id.
fn refused(call: u8) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{call},"error":{{"code":-32601,"message":"Method not found"}}}}"#
    )
}

/// An agent that advertises `session/load`, opens a session, then reads the load
/// and does `then` with it — including nothing at all, which is an agent that
/// dies holding the call.
///
/// [`answering`] covers every case where the agent *answers*; this is for the
/// two where what the agent does next is the point.
fn taking_the_load(then: &str) -> AgentCommand {
    shell(&format!(
        "read _; printf '%s\\n' '{LOADS}'; read _; printf '%s\\n' '{OPENED}'; read _; {then}"
    ))
}

/// What the scripted agents above said about the session they opened, as the
/// listing row a user clicks would have described it.
fn s1() -> v1::SessionInfo {
    v1::SessionInfo::new("s1", "/tmp")
}

#[tokio::test]
async fn a_load_the_agent_answered_is_recorded_as_driven_and_answered() {
    // The record's whole point, at its simplest (§7.7): a capability nobody
    // pressed the button on and one the agent honoured stop looking the same.
    // Testy advertises `loadSession` and implements it, so this is the
    // conformant half of the pair.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Unasked,
        "nothing the handshake sends drives an advertisement this record keys on"
    );
    let info = listed(&inspector, &session).await;

    inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("Testy loads the session");

    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Answered
    );
}

#[tokio::test]
async fn a_load_the_agent_refused_is_recorded_with_the_agents_own_error() {
    // §7.5's capability lie, where the panel now meets it: the affordance was
    // drawn because the agent claimed the method, the call went out, and the
    // refusal is the finding. The agent's own error is carried whole — a client
    // that paraphrased it would be standing between the reader and the wire.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[LOADS, OPENED, &refused(3)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let error = inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect_err("it advertised the method it has not implemented");

    assert!(
        matches!(&error, CallError::Rejected(refusal) if refusal.message == "Method not found"),
        "the agent's own refusal: {error:?}"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Refused(error),
        "and the record says exactly what the caller was told"
    );
}

#[tokio::test]
async fn a_load_the_connection_died_under_is_recorded_as_refused() {
    // **Not a fourth outcome** (§7.7). A connection that went away holding a
    // call is where `AuthState` and both Session Settings setters already put a
    // refusal, so one dead connection does not leave every in-flight row telling
    // a different story from the auth panel about the same event.
    let inspector = Inspector::new();
    inspector.connect(&taking_the_load("").factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let error = inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect_err("the agent died holding the call");

    assert_eq!(error, CallError::Disconnected);
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Refused(CallError::Disconnected)
    );
}

#[tokio::test]
async fn a_load_attempted_with_no_connection_at_all_is_recorded_by_the_caller() {
    // A call that never became a frame has no reader event, so the record is
    // written by whoever asked — the rule the turn and the two setters already
    // follow. The panel outlives the agent it describes (§5), so a click that
    // reached nothing has to read as an attempt that failed rather than as a row
    // nobody ever pressed.
    let inspector = Inspector::new();

    let error = inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect_err("there is no agent to ask");

    assert_eq!(error, CallError::Disconnected);
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Refused(CallError::Disconnected)
    );
}

#[tokio::test]
async fn a_refusal_is_replaced_by_the_next_drive() {
    // **Latest wins, and a refusal is not sticky** (§7.7): a later drive is the
    // next ask, so a capability the user has since driven successfully stops
    // reporting a refusal they have already fixed. The rule a
    // `session/set_config_option` answer already follows.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[LOADS, OPENED, &refused(3), &answered(4)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect_err("the first one is refused");
    assert!(matches!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Refused(_)
    ));

    inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect("and the second one is answered");

    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Answered
    );
}

#[tokio::test]
async fn the_record_is_empty_until_something_is_driven_and_empty_again_on_the_next_agent() {
    // Empty is an answer — this agent has been asked nothing yet — and it is the
    // answer a fresh connection gives, so the panel never attributes the last
    // agent's behaviour to this one.
    let inspector = Inspector::new();
    assert!(
        inspector.driven().is_empty(),
        "an inspector that has connected to nothing has driven nothing"
    );

    let session = inspector.start(&agent()).await.expect("Testy starts");
    assert!(
        inspector.driven().is_empty(),
        "and no call the handshake makes is one this record keys on"
    );
    let info = listed(&inspector, &session).await;
    inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("Testy loads the session");
    assert!(!inspector.driven().is_empty());

    inspector.connect(&agent().factory());

    assert!(
        inspector.driven().is_empty(),
        "what the last agent did is not this one's answer to anything"
    );
}

#[tokio::test]
async fn the_record_survives_the_agent_exiting() {
    // The panel outlives the agent it describes (§5), and blanking the record at
    // the moment the run ended would erase the evidence just as somebody turned
    // to read it.
    let inspector = Inspector::new();
    inspector
        .connect(&taking_the_load(&format!("printf '%s\\n' '{}'; read _", answered(3))).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    inspector
        .restore_session(&s1(), Restore::Load, None)
        .await
        .expect("it loaded the session");

    // Release the scripted Agent only after the load answer has crossed the
    // typed-client seam. It exits without answering this extra request; the
    // record under test is the completed load that came before it.
    let _ = inspector.list_sessions(None).await;

    let mut status = inspector.status_changes();
    until(&mut status, "the agent to exit on its own", || {
        inspector.status() == ConnectionStatus::Lost
    })
    .await;

    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Answered,
        "what it did while it was there is still readable"
    );
}

#[tokio::test]
async fn opening_a_different_session_leaves_the_record_alone() {
    // A capability is claimed once per connection and is true of every session
    // on it, so a session switch is not an event this record has anything to say
    // about (§7.7) — unlike the settings, which are the live session's and are
    // replaced whole.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let info = listed(&inspector, &session).await;
    inspector
        .restore_session(&info, Restore::Load, None)
        .await
        .expect("Testy loads the session");

    inspector
        .new_session("/tmp", None)
        .await
        .expect("another session on the same agent");

    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Answered
    );
}

#[tokio::test]
async fn an_answer_that_arrives_after_its_connection_was_released_writes_nothing() {
    // The registration that maps an answer back to its capability is emptied by
    // the teardown, which is what stops a dying connection's late answer landing
    // in the record — the mechanism five id-keyed slots already rely on, rather
    // than a connection ordinal on one store and not the others (§7.7).
    //
    // So the teardown is what refuses the call, and the answer the released
    // connection was about to give writes nothing over it.
    let inspector = Inspector::new();
    inspector.connect(
        &taking_the_load(&format!(
            "sleep 1; printf '%s\\n' '{}'; cat > /dev/null",
            answered(3)
        ))
        .factory(),
    );
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let loading = tokio::spawn({
        let inspector = inspector.clone();
        async move { inspector.restore_session(&s1(), Restore::Load, None).await }
    });
    let mut frames = inspector.trace().changes();
    until(&mut frames, "the load to reach the agent", || {
        position(&inspector, r#""method":"session/load""#).is_some()
    })
    .await;
    inspector.disconnect();

    assert_eq!(
        loading.await.expect("the load task"),
        Err(CallError::Disconnected)
    );
    let refused = Driven::Refused(CallError::Disconnected);
    assert_eq!(inspector.driven().of(AgentCapability::Load), refused);
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        refused,
        "and nothing the released connection was carrying landed afterwards"
    );
}

#[tokio::test]
async fn authenticate_is_recorded_where_it_always_was_and_not_in_this_store() {
    // **One store, and `authenticate` is not in the new one** (§7.7).
    // `AuthState` already holds not-driven, accepted-with-a-method and
    // refused-with-the-agent's-error, per connection — so the auth row renders
    // from it, and two stores holding the same three things would be two things
    // to disagree.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    let described = inspector.initialize().await.expect("Testy initializes");
    let method = described
        .auth_methods
        .first()
        .expect("Testy advertises an auth method")
        .id()
        .clone();

    inspector
        .authenticate(&method)
        .await
        .expect("Testy authenticates");

    assert_eq!(inspector.auth(), AuthState::Authenticated(method));
    assert!(
        inspector.driven().is_empty(),
        "and the record of what was driven says nothing a second time"
    );
    assert_eq!(
        inspector.auth().driven(),
        Driven::Answered,
        "the auth row's fourth fact comes out of the state instead"
    );
}

/// An agent that advertises a way to log in and implements nothing else.
///
/// The `authenticate` row's own fixture: what the display gates that row on is
/// `authMethods`, which is an advertisement on the `initialize` result beside
/// the capabilities rather than one of them (§7.1).
const OFFERS_A_LOGIN: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"authMethods":[{"id":"browser","name":"Log in with the browser"}]}}"#;

/// The agent asking for a login, in the words it asks in.
fn wants_a_login() -> v1::Error {
    v1::Error::new(i32::from(v1::ErrorCode::AuthRequired), "authenticate first")
}

#[test]
fn the_auth_rows_fourth_fact_is_the_auth_state_and_says_the_same_thing_it_does() {
    // **The two must never say different things about the same login** (§7.7).
    // The auth block reads `AuthState` and so does the capability row, so what
    // is asserted here is the one mapping between them — every state this
    // connection can be in, and what the row says while it is in it.
    //
    // `Required` is the one worth stating: an agent that answered
    // `-32000 auth_required` to something else has asked for a login and nobody
    // has sent `authenticate`, so the row reads *not driven* while the block
    // above it says what the agent said.
    let method = browser();

    for (auth, driven) in [
        (AuthState::Unasked, Driven::Unasked),
        (AuthState::Required(wants_a_login()), Driven::Unasked),
        (AuthState::Authenticated(method.clone()), Driven::Answered),
        (
            AuthState::Refused {
                method: method.clone(),
                error: CallError::Rejected(wants_a_login()),
            },
            Driven::Refused(CallError::Rejected(wants_a_login())),
        ),
        (
            AuthState::Refused {
                method,
                error: CallError::Disconnected,
            },
            Driven::Refused(CallError::Disconnected),
        ),
    ] {
        assert_eq!(auth.driven(), driven, "{auth:?}");
    }
}

#[tokio::test]
async fn an_authenticate_the_connection_died_under_leaves_one_answer_for_both() {
    // **A connection that went away holding a call is a refusal** (§7.7), which
    // is where this state has always put it — so the auth block and the
    // capability row say the same thing about the same event rather than one of
    // them reporting a login still in flight.
    let inspector = Inspector::new();
    // It answers `initialize` and then dies holding the `authenticate`, which is
    // the shape `taking_the_load` gives the load path.
    inspector.connect(
        &shell(&format!(
            "read _; printf '%s\\n' '{OFFERS_A_LOGIN}'; read _;"
        ))
        .factory(),
    );
    inspector.initialize().await.expect("it answered");
    let method = browser();

    let error = inspector
        .authenticate(&method)
        .await
        .expect_err("the agent died holding the call");

    assert_eq!(error, CallError::Disconnected);
    assert_eq!(
        inspector.auth(),
        AuthState::Refused {
            method,
            error: CallError::Disconnected
        }
    );
    assert_eq!(
        inspector.auth().driven(),
        Driven::Refused(CallError::Disconnected),
        "and the row says exactly what the block above it says"
    );
    assert!(
        inspector.driven().is_empty(),
        "out of the one store that holds it, and not a second one"
    );
}

#[tokio::test]
async fn a_new_connection_claims_nothing_the_last_one_claimed() {
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");
    let session = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    inspector.list_sessions(None).await.expect("a listing");
    let described = inspector.agent().expect("it claimed something");
    let method = described.auth_methods.first().expect("an auth method").id();
    inspector
        .authenticate(method)
        .await
        .expect("Testy authenticates");
    inspector
        .restore_session(&listed(&inspector, &session).await, Restore::Load, None)
        .await
        .expect("Testy loads the session");

    inspector.connect(&agent().factory());

    assert!(
        inspector.agent().is_none(),
        "a new agent has claimed nothing yet"
    );
    assert!(!inspector.lists_sessions());
    assert_eq!(inspector.listing(), None);
    assert_eq!(inspector.auth(), AuthState::Unasked);
    // And nothing it did has been done to this one either (§7.7).
    assert!(inspector.driven().is_empty());
}

// And the rest of the session lifecycle, which is the same record wired to the
// four calls that were going out registering nothing: `session/resume` off the
// restore property, and `session/close`, `session/delete` and `session/list`
// through the prepare/send split the load path already used (§7.7).

#[tokio::test]
async fn a_resume_the_agent_answered_is_recorded_apart_from_the_load_it_is_not() {
    // **Two advertisements, not one** (§7.5): reopening a session is one
    // operation with a property, and an agent claims the two calls it can be
    // sent as separately — so the outcome of the one that was sent must not be
    // written on the row of the one that was not.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let info = listed(&inspector, &session).await;

    inspector
        .restore_session(&info, Restore::Resume, None)
        .await
        .expect("Testy resumes the session");

    assert_eq!(
        inspector.driven().of(AgentCapability::Resume),
        Driven::Answered
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Unasked,
        "the call that was not sent drove nothing"
    );
}

#[tokio::test]
async fn a_resume_the_agent_refused_is_recorded_with_the_agents_own_error() {
    // The advertisement that is a lie, for the other way in: the affordance was
    // drawn because the agent claimed `resume`, the call went out, and the
    // refusal is the finding.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[&claiming(&["resume"]), OPENED, &refused(3)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let error = inspector
        .restore_session(&s1(), Restore::Resume, None)
        .await
        .expect_err("it advertised the method it has not implemented");

    assert_eq!(
        inspector.driven().of(AgentCapability::Resume),
        Driven::Refused(error)
    );
}

#[tokio::test]
async fn the_two_calls_that_end_a_session_record_what_the_agent_answered() {
    // The pair the record was blank about (§7.7): both went out through a path
    // that registered nothing, so a close the agent honoured read exactly like a
    // close nobody asked for. Testy claims both and answers both.
    let inspector = Inspector::new();
    let first = inspector.start(&agent()).await.expect("Testy starts");
    let second = inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");

    inspector
        .close_session(&first)
        .await
        .expect("Testy closes it");
    assert_eq!(
        inspector.driven().of(AgentCapability::Close),
        Driven::Answered
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Delete),
        Driven::Unasked,
        "and the row beside it still says nobody has asked"
    );

    inspector
        .delete_session(&second)
        .await
        .expect("Testy deletes it");

    assert_eq!(
        inspector.driven().of(AgentCapability::Delete),
        Driven::Answered
    );
}

#[tokio::test]
async fn a_close_the_agent_refused_is_recorded_with_the_agents_own_error() {
    // §7.5's capability lie again, on the row #96 opened with: an agent that
    // advertises `session/close` and refuses every one of them stops looking
    // like an agent nobody pressed the button on.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[&claiming(&["close"]), OPENED, &refused(3)]).factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let error = inspector
        .close_session(&live)
        .await
        .expect_err("it advertised the method it has not implemented");

    assert!(
        matches!(&error, CallError::Rejected(refusal) if refusal.message == "Method not found"),
        "the agent's own refusal: {error:?}"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Close),
        Driven::Refused(error),
        "and the record says exactly what the caller was told"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Delete),
        Driven::Unasked,
        "the delete row beside it is still a row nobody drove"
    );
}

#[tokio::test]
async fn a_delete_the_agent_refused_is_recorded_with_the_agents_own_error() {
    // The same rule for the other call, which is a different thing to an agent
    // and the same thing to this record.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[&claiming(&["delete"]), OPENED, &refused(3)]).factory());
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let error = inspector
        .delete_session(&live)
        .await
        .expect_err("it advertised the method it has not implemented");

    assert_eq!(
        inspector.driven().of(AgentCapability::Delete),
        Driven::Refused(error)
    );
}

#[tokio::test]
async fn a_listing_somebody_asked_for_records_what_the_agent_answered() {
    // The listing is the one call this client also makes on its own initiative,
    // so the drive that counts is the one a user asked for — and that one
    // records like every other (§7.7).
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    inspector
        .list_sessions(None)
        .await
        .expect("Testy lists its sessions");

    assert_eq!(
        inspector.driven().of(AgentCapability::List),
        Driven::Answered
    );
}

#[tokio::test]
async fn a_listing_the_agent_refused_is_recorded_with_the_agents_own_error() {
    let inspector = Inspector::new();
    inspector.connect(&answering(&[&claiming(&["list"]), OPENED, &refused(3)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let error = inspector
        .list_sessions(None)
        .await
        .expect_err("it advertised the method it has not implemented");

    assert_eq!(
        inspector.driven().of(AgentCapability::List),
        Driven::Refused(error)
    );
}

// `logout`, the first of the two advertisements the panel drew and nothing
// could reach (§7.7). Testy answers it and can say nothing else about it — it
// clears its authenticated methods, always succeeds, and nothing it holds ever
// reads that set — so every refusal and every consequence below is a scripted
// one-liner, the way the boolean config option fixtures were.

/// An agent that advertises `logout` and one auth method, and implements
/// neither: what it does with either call is whatever the script says next.
const LOGS_OUT: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"#,
    r#""authMethods":[{"id":"browser","name":"Log in with the browser"}],"#,
    r#""agentCapabilities":{"auth":{"logout":{}}}}}"#,
);

/// The method the user picked, which is the one the scripted agents above
/// advertise.
fn browser() -> v1::AuthMethodId {
    v1::AuthMethodId::new("browser")
}

#[tokio::test]
async fn the_logout_affordance_is_gated_on_the_advertisement_and_on_nothing_else() {
    // **Never on what this tool believes the agent's state makes sensible**
    // (§7.7): a logout with nobody logged in is a corner the specification
    // leaves entirely undefined, which is the reason to drive it rather than a
    // reason to withhold the button. So the gate answers the same before a login
    // and after one.
    let inspector = Inspector::new();
    assert!(
        !inspector.logs_out(),
        "nothing is advertised before an agent has said anything"
    );

    inspector.connect(&agent().factory());
    assert!(
        !inspector.logs_out(),
        "and nothing is advertised until `initialize` has answered"
    );
    let described = inspector.initialize().await.expect("Testy initializes");

    assert!(
        inspector.logs_out(),
        "Testy advertises `agentCapabilities.auth.logout`"
    );
    assert_eq!(
        inspector.auth(),
        AuthState::Unasked,
        "and it is offered with nobody logged in"
    );

    let method = described
        .auth_methods
        .first()
        .expect("Testy advertises an auth method")
        .id()
        .clone();
    inspector
        .authenticate(&method)
        .await
        .expect("Testy authenticates");

    assert!(
        inspector.logs_out(),
        "and it is the same offer once a login has been accepted"
    );
}

#[tokio::test]
async fn an_agent_that_advertises_no_logout_is_not_offered_one() {
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#]).factory(),
    );

    inspector.initialize().await.expect("it answered");

    assert!(
        !inspector.logs_out(),
        "an agent that advertised nothing is offered nothing"
    );
}

#[tokio::test]
async fn a_logout_the_agent_answered_is_recorded_as_driven_and_answered() {
    // The one thing Testy can settle about a logout: whether it is answered at
    // all. Driven with nobody logged in, because that is the state the button is
    // offered in and the corner the specification says nothing about.
    let inspector = Inspector::new();
    inspector.connect(&agent().factory());
    inspector.initialize().await.expect("Testy initializes");
    assert_eq!(
        inspector.driven().of(AgentCapability::Logout),
        Driven::Unasked,
        "nothing the handshake sends drives it"
    );

    inspector.logout().await.expect("Testy answers a logout");

    assert_eq!(
        inspector.driven().of(AgentCapability::Logout),
        Driven::Answered
    );
    let asked = sent(&inspector);
    let logout = asked
        .iter()
        .find(|frame| frame.contains(r#""method":"logout""#))
        .unwrap_or_else(|| panic!("the logout went out: {asked:?}"));
    assert!(
        !logout.contains("sessionId"),
        "connection-scoped, and the request carries no session id: {logout}"
    );
}

#[tokio::test]
async fn a_successful_logout_returns_the_auth_state_to_unasked() {
    // **It answers `{}`, so it licenses no claim about where authentication
    // stands** (§7.7). What a success does say is that the agent has taken back
    // what it accepted — so going on saying *the agent accepted `authenticate`
    // with browser* would be the panel stating something the agent retracted.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[LOGS_OUT, &answered(2), &answered(3)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .authenticate(&browser())
        .await
        .expect("it accepted the method");
    assert_eq!(inspector.auth(), AuthState::Authenticated(browser()));

    inspector
        .logout()
        .await
        .expect("and it answered the logout");

    assert_eq!(inspector.auth(), AuthState::Unasked);
}

#[tokio::test]
async fn a_successful_logout_leaves_the_login_screen_down() {
    // **The schema predicts a login; the agent has not asked for one** (§7.7).
    // The inspector reports what the agent said rather than what the schema
    // expects it to say next, and whatever gets refused later settles it
    // honestly — which is the same rule that raised the screen in the first
    // place, wherever an `auth_required` crosses.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[LOGS_OUT, &answered(2), &answered(3)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .authenticate(&browser())
        .await
        .expect("it accepted the method");

    inspector
        .logout()
        .await
        .expect("and it answered the logout");

    assert!(
        !inspector.auth().needs_login(),
        "the agent has asked for nothing: {:?}",
        inspector.auth()
    );
}

#[tokio::test]
async fn a_logout_the_agent_refused_says_what_the_agent_said_and_claims_nothing_about_the_login() {
    // The refusal Testy cannot be made to give (§12), scripted: the affordance
    // was drawn because the agent claimed the method, the call went out, and the
    // refusal is the finding. The agent retracted nothing, so the auth state
    // stands exactly as it was — a client that read a refusal as a logout would
    // be reporting something nobody said.
    let inspector = Inspector::new();
    inspector.connect(&answering(&[LOGS_OUT, &answered(2), &refused(3)]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .authenticate(&browser())
        .await
        .expect("it accepted the method");

    let error = inspector
        .logout()
        .await
        .expect_err("it advertised the method it has not implemented");

    assert!(
        matches!(&error, CallError::Rejected(refusal) if refusal.message == "Method not found"),
        "the agent's own refusal: {error:?}"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Logout),
        Driven::Refused(error),
        "and the record says exactly what the caller was told"
    );
    assert_eq!(
        inspector.auth(),
        AuthState::Authenticated(browser()),
        "while where authentication stands is what it was before the attempt"
    );
}

#[tokio::test]
async fn a_logout_the_connection_died_under_is_recorded_as_refused() {
    // **Not a fourth outcome** (§7.7), for the capability that has no session to
    // fall back on: a connection that went away holding the call is where
    // `AuthState` and both setters already put a refusal.
    let inspector = Inspector::new();
    inspector.connect(&shell(&format!("read _; printf '%s\\n' '{LOGS_OUT}'; read _;")).factory());
    inspector.initialize().await.expect("it answered");

    let error = inspector
        .logout()
        .await
        .expect_err("the agent died holding the call");

    assert_eq!(error, CallError::Disconnected);
    assert_eq!(
        inspector.driven().of(AgentCapability::Logout),
        Driven::Refused(CallError::Disconnected)
    );
    assert_eq!(
        inspector.auth(),
        AuthState::Unasked,
        "and a call nobody answered retracted nothing"
    );
}

#[tokio::test]
async fn a_logout_leaves_the_live_session_and_its_turn_strictly_alone() {
    // **Nothing is cancelled and nothing is closed ahead of it** (§7.7). What
    // the agent does to a session it has logged out of is the finding — the
    // specification guarantees nothing there — and pre-empting it would answer
    // the question on the agent's behalf.
    //
    // **Scripted, because Testy cannot be the subject of this** (§12): a logout
    // against it has no observable consequence at all. An agent that says a
    // line, answers a turn and then answers the logout is what makes "the
    // session was left alone" something there is evidence for.
    let inspector = Inspector::new();
    inspector.connect(
        &advertising(
            r#"{"auth":{"logout":{}}}"#,
            &[
                r#"'{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'"#,
                r#"'{"jsonrpc":"2.0","id":4,"result":{}}'"#,
            ],
        )
        .factory(),
    );
    inspector.initialize().await.expect("it answered");
    let session = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");
    spoken_in(&inspector).await;
    let ended = inspector
        .prompt("hello")
        .await
        .expect("it answered the turn");
    let turn = inspector.turn();
    assert_eq!(turn, TurnState::Ended(ended));

    inspector
        .logout()
        .await
        .expect("and it answered the logout");

    let asked = sent(&inspector);
    for method in [
        v1::AGENT_METHOD_NAMES.session_cancel,
        v1::AGENT_METHOD_NAMES.session_close,
        v1::AGENT_METHOD_NAMES.session_delete,
    ] {
        assert!(
            !asked
                .iter()
                .any(|frame| frame.contains(&format!(r#""method":"{method}""#))),
            "no {method} crossed, before, with or after it: {asked:?}"
        );
    }
    assert_eq!(
        inspector.session(),
        Some(session),
        "the session that was live is the one that is live"
    );
    assert_eq!(inspector.turn(), turn, "and its turn is where it stood");
    assert!(
        !inspector.timeline().is_empty(),
        "and the view of it was not discarded and rebuilt"
    );
}

#[tokio::test]
async fn driving_anything_at_a_connection_that_is_gone_is_recorded_without_a_frame() {
    // The panel outlives the agent it describes (§5), so its buttons outlive it
    // too: a click that reaches nothing has to read as an attempt that failed
    // rather than as a row nobody ever pressed. Written by the caller, because a
    // call that never became a frame has no reader event to write it — the rule
    // every affordance that drives one of these follows, so all four are here.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    inspector.disconnect();
    let asked = sent(&inspector).len();

    assert_eq!(
        inspector.close_session(&session).await,
        Err(CallError::Disconnected)
    );
    assert_eq!(
        inspector.delete_session(&session).await,
        Err(CallError::Disconnected)
    );
    assert_eq!(
        inspector.list_sessions(None).await,
        Err(CallError::Disconnected)
    );
    assert_eq!(inspector.logout().await, Err(CallError::Disconnected));

    for capability in [
        AgentCapability::Close,
        AgentCapability::Delete,
        AgentCapability::List,
        AgentCapability::Logout,
    ] {
        assert_eq!(
            inspector.driven().of(capability),
            Driven::Refused(CallError::Disconnected),
            "{} was driven at nobody, and says so",
            capability.name()
        );
    }
    assert_eq!(
        sent(&inspector).len(),
        asked,
        "and none of them put anything on a wire that has gone: {:?}",
        sent(&inspector)
    );
}

#[tokio::test]
async fn the_listing_the_tool_refreshes_on_its_own_initiative_records_nothing() {
    // **Only calls somebody asked for reach the record** (§7.7). After a close
    // or a delete the listing is asked for again without anybody clicking, and a
    // row marked by that call would be one the user never made and could not
    // clear by listing again themselves.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let second = inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");

    inspector
        .close_session(&session)
        .await
        .expect("Testy closes it");

    assert!(
        position(&inspector, r#""method":"session/list""#).is_some(),
        "the refresh went out: {:?}",
        sent(&inspector)
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::List),
        Driven::Unasked,
        "and nobody asked for it"
    );

    // And the other call it follows, which refreshes for the same reason.
    inspector
        .delete_session(&second)
        .await
        .expect("Testy deletes it");

    assert_eq!(
        inspector.driven().of(AgentCapability::List),
        Driven::Unasked,
        "nor for the one after the delete"
    );

    // And a user who asks for one themselves is driving it, on the very
    // connection the refresh already ran on.
    inspector
        .list_sessions(None)
        .await
        .expect("Testy lists its sessions");

    assert_eq!(
        inspector.driven().of(AgentCapability::List),
        Driven::Answered
    );
}

#[tokio::test]
async fn a_refresh_that_fails_records_nothing_either() {
    // The promise §7.5 made for it, kept where it bites: a listing the tool
    // asked for on its own initiative "says so in the trace and nowhere else",
    // whatever the agent answers it with. Otherwise a close the agent honoured
    // would mark the `session/list` row with a refusal of a call nobody made.
    let inspector = Inspector::new();
    inspector.connect(
        &answering(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"list":{},"close":{}}}}}"#,
            OPENED,
            &answered(3),
            &refused(4),
        ])
        .factory(),
    );
    inspector.initialize().await.expect("it answered");
    let live = inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    inspector
        .close_session(&live)
        .await
        .expect("it closed the session");

    assert_eq!(
        inspector.driven().of(AgentCapability::Close),
        Driven::Answered,
        "the call the user made is the one that was recorded"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::List),
        Driven::Unasked,
        "and the refresh it refused wrote nothing: {:?}",
        sent(&inspector)
    );
}

/// The id every *request* this client sent went out under, in the order they
/// crossed.
///
/// Requests and nothing else: a notification carries no id, and an answer this
/// client gave an agent carries the agent's, which is not one this client
/// allocated. Told apart by the method the request names, which is the field
/// only the two things this client *asks* carry.
fn ids(inspector: &Inspector) -> Vec<i64> {
    sent(inspector)
        .iter()
        .filter_map(|frame| {
            let frame: serde_json::Value =
                serde_json::from_str(frame).expect("what this client sent is JSON-RPC");
            frame.get("method")?;
            frame.get("id").and_then(serde_json::Value::as_i64)
        })
        .collect()
}

#[tokio::test]
async fn the_ids_this_client_allocates_are_still_the_order_it_sends_them_in() {
    // **What the scripted agents depend on** (§12): they answer by counting
    // reads rather than by matching ids, and several tests assert exact
    // handshake and frame counts. The prepare/send split the record needs of the
    // three calls that had none allocates ids exactly as the one-shot call did —
    // asserted here rather than assumed, because every fixture in this suite
    // rests on it. All three of them, and the listings the closes and the delete
    // refresh in between.
    let inspector = Inspector::new();
    let session = inspector.start(&agent()).await.expect("Testy starts");
    let second = inspector
        .open_session(&agent(), None)
        .await
        .expect("a second session");
    inspector.list_sessions(None).await.expect("Testy lists");
    inspector
        .close_session(&session)
        .await
        .expect("Testy closes it");
    inspector
        .delete_session(&second)
        .await
        .expect("Testy deletes the other");

    let allocated = ids(&inspector);
    assert_eq!(
        allocated,
        (1..=allocated.len() as i64).collect::<Vec<_>>(),
        "counted up from one, in the order they went out: {:?}",
        sent(&inspector)
    );
}
