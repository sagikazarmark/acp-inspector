//! The spawn form's recent-commands convenience (`docs/architecture.md` §9,
//! §15 q6): what the form remembers, and where it keeps it.
//!
//! Driven through the file rather than around it, because the whole promise is
//! that a command survives the process that ran it: every test that says
//! "across restarts" loads a second store from the same path, which is what a
//! restart is from this store's point of view.
//!
//! Most of it needs no agent: what is remembered is a fact about a form, not
//! about a connection. The one that spawns one is the rule the window records
//! by — an invocation is remembered because an agent *answered* — and it
//! asserts the rule rather than the wiring: the launch handler in
//! `crates/app/src/main.rs` is the caller, and this is the same two answers it
//! reads, made in the same order, without a window to read them in.

use std::path::PathBuf;

use acp_inspector_core::{AgentCommand, Inspector, RecentCommands};

mod common;

/// A command as somebody would have typed it.
fn typed(command: &str, args: &str) -> AgentCommand {
    AgentCommand {
        command: command.to_owned(),
        args: args.to_owned(),
        ..AgentCommand::default()
    }
}

/// The command names of a list, newest first — the order the form shows.
fn named(recent: &RecentCommands) -> Vec<String> {
    recent
        .entries()
        .into_iter()
        .map(|command| command.command)
        .collect()
}

#[test]
fn a_command_that_launched_is_there_after_a_restart() {
    let path = scratch("restart").join("recent.json");
    let recent = RecentCommands::load_from(&path);
    assert!(recent.entries().is_empty(), "nothing has launched yet");

    recent
        .record(&typed("npx", "@zed-industries/claude-code-acp"))
        .expect("somewhere to write");

    // The store the next launch of the app would build: same path, new value,
    // nothing carried over in memory.
    let reopened = RecentCommands::load_from(&path);
    assert_eq!(
        reopened.entries(),
        vec![typed("npx", "@zed-industries/claude-code-acp")],
        "the invocation comes back whole — the form is refilled from it"
    );
}

#[test]
fn the_newest_is_first_and_relaunching_one_moves_it_rather_than_repeating_it() {
    let path = scratch("order").join("recent.json");
    let recent = RecentCommands::load_from(&path);

    for name in ["first", "second", "third"] {
        recent.record(&typed(name, "")).expect("somewhere to write");
    }
    assert_eq!(named(&recent), ["third", "second", "first"]);

    // The whole point of the list is relaunching from it, so the common case
    // must not be the one that fills it with copies.
    recent
        .record(&typed("first", ""))
        .expect("somewhere to write");
    assert_eq!(named(&recent), ["first", "third", "second"]);
    assert_eq!(
        named(&RecentCommands::load_from(&path)),
        ["first", "third", "second"],
        "and the file says the same"
    );
}

#[test]
fn the_list_keeps_the_newest_and_forgets_the_rest() {
    let path = scratch("bounded").join("recent.json");
    let recent = RecentCommands::load_from(&path);

    let recorded = RecentCommands::CAPACITY + 3;
    for nth in 0..recorded {
        recent
            .record(&typed(&format!("agent-{nth}"), ""))
            .expect("somewhere to write");
    }

    let names = named(&recent);
    assert_eq!(names.len(), RecentCommands::CAPACITY, "bounded: {names:?}");
    assert_eq!(names[0], format!("agent-{}", recorded - 1), "newest first");
    assert!(
        !names.contains(&"agent-0".to_owned()),
        "the oldest went to make room: {names:?}"
    );
    assert_eq!(
        named(&RecentCommands::load_from(&path)).len(),
        RecentCommands::CAPACITY,
        "the file is bounded too — nothing grows on disk that the form will not show"
    );
}

#[test]
fn what_is_remembered_is_the_invocation_and_not_the_keystrokes() {
    let path = scratch("normalized").join("recent.json");
    let recent = RecentCommands::load_from(&path);

    recent
        .record(&AgentCommand {
            command: "  npx  ".to_owned(),
            args: "\n  --flag  \n\n  value\n".to_owned(),
            env: "\n ACP_LOG=debug \n".to_owned(),
            cwd: " /tmp \n".to_owned(),
        })
        .expect("somewhere to write");

    assert_eq!(
        recent.entries(),
        vec![AgentCommand {
            command: "npx".to_owned(),
            args: "--flag\nvalue".to_owned(),
            env: "ACP_LOG=debug".to_owned(),
            cwd: "/tmp".to_owned(),
        }],
        "the blank lines and the padding are what the spawn already ignores"
    );

    // And two spellings of one invocation are therefore one entry, rather than
    // two rows that would launch the same agent.
    recent
        .record(&AgentCommand {
            command: "npx\n".to_owned(),
            args: "--flag\n\nvalue  ".to_owned(),
            env: "ACP_LOG=debug".to_owned(),
            cwd: "/tmp".to_owned(),
        })
        .expect("somewhere to write");
    assert_eq!(recent.entries().len(), 1, "{:?}", recent.entries());
}

