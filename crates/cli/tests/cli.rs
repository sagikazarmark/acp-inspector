//! The CLI as CI runs it: a real binary, real agents, and the exit code.
//!
//! Three agents drive it. **Testy** (`docs/architecture.md` §12) for a real
//! agent's permission traffic; the **acceptance agent**
//! (`scripts/acceptance-agent.py`) for a turn, a refused prompt and the export;
//! and scripts in `tests/agents/` for what neither will do — never answering,
//! breaking a MUST the inspector checks, offering exactly the permission
//! options a policy is tested on, ending a turn with any stop reason, and
//! refusing to stop.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// Nothing here takes seconds except the timeout test, which takes one. The
/// limit is so a hang fails the test rather than the CI job.
const PATIENCE: Duration = Duration::from_secs(60);

const CALLBACKS: &str = r#"{"command":"run_scenario","scenario":"callbacks"}"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Testy, where `just testy` builds it or where `TESTY_BIN` says.
fn testy() -> PathBuf {
    if let Some(path) = std::env::var_os("TESTY_BIN") {
        return PathBuf::from(path);
    }
    root().join(".testy/bin/testy").canonicalize().unwrap_or_else(|error| {
        panic!("Testy is not built. Run `just testy` at the repository root, or set TESTY_BIN. ({error})")
    })
}

fn acceptance_agent() -> String {
    root()
        .join("scripts/acceptance-agent.py")
        .to_string_lossy()
        .into_owned()
}

fn agent_script(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/agents")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// A directory of this test's own, emptied first.
fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "acp-inspector-cli-test-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// Starts the CLI in `directory`, or in this test's own directory.
fn start(directory: Option<&Path>, arguments: &[&str]) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_acp-inspector-cli"));
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    command.spawn().expect("the CLI starts")
}

/// Runs the CLI to completion, failing the test if it does not finish.
fn cli(arguments: &[&str]) -> Output {
    finish(start(None, arguments), arguments)
}

/// Runs the CLI to completion from `directory`.
fn cli_in(directory: &Path, arguments: &[&str]) -> Output {
    finish(start(Some(directory), arguments), arguments)
}

