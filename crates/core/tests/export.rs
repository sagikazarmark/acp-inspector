//! The JSONL trace export (`docs/architecture.md` §10, `docs/trace-export.md`):
//! the trace, as a file you can hand to someone.
//!
//! **The records are a contract**, so this file asserts the shape itself and
//! not just that something came out — ACP Wiretap and replay tooling read what
//! is pinned here, and a schema nothing checks is a schema that drifts.
//!
//! Most of it is driven over the public transport ends rather than a
//! subprocess, the way `trace.rs` drives the decorator: what an export contains
//! is a fact about frames and not about who sent them, and a test that had to
//! talk an agent into emitting ten thousand of them would be measuring Testy
//! (§12). The two that do spawn one are the two that are about the whole stack
//! — the export reading the same capture the timeline decorates, and the window's
//! own path through [`Inspector`].

mod common;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use acp_inspector_core::{
    AgentCommand, ConnectionStatus, EntryKind, Frame, Inspector, Trace, connection,
};
use serde_json::Value;

use common::until;

/// The export, one parsed JSON record per line.
///
/// Parsed rather than string-matched, because JSONL's promise is that each line
/// is a whole JSON document — a test that read them any other way would not be
/// reading them the way a consumer will.
fn records(export: &str) -> Vec<Value> {
    assert!(
        export.ends_with('\n'),
        "every record is a whole line, including the last: {export:?}"
    );
    export
        .lines()
        .map(|line| serde_json::from_str(line).expect("every line is one JSON record"))
        .collect()
}