#[test]
fn a_list_that_is_not_there_yet_is_an_empty_one() {
    // The first launch of the app, which is the state it spends its first run
    // in: no file, no directory, and nothing to say about either.
    let path = scratch("first-run").join("nested").join("recent.json");
    let recent = RecentCommands::load_from(&path);
    assert!(recent.entries().is_empty());

    recent
        .record(&typed("npx", ""))
        .expect("somewhere to write");
    assert_eq!(named(&RecentCommands::load_from(&path)), ["npx"]);
}

#[test]
fn a_file_this_never_wrote_is_read_as_no_list_at_all() {
    let path = scratch("unreadable").join("recent.json");
    std::fs::write(&path, "{ this is not the file we left }").expect("a file to write");

    // A convenience is not evidence: the file is the inspector's own, and one
    // it cannot read says nothing about any agent. Starting empty is the answer
    // that keeps the form working.
    let recent = RecentCommands::load_from(&path);
    assert!(recent.entries().is_empty());

    recent
        .record(&typed("npx", ""))
        .expect("somewhere to write");
    assert_eq!(
        named(&RecentCommands::load_from(&path)),
        ["npx"],
        "and the next launch is remembered over it"
    );
}

#[test]
fn a_list_with_nowhere_to_live_says_so_rather_than_pretending() {
    // A form that half-remembers is worse than one that does not pretend to:
    // the entry is still on screen for this run, and whoever recorded it is
    // told it will not outlive the window.
    let blocked = scratch("blocked").join("in-the-way");
    std::fs::write(&blocked, "not a directory").expect("a file to write");

    let recent = RecentCommands::load_from(blocked.join("recent.json"));
    let saved = recent.record(&typed("npx", ""));

    assert!(saved.is_err(), "there is nowhere to write it: {saved:?}");
    assert_eq!(
        named(&recent),
        ["npx"],
        "and the list still holds it for this run"
    );
}

#[test]
fn the_file_is_the_shape_the_docs_describe() {
    let path = scratch("shape").join("recent.json");
    let recent = RecentCommands::load_from(&path);
    recent
        .record(&typed("npx", "@zed-industries/claude-code-acp"))
        .expect("somewhere to write");

    let written = std::fs::read_to_string(&path).expect("the file it wrote");
    let file: serde_json::Value = serde_json::from_str(&written).expect("one JSON document");

    assert_eq!(file["format"], "acp-inspector-recent");
    assert_eq!(file["version"], 1);
    let commands = file["commands"].as_array().expect("the commands");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["command"], "npx");
    assert_eq!(commands[0]["args"], "@zed-industries/claude-code-acp");
    assert_eq!(commands[0]["env"], "");
    assert_eq!(commands[0]["cwd"], "");
}

#[test]
fn where_it_lives_when_nobody_says_is_the_users_own() {
    let location = RecentCommands::location().expect("a home to keep it in");

    assert!(
        location.is_absolute(),
        "not resolved against whatever directory the app was started in: {location:?}"
    );
    assert!(
        location.ends_with("acp-inspector/recent.json"),
        "under a directory of the inspector's own: {location:?}"
    );
    assert_eq!(
        RecentCommands::load().path(),
        Some(location.as_path()),
        "and that is where the store nobody configured reads and writes"
    );
}

#[tokio::test]
async fn an_invocation_is_remembered_because_an_agent_answered() {
    let path = scratch("launched").join("recent.json");
    let recent = RecentCommands::load_from(&path);

    // The launch handler's rule, in the order it has it (`crates/app/src/main.rs`):
    // start the agent, then record if an agent described itself — which a
    // handshake that failed afterwards, at `session/new`, does not undo.
    //
    // The store and not the call, and not the status either: a connection is
    // reported optimistically and corrected a moment later by the diagnostic
    // channel (§6.1), so a command that was never there can still be "connected"
    // at the instant its launch fails. What an agent *said* is never optimistic.
    let launch = |command: AgentCommand| {
        let recent = recent.clone();
        async move {
            let inspector = Inspector::new();
            let started = inspector.start(&command).await;
            if inspector.agent().is_some() {
                recent.record(&command).expect("somewhere to write");
            }
            started
        }
    };

    let agent = AgentCommand {
        command: common::testy().display().to_string(),
        cwd: std::env::temp_dir().display().to_string(),
        ..AgentCommand::default()
    };
    launch(agent.clone()).await.expect("a session");

    // A command that cannot start is one to fix in the form it is still in, not
    // one to relaunch — so the list it would clutter never hears about it.
    let missing = AgentCommand {
        command: "no-such-agent-anywhere".to_owned(),
        ..agent.clone()
    };
    launch(missing.clone())
        .await
        .expect_err("an agent that is not there");

    assert_eq!(
        RecentCommands::load_from(&path).entries(),
        vec![agent.normalized()],
        "the one that answered, and only it"
    );
}

/// An empty directory of this test run's own.
fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("inspector-recent-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}