/// Waits for a started CLI, failing the test if it does not finish.
fn finish(mut child: Child, arguments: &[&str]) -> Output {
    let started = Instant::now();
    while child
        .try_wait()
        .expect("the CLI can be waited on")
        .is_none()
    {
        if started.elapsed() > PATIENCE {
            let _ = child.kill();
            panic!("the CLI ran for more than {PATIENCE:?}: {arguments:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().expect("the CLI's output")
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("the CLI exited rather than being killed")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn summary(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "--json printed something that is not JSON ({error}):\n{}\nstderr:\n{}",
            stdout(output),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// The export, parsed: the header and every record after it, each checked
/// against the pinned format (`docs/trace-export.md`).
fn trace(path: &Path) -> (Value, Vec<Value>) {
    let text = std::fs::read_to_string(path).expect("the export was written");
    assert!(text.ends_with('\n'), "every line is terminated");
    let mut lines = text.lines();
    let header: Value = serde_json::from_str(lines.next().expect("a header")).expect("JSON");
    assert_eq!(header["type"], "header");
    assert_eq!(header["format"], "acp-inspector-trace");
    assert_eq!(header["version"], 1);
    let records: Vec<Value> = lines
        .map(|line| serde_json::from_str(line).expect("every record is JSON"))
        .collect();
    assert_eq!(header["frames"].as_u64(), Some(records.len() as u64));
    for record in &records {
        assert_eq!(record["type"], "frame");
        assert!(record["at"].is_i64());
        assert!(matches!(
            record["direction"].as_str(),
            Some("client -> agent" | "agent -> client")
        ));
        let frame = record["frame"].as_str().expect("a frame is a string");
        serde_json::from_str::<Value>(frame).expect("these agents speak JSON");
    }
    (header, records)
}

/// The frames this client sent, verbatim.
fn sent(records: &[Value]) -> Vec<&str> {
    records
        .iter()
        .filter(|record| record["direction"] == "client -> agent")
        .filter_map(|record| record["frame"].as_str())
        .collect()
}

#[test]
fn a_prompted_run_exits_zero_and_exports_the_trace_the_app_would() {
    let directory = scratch("prompted");
    let export = directory.join("trace.jsonl");
    let cwd = directory.join("work");
    std::fs::create_dir(&cwd).unwrap();

    let output = cli(&[
        "run",
        "--cwd",
        cwd.to_str().unwrap(),
        "--prompt",
        "hello",
        "--prompt",
        "and again",
        "--export",
        export.to_str().unwrap(),
        "--json",
        "--",
        "python3",
        &acceptance_agent(),
    ]);
    let json = summary(&output);
    assert_eq!(code(&output), 0, "{json:#}");
    assert_eq!(json["outcome"], "ok");
    assert_eq!(json["agent"]["name"], "acceptance-agent");
    assert_eq!(json["sessionId"], "acceptance-1");
    let turns = json["turns"].as_array().expect("turns");
    assert_eq!(turns.len(), 2);
    for turn in turns {
        assert_eq!(turn["stopReason"], "end_turn");
        assert!(
            turn["message"]
                .as_str()
                .unwrap()
                .contains("acceptance-agent")
        );
    }
    assert_eq!(json["annotations"], Value::Array(Vec::new()));

    let (header, records) = trace(&export);
    assert_eq!(header["dropped"], 0);
    assert_eq!(json["trace"]["frames"], header["frames"]);
    let sent = sent(&records);
    assert_eq!(
        sent.iter()
            .filter(|frame| frame.contains(r#""method":"session/prompt""#))
            .count(),
        2
    );
    let new = sent
        .iter()
        .find(|frame| frame.contains(r#""method":"session/new""#))
        .expect("session/new was sent");
    let new: Value = serde_json::from_str(new).unwrap();
    assert_eq!(
        Path::new(new["params"]["cwd"].as_str().unwrap()),
        cwd,
        "--cwd is the session's cwd, absolute"
    );

    // And `inspect` reads it back.
    let inspected = cli(&["inspect", "--json", export.to_str().unwrap()]);
    assert_eq!(code(&inspected), 0);
    let counts = summary(&inspected);
    assert_eq!(counts["frames"], header["frames"]);
    assert_eq!(counts["connections"], 1);
    assert_eq!(counts["directions"]["client -> agent"]["session/prompt"], 2);
    assert_eq!(
        counts["directions"]["agent -> client"]["session/prompt → result"],
        2
    );

    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_run_against_testy_prints_a_human_summary() {
    let output = cli(&["run", "--prompt", "hello", "--", testy().to_str().unwrap()]);
    let text = stdout(&output);
    assert_eq!(code(&output), 0, "{text}");
    assert!(text.contains("turn 1"), "{text}");
    assert!(text.contains("end_turn"), "{text}");
    assert!(text.contains("conformance  no annotations"), "{text}");
    assert!(text.contains("result       completed (exit 0)"), "{text}");
}

/// Runs Testy's callbacks scenario under `policy` and answers with the summary
/// and the frames this client sent.
fn callbacks(policy: &str) -> (Output, Value, Vec<String>) {
    let directory = scratch(&format!("callbacks-{policy}"));
    let export = directory.join("trace.jsonl");
    let output = cli(&[
        "run",
        "--permission",
        policy,
        "--prompt",
        CALLBACKS,
        "--export",
        export.to_str().unwrap(),
        "--json",
        "--",
        testy().to_str().unwrap(),
    ]);
    let json = summary(&output);
    let (_, records) = trace(&export);
    let sent = sent(&records).into_iter().map(str::to_owned).collect();
    let _ = std::fs::remove_dir_all(directory);
    (output, json, sent)
}

fn answered_with(sent: &[String], option: &str) -> bool {
    sent.iter().any(|frame| {
        frame.contains(r#""outcome":"selected""#)
            && frame.contains(&format!(r#""optionId":"{option}""#))
    })
}

#[test]
fn allow_selects_the_allow_once_option() {
    let (output, json, sent) = callbacks("allow");
    assert_eq!(code(&output), 0, "{json:#}");
    let permission = &json["permissions"][0];
    assert_eq!(permission["answer"], "selected");
    assert_eq!(permission["kind"], "allow_once");
    assert!(answered_with(&sent, "allow_once"), "{sent:#?}");
    assert_eq!(json["turns"][0]["stopReason"], "end_turn");
}

#[test]
fn reject_selects_a_reject_option() {
    let (output, json, sent) = callbacks("reject");
    assert_eq!(code(&output), 0, "{json:#}");
    assert_eq!(json["permissions"][0]["kind"], "reject_once");
    assert!(answered_with(&sent, "reject_once"), "{sent:#?}");
    assert!(!answered_with(&sent, "allow_once"));
}

#[test]
fn ask_rejects_because_nobody_is_there_to_ask() {
    let (output, json, sent) = callbacks("ask");
    assert_eq!(code(&output), 0, "{json:#}");
    assert_eq!(json["permissionPolicy"], "ask");
    assert_eq!(json["permissions"][0]["kind"], "reject_once");
    assert!(answered_with(&sent, "reject_once"), "{sent:#?}");
}

#[test]
fn an_agent_that_never_answers_times_out_with_three_and_still_exports() {
    let directory = scratch("timeout");
    let export = directory.join("trace.jsonl");
    let started = Instant::now();
    let output = cli(&[
        "run",
        "--timeout",
        "1",
        "--prompt",
        "hello?",
        "--export",
        export.to_str().unwrap(),
        "--",
        "python3",
        &agent_script("silent.py"),
    ]);
    let text = stdout(&output);
    assert_eq!(code(&output), 3, "{text}");
    assert!(text.contains("timed out after 1 s"), "{text}");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "stopping a silent agent is quick: it exits when stdin closes"
    );
    let (_, records) = trace(&export);
    assert!(
        sent(&records)
            .iter()
            .any(|frame| frame.contains(r#""method":"initialize""#)),
        "the export holds what was sent before the time ran out"
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_refused_prompt_is_a_failure() {
    let output = cli(&[
        "run",
        "--prompt",
        "hello",
        "--json",
        "--",
        "python3",
        &acceptance_agent(),
        "--refuse",
        "session/prompt",
    ]);
    let json = summary(&output);
    assert_eq!(code(&output), 1, "{json:#}");
    assert_eq!(json["outcome"], "failed");
    assert!(
        json["turns"][0]["error"]
            .as_str()
            .unwrap()
            .contains("fixture refused session/prompt"),
        "{json:#}"
    );
}

#[test]
fn an_agent_that_does_not_start_is_a_failure() {
    let output = cli(&["run", "--", "/nonexistent/acp-agent"]);
    let text = stdout(&output);
    assert_eq!(code(&output), 1, "{text}");
    assert!(text.contains("the agent did not start"), "{text}");
}

#[test]
fn a_broken_must_exits_two() {
    let output = cli(&[
        "run",
        "--prompt",
        "ask me twice",
        "--json",
        "--",
        "python3",
        &agent_script("reused_elicitation.py"),
    ]);
    let json = summary(&output);
    assert_eq!(code(&output), 2, "{json:#}");
    assert_eq!(json["outcome"], "conformance");
    assert_eq!(json["turns"][0]["stopReason"], "end_turn");
    let annotation = &json["annotations"][0];
    assert!(
        annotation["requires"]
            .as_str()
            .unwrap()
            .contains("elicitationId"),
        "{json:#}"
    );
}

#[test]
fn usage_errors_exit_sixty_four() {
    let missing = scratch("usage").join("missing.txt");
    for arguments in [
        &[][..],
        &["frobnicate"],
        &["run"],
        &["run", "--prompt", "hello"],
        &["run", "--permission", "always", "--", "agent"],
        &["run", "--timeout", "never", "--", "agent"],
        &["run", "--bogus", "--", "agent"],
        &[
            "run",
            "--prompt-file",
            missing.to_str().unwrap(),
            "--",
            "agent",
        ],
        &[
            "run",
            "--prompt-file",
            "-",
            "--prompt-file",
            "-",
            "--",
            "agent",
        ],
        &["inspect"],
    ] {
        let output = cli(arguments);
        assert_eq!(code(&output), 64, "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--help"),
            "a usage error points at the help: {arguments:?}"
        );
    }
}

#[test]
fn help_and_version() {
    let help = cli(&["--help"]);
    assert_eq!(code(&help), 0);
    let text = stdout(&help);
    assert!(text.contains("acp-inspector-cli run"), "{text}");
    assert!(text.contains("Exit codes"), "{text}");

    let version = cli(&["--version"]);
    assert_eq!(code(&version), 0);
    assert_eq!(
        stdout(&version).trim(),
        format!("acp-inspector-cli {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn inspect_refuses_a_file_that_is_not_a_trace() {
    let directory = scratch("not-a-trace");
    let path = directory.join("notes.jsonl");
    std::fs::write(
        &path,
        "{\"type\":\"header\",\"format\":\"something-else\"}\n",
    )
    .unwrap();
    let output = cli(&["inspect", path.to_str().unwrap()]);
    assert_eq!(code(&output), 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("not an acp-inspector-trace"));
    let _ = std::fs::remove_dir_all(directory);
}

/// Runs `scripted.py` through one prompt under `policy`, and answers with the
/// summary and the frames this client sent.
fn scripted(name: &str, policy: &str, script: &str) -> (Output, Value, Vec<String>) {
    let directory = scratch(name);
    let export = directory.join("trace.jsonl");
    let output = cli(&[
        "run",
        "--permission",
        policy,
        "--prompt",
        script,
        "--export",
        export.to_str().unwrap(),
        "--json",
        "--",
        "python3",
        &agent_script("scripted.py"),
    ]);
    let json = summary(&output);
    let (_, records) = trace(&export);
    let sent = sent(&records).into_iter().map(str::to_owned).collect();
    let _ = std::fs::remove_dir_all(directory);
    (output, json, sent)
}

#[test]
fn allow_cancels_the_turn_rather_than_reject_when_no_allow_once_is_offered() {
    let (output, json, sent) = scripted(
        "allow-no-allow-once",
        "allow",
        r#"{"permission":["allow_always","reject_once"]}"#,
    );
    assert_eq!(
        json["permissions"][0]["answer"], "cancelledTurn",
        "{json:#}"
    );
    assert!(!answered_with(&sent, "reject_once"), "{sent:#?}");
    assert!(!answered_with(&sent, "allow_always"), "{sent:#?}");
    assert!(
        sent.iter()
            .any(|frame| frame.contains(r#""method":"session/cancel""#)),
        "{sent:#?}"
    );
    assert_eq!(json["turns"][0]["stopReason"], "cancelled");
    assert_eq!(code(&output), 4, "a cancelled turn did not end on its own");
}

#[test]
fn reject_prefers_reject_once_to_an_earlier_reject_always() {
    let (output, json, sent) = scripted(
        "reject-once",
        "reject",
        r#"{"permission":["allow_once","reject_always","reject_once"]}"#,
    );
    assert_eq!(code(&output), 0, "{json:#}");
    assert_eq!(json["permissions"][0]["kind"], "reject_once");
    assert!(answered_with(&sent, "reject_once"), "{sent:#?}");
    assert!(!answered_with(&sent, "reject_always"), "{sent:#?}");
}

#[test]
fn reject_and_ask_cancel_the_turn_when_only_reject_always_is_offered() {
    for policy in ["reject", "ask"] {
        let (output, json, sent) = scripted(
            &format!("{policy}-no-reject-once"),
            policy,
            r#"{"permission":["allow_once","reject_always"]}"#,
        );
        assert_eq!(
            json["permissions"][0]["answer"], "cancelledTurn",
            "{json:#}"
        );
        assert!(
            !answered_with(&sent, "reject_always"),
            "{policy}: {sent:#?}"
        );
        assert!(!answered_with(&sent, "allow_once"), "{policy}: {sent:#?}");
        assert_eq!(code(&output), 4, "{policy}: {json:#}");
    }
}

#[test]
fn a_turn_that_ends_other_than_end_turn_exits_four() {
    let (output, json, _) = scripted("refusal", "ask", r#"{"stopReason":"refusal"}"#);
    assert_eq!(code(&output), 4, "{json:#}");
    assert_eq!(json["outcome"], "stopped");
    assert_eq!(json["turns"][0]["stopReason"], "refusal");
    assert_eq!(
        json["summary"], "completed, but turn 1 ended with `refusal`",
        "{json:#}"
    );
}

#[test]
fn an_export_that_cannot_be_written_fails_the_outcome_too() {
    let directory = scratch("unwritable-export");
    let export = directory.join("missing/trace.jsonl");
    let output = cli(&[
        "run",
        "--prompt",
        "hello",
        "--export",
        export.to_str().unwrap(),
        "--json",
        "--",
        "python3",
        &acceptance_agent(),
    ]);
    let json = summary(&output);
    assert_eq!(code(&output), 1, "{json:#}");
    assert_eq!(json["outcome"], "failed");
    assert!(
        json["summary"]
            .as_str()
            .unwrap()
            .starts_with("completed, but the export could not be written: "),
        "{json:#}"
    );
    assert!(json["trace"]["exportError"].is_string(), "{json:#}");
    let _ = std::fs::remove_dir_all(directory);
}

/// The `session/new` the CLI sent, as JSON.
fn session_new(export: &Path) -> Value {
    let (_, records) = trace(export);
    let frame = sent(&records)
        .into_iter()
        .find(|frame| frame.contains(r#""method":"session/new""#))
        .expect("session/new was sent");
    serde_json::from_str(frame).unwrap()
}

#[cfg(unix)]
#[test]
fn a_relative_command_is_found_from_where_it_was_typed_not_from_cwd() {
    use std::os::unix::fs::PermissionsExt;

    let directory = scratch("relative-command");
    std::fs::create_dir_all(directory.join("bin")).unwrap();
    std::fs::create_dir_all(directory.join("work")).unwrap();
    let agent = directory.join("bin/agent");
    std::fs::write(
        &agent,
        format!("#!/bin/sh\nexec python3 '{}' \"$@\"\n", acceptance_agent()),
    )
    .unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let export = directory.join("trace.jsonl");

    let output = cli_in(
        &directory,
        &[
            "run",
            "--cwd",
            "work/",
            "--prompt",
            "hello",
            "--export",
            export.to_str().unwrap(),
            "--",
            "./bin/agent",
        ],
    );
    let text = stdout(&output);
    assert_eq!(code(&output), 0, "{text}");
    let cwd = session_new(&export)["params"]["cwd"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        Path::new(&cwd),
        directory.join("work").canonicalize().unwrap(),
        "absolute, and without the trailing separator"
    );
    assert!(!cwd.ends_with('/'), "{cwd}");
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn the_session_cwd_has_no_dot_or_trailing_separator() {
    let directory = scratch("dot-cwd");
    let export = directory.join("trace.jsonl");
    let output = cli_in(
        &directory,
        &[
            "run",
            "--cwd",
            ".",
            "--export",
            export.to_str().unwrap(),
            "--",
            "python3",
            &acceptance_agent(),
        ],
    );
    assert_eq!(code(&output), 0, "{}", stdout(&output));
    assert_eq!(
        Path::new(session_new(&export)["params"]["cwd"].as_str().unwrap()),
        directory.canonicalize().unwrap()
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_missing_cwd_is_named_as_the_directory_it_is() {
    let directory = scratch("missing-cwd");
    let output = cli_in(
        &directory,
        &[
            "run",
            "--cwd",
            "nowhere",
            "--",
            "python3",
            &acceptance_agent(),
        ],
    );
    let text = stdout(&output);
    assert_eq!(code(&output), 1, "{text}");
    assert!(text.contains("the working directory `"), "{text}");
    assert!(text.contains("nowhere` cannot be used"), "{text}");
    assert!(
        !text.contains("could not start"),
        "nothing was spawned: {text}"
    );
    let _ = std::fs::remove_dir_all(directory);
}

/// Starts the CLI on `lingering.py` and waits for the agent to be running,
/// answering with the CLI, the agent's pid, the export's path and the scratch
/// directory.
#[cfg(unix)]
fn lingering(name: &str) -> (Child, u32, PathBuf, PathBuf) {
    let directory = scratch(name);
    let pidfile = directory.join("agent.pid");
    let export = directory.join("trace.jsonl");
    let child = start(
        None,
        &[
            "run",
            "--prompt",
            "hello?",
            "--export",
            export.to_str().unwrap(),
            "--",
            "python3",
            &agent_script("lingering.py"),
            pidfile.to_str().unwrap(),
        ],
    );
    let started = Instant::now();
    let pid = loop {
        if let Ok(pid) = std::fs::read_to_string(&pidfile) {
            break pid.trim().parse().expect("a pid");
        }
        assert!(started.elapsed() < PATIENCE, "the agent never started");
        std::thread::sleep(Duration::from_millis(20));
    };
    (child, pid, export, directory)
}

#[cfg(unix)]
fn signal(pid: u32, name: &str) {
    let status = Command::new("kill")
        .args([format!("-{name}"), pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(status.success(), "kill -{name} {pid}");
}

/// Whether `pid` is gone, given a moment for an orphan to be reaped.
#[cfg(unix)]
fn gone(pid: u32) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        let alive = Command::new("kill")
            .args(["-0".to_owned(), pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .expect("kill runs")
            .success();
        if !alive {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(unix)]
#[test]
fn sigterm_stops_the_agent_exports_and_exits_one_hundred_forty_three() {
    let (child, agent, export, directory) = lingering("sigterm");
    signal(child.id(), "TERM");
    let output = finish(child, &["sigterm"]);
    let text = stdout(&output);
    assert_eq!(code(&output), 143, "{text}");
    assert!(text.contains("interrupted by SIGTERM"), "{text}");
    assert!(gone(agent), "the agent outlived the CLI");
    let (_, records) = trace(&export);
    assert!(
        sent(&records)
            .iter()
            .any(|frame| frame.contains(r#""method":"initialize""#)),
        "the export holds what was sent"
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[cfg(unix)]
#[test]
fn a_second_signal_while_stopping_kills_the_agent_and_exits_at_once() {
    let (child, agent, export, directory) = lingering("second-signal");
    signal(child.id(), "HUP");
    std::thread::sleep(Duration::from_millis(300));
    let forced = Instant::now();
    signal(child.id(), "INT");
    let output = finish(child, &["second-signal"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(code(&output), 130, "{}", stdout(&output));
    assert!(
        forced.elapsed() < Duration::from_secs(2),
        "the second signal does not wait for the graceful stop"
    );
    assert!(
        stderr.contains("SIGINT while stopping the agent"),
        "{stderr}"
    );
    assert!(gone(agent), "the agent outlived the CLI");
    assert!(!export.exists(), "the forced path writes no export");
    let _ = std::fs::remove_dir_all(directory);
}