#[tokio::test]
async fn an_export_is_a_self_describing_header_and_one_record_per_frame() {
    let (client, mut transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let (outgoing, mut incoming, _diagnostics) = client.into_parts();

    let asked = Frame::new(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    outgoing
        .send(asked.clone())
        .await
        .expect("a live transport");
    transport.outgoing.recv().await.expect("the frame");

    let answered = Frame::new(r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#);
    transport
        .incoming
        .send(answered.clone())
        .await
        .expect("a live client");
    incoming.recv().await.expect("the frame");

    let records = records(&trace.export().to_string());
    assert_eq!(records.len(), 3, "a header and the two frames");

    let header = &records[0];
    assert_eq!(header["type"], "header");
    assert_eq!(header["format"], "acp-inspector-trace");
    assert_eq!(header["version"], 1);
    assert_eq!(header["frames"], 2);
    assert_eq!(header["dropped"], 0);
    assert!(
        header["warning"]
            .as_str()
            .expect("a warning")
            .contains("nothing is redacted"),
        "the header says what is in the file: {header}"
    );

    // One record per frame, in the order the wire had them, each carrying the
    // frame verbatim as a string — never a re-encoding of the JSON, which would
    // make the export a decoding of the wire rather than the wire.
    assert_eq!(records[1]["type"], "frame");
    assert_eq!(records[1]["direction"], "client -> agent");
    assert_eq!(records[1]["encoding"], "utf8");
    assert_eq!(records[1]["frame"], asked.as_str());
    assert_eq!(records[2]["direction"], "agent -> client");
    assert_eq!(records[2]["frame"], answered.as_str());

    let entries = trace.entries();
    assert_eq!(
        records[1]["at"].as_i64().expect("epoch milliseconds"),
        millis(entries[0].at),
        "the frame's own timestamp, as the trace stamped it"
    );
    assert!(
        header["at"].as_i64().expect("epoch milliseconds") >= millis(entries[1].at),
        "and the header's is when the export was taken"
    );
}

#[tokio::test]
async fn a_frame_the_inspector_could_not_read_exports_verbatim() {
    // The raw-first rule reaches the export (§8): what is exported is what
    // crossed, including the frames that made no sense, because those are the
    // ones the bug report is about.
    const NOT_JSON: &str = r#"Traceback (most recent call last): "unquoted \ mess"#;

    let (client, transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let (_outgoing, mut incoming, _diagnostics) = client.into_parts();

    transport
        .incoming
        .send(Frame::new(NOT_JSON))
        .await
        .expect("a live client");
    incoming.recv().await.expect("the frame");

    let records = records(&trace.export().to_string());
    assert_eq!(
        records[1]["frame"], NOT_JSON,
        "a frame is a JSON string, so anything that crossed can be one"
    );
    assert_eq!(records[1]["direction"], "agent -> client");
}

#[tokio::test]
async fn each_connection_the_trace_recorded_is_told_apart_in_the_export() {
    // The trace outlives any one agent, so an export can carry two of them —
    // and their JSON-RPC ids both start at 1. A consumer correlating calls to
    // answers has to be able to tell whose "id 1" it is holding.
    let trace = Trace::default();

    let (first, mut first_transport) = connection();
    let first = trace.tap(first);
    first
        .outgoing()
        .send(Frame::new(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        ))
        .await
        .expect("a live transport");
    first_transport.outgoing.recv().await.expect("the frame");

    // The next agent, on the same trace: what `Trace::tap` exists for.
    let (second, mut second_transport) = connection();
    let second = trace.tap(second);
    second
        .outgoing()
        .send(Frame::new(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        ))
        .await
        .expect("a live transport");
    second_transport.outgoing.recv().await.expect("the frame");

    let records = records(&trace.export().to_string());
    assert_eq!(records[1]["connection"], 1, "counted from one");
    assert_eq!(records[2]["connection"], 2);
}

#[tokio::test]
async fn clearing_keeps_the_frames_that_come_after_and_admits_to_the_ones_before() {
    let (client, mut transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let outgoing = client.outgoing().clone();

    outgoing
        .send(Frame::new(r#"{"id":1}"#))
        .await
        .expect("a live transport");
    transport.outgoing.recv().await.expect("the frame");

    // An export already taken is evidence in hand. Clearing the trace it came
    // from cannot reach it — which is the whole of what "export-anchored"
    // means here (§15 q3).
    let taken = trace.export().to_string();

    trace.clear();
    assert!(trace.is_empty(), "the trace is the frames from here on");
    assert_eq!(trace.dropped(), 1);

    outgoing
        .send(Frame::new(r#"{"id":2}"#))
        .await
        .expect("a live transport");
    transport.outgoing.recv().await.expect("the frame");

    let before = records(&taken);
    assert_eq!(before.len(), 2, "the taken export still has its frame");
    assert_eq!(before[1]["frame"], r#"{"id":1}"#);

    let after = records(&trace.export().to_string());
    assert_eq!(after.len(), 2, "and the next one has only what followed");
    assert_eq!(after[1]["frame"], r#"{"id":2}"#);
    assert_eq!(
        after[0]["dropped"], 1,
        "an export of what is left of a session says so"
    );
}

#[tokio::test]
async fn a_trace_past_its_capacity_keeps_the_newest_frames_and_counts_the_rest() {
    // The bounding policy (§15 q3): a ring, so the frames being looked for —
    // the last thing the agent did — are the ones that survive.
    let (client, mut transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let outgoing = client.outgoing().clone();

    let over = Trace::CAPACITY + 5;
    for id in 1..=over {
        outgoing
            .send(Frame::new(format!(r#"{{"id":{id}}}"#)))
            .await
            .expect("a live transport");
        transport.outgoing.recv().await.expect("the frame");
    }

    assert_eq!(trace.len(), Trace::CAPACITY);
    assert_eq!(trace.dropped(), 5);

    let records = records(&trace.export().to_string());
    assert_eq!(records[0]["frames"], Trace::CAPACITY);
    assert_eq!(records[0]["dropped"], 5);
    assert_eq!(
        records[1]["frame"], r#"{"id":6}"#,
        "the oldest that is left"
    );
    assert_eq!(
        records[Trace::CAPACITY]["frame"],
        format!(r#"{{"id":{over}}}"#),
        "and the newest there is"
    );
}

#[tokio::test]
async fn saving_writes_the_export_to_a_file_of_its_own() {
    let (client, mut transport) = connection();
    let (client, trace) = Trace::decorate(client);
    client
        .outgoing()
        .send(Frame::new(r#"{"id":1}"#))
        .await
        .expect("a live transport");
    transport.outgoing.recv().await.expect("the frame");

    let directory = scratch("saved");
    let export = trace.export();
    let path = export.save_in(&directory).expect("somewhere to write");

    assert_eq!(path.parent(), Some(directory.as_path()));
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(export.filename().as_str()),
        "the file is called what the export says it is called"
    );
    assert!(
        export.filename().starts_with("acp-trace-") && export.filename().ends_with(".jsonl"),
        "and what it says is a timestamped JSONL name: {}",
        export.filename()
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        export.to_string(),
        "the file is the export, byte for byte"
    );

    // Nothing is redacted, so the file is the user's alone until they hand it
    // to somebody — the same care the bridge's trace takes with the same risk.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("the file").permissions();
        assert_eq!(mode.mode() & 0o777, 0o600, "readable by its owner only");
    }

    // Two exports in the same millisecond are one impatient user, not a reason
    // to overwrite the evidence they just took.
    let again = trace.export().save_in(&directory).expect("a second file");
    assert_ne!(again, path);
    assert!(again.exists() && path.exists());
}

#[tokio::test]
async fn an_export_saved_without_a_directory_lands_somewhere_the_user_can_be_told_about() {
    let (client, _transport) = connection();
    let (_client, trace) = Trace::decorate(client);

    let path = trace.export().save().expect("somewhere to write");
    assert_eq!(
        path.parent(),
        Some(std::env::temp_dir().as_path()),
        "a place that is always writable and never the user's project"
    );
    assert!(std::fs::read_to_string(&path).is_ok());
    std::fs::remove_file(&path).expect("the file");
}

#[tokio::test]
async fn an_export_carries_what_the_inspector_could_not_read_and_what_it_answered() {
    // One capture, three consumers (§6.2): the timeline decorates what the
    // trace recorded, and the export writes out the same frames — so traffic
    // nobody could decode is in the file verbatim, which is the whole reason to
    // hand the file to somebody.
    const OVERREACH: &str = r#"{"jsonrpc":"2.0","id":41,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/etc/passwd"}}"#;

    let agent = AgentCommand {
        command: "/bin/sh".into(),
        args: format!("-c\nprintf '%s\\n' '{OVERREACH}'; cat > /dev/null"),
        ..AgentCommand::default()
    };

    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&agent.factory());

    until(
        &mut frames,
        "the call and the refusal to be recorded",
        || inspector.trace().len() == 2,
    )
    .await;

    let records = records(&inspector.trace().export().to_string());
    assert_eq!(records[1]["frame"], OVERREACH, "verbatim, down to the byte");
    assert_eq!(records[1]["direction"], "agent -> client");
    assert_eq!(records[2]["direction"], "client -> agent");
    assert!(
        records[2]["frame"]
            .as_str()
            .expect("the refusal")
            .contains("-32601"),
        "and what the inspector said back: {}",
        records[2]["frame"]
    );

    // The timeline read the same frame the export wrote.
    let entries = inspector.timeline().entries();
    assert!(
        matches!(entries[0].kind, EntryKind::Unrecognized(_)),
        "the typed layer decorates the capture; it is not a second one"
    );
    assert_eq!(entries[0].frames[0].frame.as_str(), OVERREACH);
}

#[tokio::test]
async fn clearing_and_exporting_a_connected_inspector_is_the_windows_own_path() {
    // What the two buttons do, reached the way the shell reaches it: through
    // the inspector's trace, on a live agent, with frames still arriving.
    let agent = AgentCommand {
        command: "/bin/sh".into(),
        args: "-c\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'; cat > /dev/null"
            .into(),
        ..AgentCommand::default()
    };

    let inspector = Inspector::new();
    let mut frames = inspector.trace().changes();
    inspector.connect(&agent.factory());
    until(&mut frames, "the agent's frame to be recorded", || {
        inspector.trace().len() == 1
    })
    .await;

    inspector.trace().clear();
    assert!(inspector.trace().is_empty());
    assert_eq!(inspector.trace().dropped(), 1);

    let directory = scratch("window");
    let path = inspector
        .trace()
        .export()
        .save_in(&directory)
        .expect("somewhere to write");
    let records = records(&std::fs::read_to_string(&path).expect("the file"));

    assert_eq!(records.len(), 1, "a header, and the frames it kept: none");
    assert_eq!(records[0]["frames"], 0);
    assert_eq!(
        records[0]["dropped"], 1,
        "an export taken after a clear says what the clear took"
    );

    // And the connection is untouched by either: clearing a trace is not a way
    // of stopping an agent.
    assert_eq!(inspector.status(), ConnectionStatus::Connected);
}

/// An empty directory of this test run's own.
fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("inspector-export-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// A timestamp the way the export writes it, from the trace's own stamp.
fn millis(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .expect("a clock past 1970")
        .as_millis()
        .try_into()
        .expect("a clock before the year 292 million")
}
