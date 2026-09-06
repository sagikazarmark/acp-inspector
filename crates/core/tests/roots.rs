//! The roots a session is opened with (`docs/architecture.md` §7.7):
//! `additionalDirectories`, on `session/new` and on both ways of reopening a
//! session.
//!
//! **The advertisement the panel drew for four rings with nothing that could
//! reach it.** It gates a field on a request rather than a method, so what is
//! asserted here is what crossed: the list the user supplied, absolute, on the
//! three calls that carry it — and nothing at all where the list is empty, which
//! is what makes *whether it was driven* a fact about the frame rather than about
//! anybody's intent.
//!
//! Driven through the public inspector API only, the way the shell drives it
//! (§12). Testy is the agent where a real one can produce the case — it
//! advertises the capability and answers the calls that carry it — and `/bin/sh`
//! one-liners cover what it cannot: an agent that reports roots for a session it
//! listed, and one that never advertised the capability at all.

mod common;

use std::path::PathBuf;

use acp_inspector_core::{
    AgentCapability, AgentCommand, CallError, Driven, Inspector, Restore, Roots, v1,
};

use common::{advertising, sent, testy};

fn agent() -> AgentCommand {
    AgentCommand {
        command: testy().to_string_lossy().into_owned(),
        cwd: "/tmp".into(),
        ..AgentCommand::default()
    }
}

/// The answer a load or a resume gets from the scripted agents here.
const OPENED: &str = r#"'{"jsonrpc":"2.0","id":3,"result":{}}'"#;
/// And what an agent that will not take the roots says instead.
const TURNED_DOWN: &str = r#"'{"jsonrpc":"2.0","id":3,"error":{"code":-32602,"message":"that root is not mine to open"}}'"#;

/// An agent that advertises everything these tests gate a control on.
const OPENS: &str =
    r#"{"loadSession":true,"sessionCapabilities":{"resume":{},"additionalDirectories":{}}}"#;

/// What a scripted agent said about the session it opened, as the listing row a
/// user clicks would have described it — with whatever roots it reported.
fn s1(reported: &[&str]) -> v1::SessionInfo {
    v1::SessionInfo::new("s1", "/tmp")
        .additional_directories(reported.iter().map(PathBuf::from).collect())
}

