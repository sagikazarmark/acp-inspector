//! Frames a person sends by hand (F2): numbered from the client's own id
//! sequence, marked in the trace and the export, paired with their answers —
//! and kept out of every store the typed layer writes.

use std::sync::Mutex;

use acp_inspector_core::{
    AuthState, Connection, ConnectionFactory, EntryKind, Frame, Inspector, TransportEnds,
    connection,
};
use serde_json::value::RawValue;

/// Waits, briefly, for the reader task to have taken what the test sent.
async fn settled(mut ready: impl FnMut() -> bool) {
    for _ in 0..1000 {
        if ready() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("waited and it never happened");
}

/// A factory whose transport end the test holds, so it can play the agent.
#[derive(Default)]
struct Wired(Mutex<Option<TransportEnds>>);

impl ConnectionFactory for Wired {
    fn connect(&self) -> Connection {
        let (client, ends) = connection();
        *self.0.lock().unwrap() = Some(ends);
        client
    }
}

fn wired() -> (Inspector, TransportEnds) {
    let inspector = Inspector::new();
    let factory = Wired::default();
    inspector.connect(&factory);
    let ends = factory.0.lock().unwrap().take().expect("connected");
    (inspector, ends)
}

#[tokio::test]
async fn a_hand_sent_request_is_marked_paired_and_drives_no_store() {
    let (inspector, mut agent) = wired();
    let params: Box<RawValue> =
        serde_json::from_str("{ \"cwd\": \"/tmp\",\n  \"mcpServers\": [] }").unwrap();
    let sent = inspector
        .send_raw("session/new", Some(&params), false)
        .await
        .expect("a live connection");
    assert_eq!(sent.id, Some(1), "the client's own id sequence");
    let request = agent.outgoing.recv().await.expect("the frame");
    assert_eq!(
        request.as_str(),
        r#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/tmp","mcpServers":[]}}"#,
        "params as written, on one line"
    );
    assert!(request.is_hand_sent());

    let pending = inspector.pending_calls();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].hand_sent);
    assert_eq!(pending[0].captured, sent.captured);

    // An answer that would open a session — and one that asks for a login —
    // change nothing a screen reads, because a person sent the question.
    agent
        .incoming
        .send(Frame::new(
            r#"{"jsonrpc":"2.0","id":1,"result":{"sessionId":"by-hand"}}"#,
        ))
        .await
        .unwrap();
    settled(|| inspector.pending_calls().is_empty()).await;
    assert_eq!(inspector.session(), None);

    let again = inspector
        .send_raw("session/prompt", None, false)
        .await
        .unwrap();
    agent.outgoing.recv().await.unwrap();
    agent
        .incoming
        .send(Frame::new(
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required"}}"#,
        ))
        .await
        .unwrap();
    settled(|| inspector.pending_calls().is_empty()).await;
    assert_eq!(inspector.auth(), AuthState::Unasked);
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .all(|entry| !matches!(entry.kind, EntryKind::Unrecognized(_))),
        "an answer to a hand-sent call is not unsolicited"
    );

    // Paired in the trace, both ways.
    let request = sent.captured.expect("traced");
    let answer = inspector.trace().counterpart(request).expect("answered");
    assert_eq!(
        inspector
            .trace()
            .counterpart(answer.ordinal)
            .unwrap()
            .ordinal,
        request
    );
    assert!(
        inspector
            .trace()
            .counterpart(again.captured.unwrap())
            .is_some()
    );

    // And marked in the export.
    let export = inspector.trace().export().to_string();
    let hand = export
        .lines()
        .filter(|line| line.contains(r#""origin":"hand""#))
        .count();
    assert_eq!(hand, 2, "{export}");
}

#[tokio::test]
async fn a_hand_sent_notification_has_no_id_and_waits_for_nothing() {
    let (inspector, mut agent) = wired();
    let sent = inspector.send_raw("x/ping", None, true).await.unwrap();
    assert_eq!(sent.id, None);
    let frame = agent.outgoing.recv().await.unwrap();
    assert_eq!(frame.as_str(), r#"{"jsonrpc":"2.0","method":"x/ping"}"#);
    assert!(inspector.pending_calls().is_empty());
    let traced = inspector.trace().entries();
    assert!(traced[sent.captured.unwrap() as usize].frame.is_hand_sent());
}

#[tokio::test]
async fn nothing_is_sent_by_hand_without_an_agent() {
    let inspector = Inspector::new();
    assert!(inspector.send_raw("x", None, false).await.is_err());
}
