//! Repeatable debug/test-profile probe; run explicitly with --ignored --nocapture.
use acp_inspector_core::{ConnectionStatus, Frame, Inspector, StdioSpawn};
use std::time::{Duration, Instant};

#[test]
#[ignore]
fn envelope_probe() {
    let frame = Frame::new(format!(
        r#"{{"id":1,"method":"session/update","params":{{"text":"{}"}}}}"#,
        "x".repeat(9 * 1024 * 1024)
    ));
    let start = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(frame.summary());
    }
    eprintln!("100 summaries of 9 MiB Frame: {:?}", start.elapsed());
}

#[tokio::test]
#[ignore]
async fn real_traffic_probe() {
    let inspector = Inspector::new();
    let mut changes = inspector.status_changes();
    let start = Instant::now();
    inspector.connect(&StdioSpawn::new("python3").args(["-c", r#"
import json, sys
for i in range(15000):
    print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'stress','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'x'*256}}}}))
    print(str(i)+':'+('e'*1024), file=sys.stderr)
for i in range(8):
    print(json.dumps({'jsonrpc':'2.0','method':'stress/large','params':{'text':'x'*(9*1024*1024)}}))
"#]));
    tokio::time::timeout(Duration::from_secs(120), async {
        while inspector.status() != ConnectionStatus::Lost {
            changes.next().await.unwrap();
        }
    })
    .await
    .unwrap();
    let ingest = start.elapsed();
    let start = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(inspector.trace().snapshot());
        std::hint::black_box(inspector.timeline().entries());
        std::hint::black_box(inspector.diagnostics().snapshot());
    }
    eprintln!(
        "traffic ingest={ingest:?}; 100 store snapshots={:?}; trace={} dropped={} timeline={} diagnostics={} dropped={}",
        start.elapsed(),
        inspector.trace().len(),
        inspector.trace().dropped(),
        inspector.timeline().len(),
        inspector.diagnostics().len(),
        inspector.diagnostics().snapshot().1
    );
    assert_eq!(inspector.trace().len() + inspector.trace().dropped(), 15008);
    assert_eq!(
        inspector.diagnostics().len() + inspector.diagnostics().snapshot().1,
        15001
    );
}
