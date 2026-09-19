//! Retention is observable evidence loss, never a changed Frame or lost resolver.
use acp_inspector_core::{EntryKind, Frame, Inspector, StdioSpawn, Timeline, Trace, connection};

#[tokio::test]
async fn noisy_diagnostics_count_every_missing_line_and_keep_exit_last() {
    let inspector = Inspector::new();
    let mut status = inspector.status_changes();
    inspector.connect(&StdioSpawn::new("python3").args([
        "-c",
        "import sys\nfor i in range(11000): print(str(i)+':'+'x'*1024, file=sys.stderr)\n",
    ]));
    tokio::time::timeout(Duration::from_secs(20), async {
        while inspector.status() != acp_inspector_core::ConnectionStatus::Lost {
            status.next().await.unwrap();
        }
    })
    .await
    .unwrap();
    let (held, dropped) = inspector.diagnostics().snapshot();
    assert_eq!(held.len() + dropped, 11001);
    assert!(
        held.len() < acp_inspector_core::DiagnosticLog::CAPACITY,
        "byte limit triggers before entry limit"
    );
    assert!(dropped > 1001);
    assert!(matches!(
        held.last().unwrap().kind,
        acp_inspector_core::DiagnosticKind::AgentExited(_)
    ));
    for (position, line) in held[..held.len() - 1].iter().enumerate() {
        assert!(
            matches!(&line.kind, acp_inspector_core::DiagnosticKind::Stderr(text) if text == &format!("{}:{}", dropped + position, "x".repeat(1024)))
        );
    }
}
use std::time::Duration;

#[tokio::test]
async fn byte_budget_rotates_whole_frames_and_snapshot_export_keeps_exact_bytes() {
    let (client, transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let (_, mut incoming, _) = client.into_parts();
    let raw = format!(
        r#" {{"id":18446744073709551615,"result":"{}🦀"}} "#,
        "x".repeat(9 * 1024 * 1024)
    );
    let frame = Frame::new(raw.clone());
    for _ in 0..8 {
        transport.incoming.send(frame.clone()).await.unwrap();
        incoming.recv().await.unwrap();
    }
    let (held, dropped) = trace.snapshot();
    assert_eq!((held.len(), dropped), (7, 1));
    assert!(held.iter().map(|e| e.frame.as_str().len()).sum::<usize>() <= Trace::BYTE_CAPACITY);
    assert_eq!(held[0].frame.as_str(), raw);
    let export = trace.export();
    trace.clear();
    transport.incoming.send(Frame::new("later")).await.unwrap();
    incoming.recv().await.unwrap();
    // The worker writes an activation snapshot, not whatever Trace holds later.
    let written = tokio::task::spawn_blocking(move || export.to_string())
        .await
        .unwrap();
    let mut lines = written.lines();
    let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(header["dropped"], 1);
    assert_eq!(header["frames"], 7);
    for line in lines {
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["frame"].as_str().unwrap(), raw);
    }
    assert_eq!(trace.snapshot().1, 8);
}

#[tokio::test]
async fn long_stream_keeps_pending_permission_and_elicitation_answerable_with_hole_counts() {
    let inspector = Inspector::new();
    let mut changes = inspector.timeline().changes();
    inspector.connect(&StdioSpawn::new("python3").args(["-c", r#"
import json, sys
def emit(value): print(json.dumps(value), flush=True)
emit({'jsonrpc':'2.0','id':'p','method':'session/request_permission','params':{'sessionId':'s','toolCall':{'toolCallId':'t','title':'Keep me'},'options':[{'optionId':'yes','name':'Yes','kind':'allow_once'}]}})
emit({'jsonrpc':'2.0','id':'e','method':'elicitation/create','params':{'sessionId':'s','mode':'form','message':'Keep me too','requestedSchema':{'type':'object','properties':{}}}})
for i in range(2500):
    emit({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':str(i)}}}})
for i in range(2):
    reply=json.loads(sys.stdin.readline())
    emit({'jsonrpc':'2.0','method':'answered/'+str(reply['id'])})
sys.stdin.read()
"#]));
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let (entries, _, holes) = inspector.timeline().snapshot();
            if entries.len() + holes.entries >= 2502 {
                break;
            }
            changes.next().await.unwrap();
        }
    })
    .await
    .unwrap();
    let (entries, _, holes) = inspector.timeline().snapshot();
    assert_eq!(entries.len(), Timeline::CAPACITY);
    assert_eq!(holes.entries, 1502);
    assert_eq!(holes.frames, 1502);
    assert!(holes.bytes > 0);
    assert_eq!(entries.iter().filter(|e| e.is_waiting()).count(), 2);
    assert_eq!(entries[0].frames[0].captured, Some(0));
    assert_eq!(entries[1].frames[0].captured, Some(1));
    for entry in entries.iter().take(2) {
        match &entry.kind {
            EntryKind::Permission(request) => {
                request.select(&"yes".into()).await.unwrap();
            }
            EntryKind::Elicitation(request) => {
                request.cancel().await.unwrap();
            }
            _ => panic!("pending request must retain its evidence and resolver"),
        }
    }
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .all(|e| !e.is_waiting())
    );
    inspector.disconnect();
}
