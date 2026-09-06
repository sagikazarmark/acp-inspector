//! The trace decorator against a transport that is not a process.
//!
//! Built on the public transport ends, which is the point twice over: it is how
//! the deferred WebSocket factory will be written (§6.1), and it shows the
//! decorator recording frames with no help at all from the transport under it.

use std::time::SystemTime;

use acp_inspector_core::{Direction, Frame, Trace, connection};

#[tokio::test]
async fn every_frame_is_recorded_in_both_directions() {
    let started = SystemTime::now();
    let (client, mut transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let (outgoing, mut incoming, _diagnostics) = client.into_parts();

    let sent = Frame::new(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    outgoing
        .send(sent.clone())
        .await
        .expect("the transport is alive");
    assert_eq!(transport.outgoing.recv().await, Some(sent.clone()));

    let answered = Frame::new(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
    transport
        .incoming
        .send(answered.clone())
        .await
        .expect("the client is alive");
    assert_eq!(
        incoming.recv().await.map(|received| received.frame),
        Some(answered.clone())
    );

    let entries = trace.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].direction, Direction::ToAgent);
    assert_eq!(entries[0].frame, sent);
    assert_eq!(entries[1].direction, Direction::FromAgent);
    assert_eq!(entries[1].frame, answered);
    assert!(entries[0].at >= started && entries[1].at >= entries[0].at);
}

#[tokio::test]
async fn a_frame_is_recorded_before_anything_above_the_seam_has_it() {
    let (client, transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let (outgoing, mut incoming, _diagnostics) = client.into_parts();

    // Outgoing: recorded before the transport is given it, so a frame that is
    // the last thing before a broken pipe is still in the record.
    outgoing.send(Frame::new(r#"{"id":1}"#)).await.unwrap();
    assert_eq!(
        trace.len(),
        1,
        "recorded on the way down, not on the way out"
    );

    // Incoming: a frame in flight is not yet a frame anyone has seen...
    transport
        .incoming
        .send(Frame::new(r#"{"id":2}"#))
        .await
        .unwrap();
    assert_eq!(trace.len(), 1);

    // ...and by the time the client has it, the trace does too. There is no
    // ordering in which something above the seam sees a frame first.
    let received = incoming.recv().await.unwrap();
    assert!(
        trace
            .entries()
            .iter()
            .any(|entry| entry.frame == received.frame)
    );

    // And the row it says it crossed as is the row the trace holds it at: the
    // number an entry keeps (`Recorded::captured`) and the one a renderer reads
    // off a snapshot (`dropped + position`) are the same number, or every jump
    // between the two screens lands one row out.
    let (held, dropped) = trace.snapshot();
    let captured = received.captured.expect("a tapped connection numbers it");
    assert_eq!(
        held[captured as usize - dropped].frame,
        received.frame,
        "the ordinal the stream reported is where the trace put it"
    );
}

#[tokio::test]
async fn an_undecorated_connection_records_nothing_and_still_works() {
    // The decorator is a decision, not a tax: the contract underneath it is a
    // whole connection on its own, which is what keeps `Trace` testable and the
    // seam honest about who owns capture.
    let (client, mut transport) = connection();
    let (outgoing, _incoming, _diagnostics) = client.into_parts();

    outgoing.send(Frame::new(r#"{"id":1}"#)).await.unwrap();
    assert!(transport.outgoing.recv().await.is_some());
}

#[tokio::test]
async fn dropping_the_client_end_tells_the_transport_to_stop() {
    let (client, transport) = connection();

    assert!(!transport.shutdown.is_cancelled());
    drop(client);

    // The one signal a transport that owns a process needs: nobody is listening
    // any more, so end the agent.
    assert!(transport.shutdown.is_cancelled());
}

#[tokio::test]
async fn the_last_client_part_alive_keeps_the_connection() {
    let (client, transport) = connection();
    let (outgoing, incoming, diagnostics) = client.into_parts();

    drop(incoming);
    drop(diagnostics);
    assert!(
        !transport.shutdown.is_cancelled(),
        "a client that parked the stream elsewhere and kept the sink is still connected"
    );

    drop(outgoing);
    assert!(transport.shutdown.is_cancelled());
}