/// The one frame that carries `method`, as it crossed.
fn frame(inspector: &Inspector, method: &str) -> String {
    sent(inspector)
        .into_iter()
        .find(|frame| frame.contains(&format!(r#""method":"{method}""#)))
        .unwrap_or_else(|| panic!("{method} is on the wire: {:?}", sent(inspector)))
}

#[tokio::test]
async fn roots_the_user_supplied_reach_the_wire_on_a_new_session() {
    // The control's whole job: a list somebody typed beside the `session/new`
    // button is what the agent is asked to open the session with. Testy
    // advertises the capability and answers the call, so this is the conformant
    // half of the pair.
    let inspector = Inspector::new();
    inspector.start(&agent()).await.expect("Testy starts");

    inspector
        .open_session(&agent(), Some(Roots::supplied("/srv/data\n/srv/models")))
        .await
        .expect("Testy opens a session with the roots it was given");

    // The last one, because the handshake opened a session of its own first and
    // this affordance is the one that was given roots.
    let asked = sent(&inspector)
        .into_iter()
        .rfind(|frame| frame.contains(r#""method":"session/new""#))
        .expect("the second session was asked for");
    assert!(
        asked.contains(r#""additionalDirectories":["/srv/data","/srv/models"]"#),
        "the roots the user supplied, in the order they supplied them: {asked}"
    );
    assert_eq!(
        inspector
            .driven()
            .of(AgentCapability::AdditionalDirectories),
        Driven::Answered,
        "and the advertisement the field is gated on was driven and answered"
    );
}

#[tokio::test]
async fn roots_the_user_supplied_reach_the_wire_on_both_ways_of_reopening() {
    // The other two calls that carry the field. Both, because they are one
    // operation with a property (§7.5) and a client that only filled the field
    // in on one of them would be two operations after all.
    for (how, method) in [
        (Restore::Load, "session/load"),
        (Restore::Resume, "session/resume"),
    ] {
        let inspector = Inspector::new();
        inspector.connect(&advertising(OPENS, &[OPENED]).factory());
        inspector.initialize().await.expect("it answered");
        inspector
            .new_session("/tmp", None)
            .await
            .expect("a session");

        inspector
            .restore_session(&s1(&[]), how, Some(Roots::supplied("/srv/data")))
            .await
            .expect("it reopened the session");

        let asked = frame(&inspector, method);
        assert!(
            asked.contains(r#""additionalDirectories":["/srv/data"]"#),
            "the roots the user supplied cross on {method}: {asked}"
        );
        assert_eq!(
            inspector
                .driven()
                .of(AgentCapability::AdditionalDirectories),
            Driven::Answered
        );
    }
}

#[tokio::test]
async fn a_reopen_asks_for_the_list_the_user_changed_it_to_and_reports_what_the_agent_said() {
    // **Changing them on a reopen is the point, not a hazard** (§7.5): the
    // schema permits a list that differs from any previously reported one as
    // long as the `cwd` matches, so the reported roots are a default rather than
    // a constraint. What the agent then does with a list it did not report is
    // exactly the finding — and it is reported whatever it is, in the agent's
    // own words.
    let inspector = Inspector::new();
    inspector.connect(&advertising(OPENS, &[TURNED_DOWN]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    let refused = inspector
        .restore_session(
            &s1(&["/srv/reported"]),
            Restore::Load,
            Some(Roots::supplied("/srv/somewhere-else")),
        )
        .await
        .expect_err("the agent would not open it with those");

    let asked = frame(&inspector, "session/load");
    assert!(
        asked.contains(r#""additionalDirectories":["/srv/somewhere-else"]"#)
            && !asked.contains("/srv/reported"),
        "the list the user changed it to, and not the one the listing reported: {asked}"
    );
    assert!(
        matches!(&refused, CallError::Rejected(error) if error.message == "that root is not mine to open"),
        "the agent's own answer, whole: {refused:?}"
    );
    assert_eq!(
        inspector
            .driven()
            .of(AgentCapability::AdditionalDirectories),
        Driven::Refused(refused.clone()),
        "and the row says what became of driving it"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Refused(refused),
        "on the call's own row too: one frame refused both (§7.7)"
    );
}

#[tokio::test]
async fn a_relative_root_goes_out_resolved_against_the_sessions_own_directory() {
    // **Nothing relative reaches the wire** (§7.7). The schema says each path
    // must be absolute and names `cwd` as the base for relative ones, so the
    // base is the session's — not the inspector's working directory, which is
    // what the spawn form's own `cwd` resolves against for a reason that does
    // not transfer. A request the schema calls malformed would contaminate every
    // annotation drawn against this agent with the client's own violation.
    let inspector = Inspector::new();
    inspector.connect(&advertising(OPENS, &[OPENED]).factory());
    inspector.initialize().await.expect("it answered");
    // The created session's own `cwd` is the one it is opened in, so that is
    // what its roots are relative to as well.
    inspector
        .new_session("/opt/project", Some(Roots::supplied("vendor")))
        .await
        .expect("a session");

    inspector
        .restore_session(
            &v1::SessionInfo::new("s1", "/home/work"),
            Restore::Load,
            Some(Roots::supplied("data")),
        )
        .await
        .expect("it reopened the session");

    for (method, resolved) in [
        (
            "session/new",
            r#""additionalDirectories":["/opt/project/vendor"]"#,
        ),
        (
            "session/load",
            r#""additionalDirectories":["/home/work/data"]"#,
        ),
    ] {
        let asked = frame(&inspector, method);
        assert!(
            asked.contains(resolved),
            "resolved against the session's cwd and not against this process's: {asked}"
        );
    }
    assert!(
        !sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""vendor""#) || frame.contains(r#""data""#)),
        "and nothing relative crossed at all: {:?}",
        sent(&inspector)
    );
}

#[tokio::test]
async fn an_empty_list_puts_no_field_on_the_wire_and_drives_nothing() {
    // The field is omitted when it is empty, so an empty control asks for
    // nothing — and *driven* is read off the frame rather than off the click
    // (§7.7), which is what keeps a row that says nothing happened honest about
    // an agent nobody asked anything of.
    let inspector = Inspector::new();
    inspector.connect(&advertising(OPENS, &[OPENED]).factory());
    inspector.initialize().await.expect("it answered");

    inspector
        .new_session("/tmp", Some(Roots::supplied("   \n\n")))
        .await
        .expect("a session");
    inspector
        .restore_session(&s1(&[]), Restore::Load, Some(Roots::default()))
        .await
        .expect("and a reopen with nothing in the control");

    for method in ["session/new", "session/load"] {
        let asked = frame(&inspector, method);
        assert!(
            !asked.contains("additionalDirectories"),
            "{method} carries no such field: {asked}"
        );
    }
    assert_eq!(
        inspector
            .driven()
            .of(AgentCapability::AdditionalDirectories),
        Driven::Unasked,
        "so nothing drove the advertisement"
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Load),
        Driven::Answered,
        "and the call that carried no roots still drove its own"
    );
}

#[tokio::test]
async fn a_default_reopen_drives_the_capability_exactly_when_the_listing_reported_roots() {
    // **The default reopen hands the agent back the roots it reported**, so it
    // drives the capability when that list was not empty and drives nothing when
    // it was — which is the same rule read off the same frame, with nobody
    // having typed anything.
    for (reported, expected) in [
        (vec![], Driven::Unasked),
        (vec!["/srv/reported"], Driven::Answered),
    ] {
        let inspector = Inspector::new();
        inspector.connect(&advertising(OPENS, &[OPENED]).factory());
        inspector.initialize().await.expect("it answered");
        inspector
            .new_session("/tmp", None)
            .await
            .expect("a session");

        inspector
            .restore_session(&s1(&reported), Restore::Load, None)
            .await
            .expect("it reopened the session");

        let asked = frame(&inspector, "session/load");
        assert_eq!(
            asked.contains(r#""additionalDirectories":["/srv/reported"]"#),
            !reported.is_empty(),
            "the reported roots are what a default reopen asks for: {asked}"
        );
        assert_eq!(
            inspector
                .driven()
                .of(AgentCapability::AdditionalDirectories),
            expected,
            "reopening a session the agent described with {reported:?}"
        );
    }
}

#[tokio::test]
async fn a_reopen_still_sends_the_reported_roots_to_an_agent_that_never_advertised_the_capability()
{
    // Handing an agent its own words back is fidelity to the session being
    // reopened rather than a claim about a capability (§7.7): a reopen gated
    // into asking for less would be reopening a different session from the one
    // it named. What the advertisement gates is the *control*, and this agent
    // offers none for the user to have changed anything with.
    let inspector = Inspector::new();
    inspector.connect(&advertising(r#"{"loadSession":true}"#, &[OPENED]).factory());
    inspector.initialize().await.expect("it answered");
    inspector
        .new_session("/tmp", None)
        .await
        .expect("a session");

    inspector
        .restore_session(&s1(&["/srv/reported"]), Restore::Load, None)
        .await
        .expect("it reopened the session");

    let asked = frame(&inspector, "session/load");
    assert!(
        asked.contains(r#""additionalDirectories":["/srv/reported"]"#),
        "the session is reopened as the agent described it: {asked}"
    );
    assert_eq!(
        inspector
            .driven()
            .of(AgentCapability::AdditionalDirectories),
        Driven::Answered,
        "and what crossed is what the record reads, whatever was advertised"
    );
}

#[tokio::test]
async fn roots_asked_of_an_agent_that_is_no_longer_there_are_recorded_as_refused() {
    // A call that never became a frame has no reader event, so the record is
    // written by whoever asked — the rule the five methods already follow. An
    // empty ask is still no ask: there would have been no field on the wire, so
    // there is nothing to have driven.
    let inspector = Inspector::new();

    let refused = inspector
        .new_session("/tmp", Some(Roots::supplied("/srv/data")))
        .await
        .expect_err("there is no agent to ask");

    assert_eq!(refused, CallError::Disconnected);
    assert_eq!(
        inspector
            .driven()
            .of(AgentCapability::AdditionalDirectories),
        Driven::Refused(CallError::Disconnected)
    );

    let empty = Inspector::new();
    let _ = empty.new_session("/tmp", Some(Roots::default())).await;
    assert_eq!(
        empty.driven().of(AgentCapability::AdditionalDirectories),
        Driven::Unasked,
        "an empty control drove nothing to be refused"
    );
}
