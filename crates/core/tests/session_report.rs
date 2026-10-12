//! What the agent reports about a session as it goes (`docs/architecture.md`
//! §7.5): the title and last activity a `session_info_update` states, held per
//! session as the last thing said.
//!
//! Driven through the public inspector API only (§12). Testy sends no
//! `session_info_update`, so every agent here is a `/bin/sh` one-liner.

mod common;

use acp_inspector_core::{
    AgentCommand, EntryKind, Inspector, Restore, SessionReport, Stated, StatedBy, v1,
};

use common::{shell, until};

/// What an agent answers `initialize` with, advertising `session/load`.
const DESCRIBED: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}"#;
/// The `session/new` answer, the second call this client makes.
const OPENED: &str = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}"#;
/// The `session/load` answer, the third.
const RELOADED: &str = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;

/// One `read`-and-`printf`: the answer to whatever was asked, and anything the
/// agent says after it.
fn exchange(frames: &[&str]) -> String {
    let printed: String = frames.iter().map(|frame| format!(" '{frame}'")).collect();
    format!("read _; printf '%s\\n'{printed}; ")
}

/// A `session_info_update` in `s1`.
fn info(fields: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"session_info_update",{fields}}}}}}}"#
    )
}

/// An agent that opens `s1`, says `then` without being prompted, and answers
/// whatever it is asked next with `answers`.
fn reporting(then: &[&str], answers: &[&str]) -> AgentCommand {
    let mut opening = vec![OPENED];
    opening.extend(then);
    shell(&format!(
        "{}{}{}cat > /dev/null",
        exchange(&[DESCRIBED]),
        exchange(&opening),
        exchange(answers),
    ))
}

fn s1() -> v1::SessionId {
    v1::SessionId::new("s1")
}

#[tokio::test]
async fn the_last_title_the_agent_stated_is_the_one_held_whether_or_not_a_turn_is_running() {
    // An agent names a session when it has something to name it after, which
    // is often between turns. A title it states is the session's title from
    // then on — and a later one replaces it rather than being refused because
    // the session already had one.
    let first = info(r#""title":"Untitled""#);
    let renamed = info(r#""title":"Fix the flaky build","updatedAt":"2026-10-12T09:00:00Z""#);
    let inspector = Inspector::new();
    inspector
        .start(&reporting(&[&first, &renamed], &[]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.session_reports_changes();
    until(&mut changes, "the rename to land", || {
        inspector.session_reports().title(&s1(), None)
            == Some(Stated {
                value: Some("Fix the flaky build".to_owned()),
                by: StatedBy::Update,
            })
    })
    .await;
    assert_eq!(
        inspector
            .session_reports()
            .updated_at(&s1(), None)
            .and_then(|stated| stated.value)
            .as_deref(),
        Some("2026-10-12T09:00:00Z")
    );

    // The timeline keeps both statements: the store is the latest, the view
    // is the history.
    let stated = inspector
        .timeline()
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                EntryKind::Update(v1::SessionUpdate::SessionInfoUpdate(_))
            )
        })
        .count();
    assert_eq!(stated, 2);
}

#[tokio::test]
async fn a_cleared_title_is_held_as_cleared() {
    let named = info(r#""title":"Named""#);
    let cleared = info(r#""title":null"#);
    let inspector = Inspector::new();
    inspector
        .start(&reporting(&[&named, &cleared], &[]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.session_reports_changes();
    until(&mut changes, "the title to be cleared", || {
        inspector
            .session_reports()
            .of(&s1())
            .is_some_and(|report| report.title == Some(None))
    })
    .await;
    assert_eq!(
        inspector.session_reports().title(
            &s1(),
            Some(&v1::SessionInfo::new("s1", "/tmp").title("Listed"))
        ),
        Some(Stated {
            value: None,
            by: StatedBy::Update,
        }),
        "the agent cleared it, which outranks a listing that still names it"
    );
}

#[tokio::test]
async fn a_reopened_session_is_described_by_what_its_replay_says_and_nothing_before() {
    // A load replays ahead of its answer, so what it says lands before the
    // session is live again; what was said about the session before the load
    // is not part of that account.
    let named = info(r#""title":"Before the load""#);
    let replayed = info(r#""updatedAt":"2026-10-12T10:00:00Z""#);
    let inspector = Inspector::new();
    inspector
        .start(&reporting(&[&named], &[&replayed, RELOADED]))
        .await
        .expect("the agent opens a session");

    let mut changes = inspector.session_reports_changes();
    until(&mut changes, "the first title to land", || {
        inspector.session_reports().of(&s1()).is_some()
    })
    .await;

    inspector
        .restore_session(&v1::SessionInfo::new("s1", "/tmp"), Restore::Load, None)
        .await
        .expect("the agent reloads the session");

    let reports = inspector.session_reports();
    let report: &SessionReport = reports.of(&s1()).expect("the replay said something");
    assert_eq!(report.title, None, "the title from before the load is gone");
    assert_eq!(
        report.updated_at,
        Some(Some("2026-10-12T10:00:00Z".to_owned()))
    );
}
