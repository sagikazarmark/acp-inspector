//! The state the desktop shell renders (`docs/architecture.md` §5), driven the
//! way the shell drives it: connect an agent, watch the stores fill, disconnect.
//!
//! Everything the shell does about a connection is asserted here, without a
//! window — which is the boundary's whole claim. The agents are `/bin/sh`
//! one-liners, so what each test is about is the inspector's behaviour and not
//! the agent's.

mod common;

use std::path::Path;

use acp_inspector_core::{AgentCommand, ConnectionStatus, DiagnosticKind, Direction, Inspector};

use common::{QUIET, until};

/// One frame, one stderr line, and then quiet until stdin closes.
const GREETER: &str =
    r#"printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{}}'; echo listening >&2; cat > /dev/null"#;

fn shell(script: &str) -> AgentCommand {
    AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\n{script}"),
        ..AgentCommand::default()
    }
}

/// An agent that will not stop talking, for the tests about making it stop.
fn chatty(word: &str) -> AgentCommand {
    shell(&format!("while :; do echo {word} >&2; sleep 0.05; done"))
}

fn stderr_lines(inspector: &Inspector) -> Vec<String> {
    inspector
        .diagnostics()
        .entries()
        .into_iter()
        .filter_map(|diagnostic| match diagnostic.kind {
            DiagnosticKind::Stderr(line) => Some(line),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_inspector_starts_with_nothing_connected() {
    let inspector = Inspector::new();

    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);
    assert!(inspector.trace().is_empty());
    assert!(inspector.diagnostics().is_empty());
}

#[tokio::test]
async fn a_connected_agents_frames_and_stderr_fill_the_stores() {
    let inspector = Inspector::new();
    // Subscribed before there is anything to see, as the shell subscribes: a
    // store that only reports changes to whoever was already watching is the
    // one the UI can be a pure function of.
    let mut frames = inspector.trace().changes();
    let mut lines = inspector.diagnostics().changes();

    inspector.connect(&shell(GREETER).factory());
    assert_eq!(inspector.status(), ConnectionStatus::Connected);

    until(&mut frames, "the agent's frame to reach the trace", || {
        inspector.trace().len() == 1
    })
    .await;
    let entry = inspector.trace().entries().pop().unwrap();
    assert_eq!(entry.direction, Direction::FromAgent);
    assert_eq!(
        entry.frame.as_str(),
        r#"{"jsonrpc":"2.0","id":1,"result":{}}"#
    );

    until(&mut lines, "the agent's stderr to reach the log", || {
        !inspector.diagnostics().is_empty()
    })
    .await;
    assert_eq!(stderr_lines(&inspector), ["listening"]);
}

#[tokio::test]
async fn a_spawn_failure_says_so_with_the_evidence_already_logged() {
    // The flow the shell auto-surfaces (§9): it opens the stderr console on the
    // status, so the output it opens on has to be there first.
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();

    inspector.connect(
        &AgentCommand {
            command: "inspector-no-such-agent-71b3".into(),
            ..AgentCommand::default()
        }
        .factory(),
    );

    until(&mut status, "the failed start to be reported", || {
        inspector.status() == ConnectionStatus::FailedToStart
    })
    .await;

    let evidence = inspector.diagnostics().entries();
    let Some(DiagnosticKind::SpawnFailed { command, .. }) =
        evidence.first().map(|diagnostic| &diagnostic.kind)
    else {
        panic!("the console opens on the reason the agent never started: {evidence:?}");
    };
    assert!(
        command.contains("inspector-no-such-agent-71b3"),
        "{command}"
    );
}

#[tokio::test]
async fn an_agent_that_ends_on_its_own_is_reported_lost_with_its_last_words() {
    // The common shape of "it didn't start": the agent execs, complains, and
    // dies. Nobody asked it to go, so this is news — the shell opens the
    // console on it exactly as it does on a failed spawn (§9).
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();

    inspector.connect(&shell("echo goodbye >&2; exit 1").factory());

    until(&mut status, "the agent's exit to reach the status", || {
        inspector.status() == ConnectionStatus::Lost
    })
    .await;

    // Stderr before the exit, as the transport promises: an agent that explains
    // itself and then dies has the explanation above the death in the console.
    let log = inspector.diagnostics().entries();
    assert!(
        matches!(&log[0].kind, DiagnosticKind::Stderr(line) if line == "goodbye"),
        "{log:?}"
    );
    assert!(
        matches!(log[1].kind, DiagnosticKind::AgentExited(_)),
        "{log:?}"
    );

    // There is nothing left to disconnect, and saying "disconnected" over the
    // news would be the window forgetting what it just told you.
    inspector.disconnect();
    assert_eq!(inspector.status(), ConnectionStatus::Lost);
}

#[tokio::test]
async fn disconnecting_ends_the_agent_and_says_so() {
    let inspector = Inspector::new();
    let mut lines = inspector.diagnostics().changes();

    inspector.connect(&chatty("tick").factory());
    until(&mut lines, "the agent to get going", || {
        inspector.diagnostics().len() >= 2
    })
    .await;

    inspector.disconnect();
    assert_eq!(inspector.status(), ConnectionStatus::Disconnected);

    // An agent that would happily talk forever stops being recorded, which is
    // the observable half of "the connection is torn down".
    let logged = inspector.diagnostics().len();
    tokio::time::sleep(QUIET).await;
    assert_eq!(inspector.diagnostics().len(), logged);
}

#[tokio::test]
async fn a_second_connection_replaces_the_first() {
    // One connection at a time (`CONTEXT.md`, *Connection*). The stores stay:
    // what the first agent said is evidence, and connecting to a second one is
    // not a reason to lose it.
    let inspector = Inspector::new();
    let mut lines = inspector.diagnostics().changes();

    inspector.connect(&chatty("alpha").factory());
    until(&mut lines, "the first agent to get going", || {
        stderr_lines(&inspector).len() >= 2
    })
    .await;

    inspector.connect(&chatty("beta").factory());
    until(&mut lines, "the second agent to get going", || {
        stderr_lines(&inspector).iter().any(|line| line == "beta")
    })
    .await;
    assert_eq!(inspector.status(), ConnectionStatus::Connected);

    let alphas = |inspector: &Inspector| {
        stderr_lines(inspector)
            .iter()
            .filter(|line| *line == "alpha")
            .count()
    };
    let spoken = alphas(&inspector);
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        alphas(&inspector),
        spoken,
        "the first agent is gone, not merely ignored"
    );
    assert!(
        stderr_lines(&inspector).contains(&"alpha".to_owned()),
        "and what it said is still in the log"
    );
}

#[tokio::test]
async fn the_spawn_forms_fields_reach_the_agent() {
    let inspector = Inspector::new();
    let mut lines = inspector.diagnostics().changes();

    inspector.connect(
        &AgentCommand {
            command: " /bin/sh ".into(),
            args: "-c\nprintf '%s\\n' \"$INSPECTOR_TEST_MARK\" >&2; pwd >&2; cat > /dev/null"
                .into(),
            env: "\nINSPECTOR_TEST_MARK=carried\nhalf-typed\n".into(),
            cwd: "/tmp".into(),
        }
        .factory(),
    );

    until(&mut lines, "the agent to report its environment", || {
        stderr_lines(&inspector).len() >= 2
    })
    .await;

    let reported = stderr_lines(&inspector);
    assert_eq!(reported[0], "carried", "the environment carried");
    assert!(
        reported[1].ends_with("/tmp"),
        "the cwd carried: {reported:?}"
    );
}

#[test]
fn a_spawn_form_is_one_argument_per_line() {
    // No quoting rules to learn: a path with spaces in it is a line, and a
    // blank line is somebody halfway through typing. The doubled space is the
    // point of the assertion — it survives, which splitting the field on
    // whitespace could not manage.
    let command = AgentCommand {
        command: "agent".into(),
        args: "--config\n/home/me/my  agents/acp.toml\n\n  --verbose  \n".into(),
        ..AgentCommand::default()
    };

    assert_eq!(
        command.factory().command_line(),
        "agent --config /home/me/my  agents/acp.toml --verbose"
    );
    assert!(command.is_runnable());
    assert!(!AgentCommand::default().is_runnable());
}

#[test]
fn a_spawn_form_names_the_sessions_working_directory() {
    // ACP wants an absolute path (§7.1) and a form is where people type `.` and
    // leave fields empty, so the two are reconciled here rather than in whatever
    // screen happens to be sending `session/new`.
    let here = std::env::current_dir().expect("a working directory");
    let form = |cwd: &str| AgentCommand {
        command: "agent".into(),
        cwd: cwd.into(),
        ..AgentCommand::default()
    };

    assert_eq!(
        form("/srv/project").session_cwd().as_deref(),
        Some(Path::new("/srv/project"))
    );
    assert_eq!(
        form("  ").session_cwd(),
        Some(here.clone()),
        "an empty field is the inspector's own directory, which is the one the agent starts in"
    );
    assert_eq!(
        form("nested/crate").session_cwd(),
        Some(here.join("nested/crate"))
    );
}
