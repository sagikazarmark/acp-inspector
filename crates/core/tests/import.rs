//! Reading an export back in (F5): the same frames — lossy, dropped and
//! hand-sent ones included — numbered and paired as they were, the stderr it
//! carried, and a timeline replayed from them.

use std::time::{Duration, UNIX_EPOCH};

use acp_inspector_core::{
    Diagnostic, DiagnosticKind, Direction, EntryKind, Frame, ImportError, Imported, Omission,
    Trace, TurnOutcome, Unrecognized, connection, v1,
};

/// A trace with one of everything the format carries.
async fn captured() -> Trace {
    let (client, mut transport) = connection();
    let (client, trace) = Trace::decorate(client);
    let (outgoing, mut incoming, _diagnostics) = client.into_parts();

    let send = |text: &'static str| Frame::new(text);
    for frame in [
        send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}"#),
        Frame::by_hand(r#"{"jsonrpc":"2.0","id":2,"method":"x/poke"}"#),
        send(
            r#"{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"s","prompt":[]}}"#,
        ),
    ] {
        outgoing.send(frame).await.unwrap();
        transport.outgoing.recv().await.unwrap();
    }
    let mut lossy = br#"{"jsonrpc":"2.0","method":"x/bytes","params":""#.to_vec();
    lossy.extend_from_slice(&[0xff, 0xfe]);
    lossy.extend_from_slice(br#""}"#);
    for frame in [
        Frame::new(
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"replayed","version":"1"}}}"#,
        ),
        Frame::read(lossy),
        Frame::omitted(
            b"{\"jsonrpc\":",
            Omission {
                length: 20_000_000,
                limit: 10_485_760,
            },
        ),
        Frame::new(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}"#,
        ),
        Frame::new(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#),
        Frame::new(r#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#),
    ] {
        transport.incoming.send(frame).await.unwrap();
        incoming.recv().await.unwrap();
    }
    trace
}

#[tokio::test]
async fn an_export_reads_back_as_the_same_frames_paired_the_same_way() {
    let trace = captured().await;
    let stderr = vec![
        Diagnostic {
            at: UNIX_EPOCH + Duration::from_millis(5),
            kind: DiagnosticKind::Stderr("starting".to_owned()),
        },
        Diagnostic {
            at: UNIX_EPOCH + Duration::from_millis(6),
            kind: DiagnosticKind::AgentExited(std::process::ExitStatus::default()),
        },
    ];
    let text = trace.export().with_stderr(&stderr).to_string();
    assert!(
        text.contains(r#""type":"stderr""#) && text.contains(r#""stderr":1"#),
        "{text}"
    );

    let imported = Imported::parse(&text).expect("an export reads back");
    assert!(imported.warnings().is_empty(), "{:?}", imported.warnings());
    let (before, dropped_before) = trace.snapshot();
    let (after, dropped_after) = imported.trace().snapshot();
    assert_eq!(dropped_before, dropped_after);
    assert_eq!(before.len(), after.len());
    for (was, is) in before.iter().zip(&after) {
        // A placeholder's retained prefix is not in the file: only its length
        // and the limit it was over.
        if was.frame.omission().is_none() {
            assert_eq!(was.frame.bytes(), is.frame.bytes());
        }
        assert_eq!(was.frame.is_lossy(), is.frame.is_lossy());
        assert_eq!(was.frame.omission(), is.frame.omission());
        assert_eq!(was.frame.is_hand_sent(), is.frame.is_hand_sent());
        assert_eq!(was.direction, is.direction);
        assert_eq!(was.connection, is.connection);
        let millis = |at: std::time::SystemTime| at.duration_since(UNIX_EPOCH).unwrap().as_millis();
        assert_eq!(millis(was.at), millis(is.at));
    }
    assert!(after.iter().any(|traced| traced.frame.is_lossy()));
    assert!(after.iter().any(|traced| traced.frame.omission().is_some()));
    assert!(after.iter().any(|traced| traced.frame.is_hand_sent()));
    for ordinal in 0..before.len() as u64 {
        assert_eq!(
            trace.counterpart(ordinal).map(|pair| pair.ordinal),
            imported
                .trace()
                .counterpart(ordinal)
                .map(|pair| pair.ordinal),
            "frame {ordinal} is paired as it was"
        );
    }
    assert_eq!(after[0].direction, Direction::ToAgent);

    // Only the agent's own stderr travels.
    assert_eq!(imported.stderr().len(), 1);

    // And it reads back out the same again.
    let again = imported.trace().export().to_string();
    let body = |text: &str| {
        text.lines()
            .skip(1)
            .filter(|line| !line.contains(r#""type":"stderr""#))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(body(&text), body(&again));
}

#[tokio::test]
async fn the_timeline_is_replayed_from_the_frames() {
    let trace = captured().await;
    let imported = Imported::parse(&trace.export().to_string()).unwrap();
    assert_eq!(
        imported
            .agent()
            .and_then(|agent| agent.agent_info.as_ref())
            .map(|info| info.name.as_str()),
        Some("replayed")
    );
    let (entries, turns, _) = imported.timeline().snapshot();
    assert_eq!(turns.len(), 1);
    assert_eq!(
        turns[0].outcome,
        Some(TurnOutcome::Ended(v1::StopReason::EndTurn))
    );
    let update = entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::Update(_)))
        .expect("the message, decoded");
    assert_eq!(update.turn, Some(turns[0].id));
    assert_eq!(update.frames[0].captured, Some(6), "the row it crossed as");
    assert!(
        entries.iter().any(|entry| matches!(
            entry.kind,
            EntryKind::Unrecognized(Unrecognized::Dropped { .. })
        )),
        "the dropped frame's placeholder is replayed too"
    );
    // The answers to the inspector's own calls are not unsolicited.
    assert!(
        !entries.iter().any(|entry| matches!(
            entry.kind,
            EntryKind::Unrecognized(Unrecognized::Unsolicited)
        )),
        "{entries:?}"
    );
}

#[tokio::test]
async fn saving_as_writes_exactly_where_asked_and_opens_again() {
    let trace = captured().await;
    let directory = std::env::temp_dir().join(format!(
        "acp-inspector-save-as-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("chosen name.jsonl");
    std::fs::write(&path, "replaced").unwrap();
    assert_eq!(trace.export().save_to(&path).unwrap(), path);
    let imported = Imported::open(&path).expect("what was saved opens");
    assert_eq!(imported.trace().len(), trace.len());
    assert_eq!(
        std::fs::read_dir(&directory).unwrap().count(),
        1,
        "nothing is left beside it"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn what_is_not_an_export_is_refused_with_the_reason() {
    assert!(matches!(
        Imported::parse("{\"hello\":1}\n"),
        Err(ImportError::NotATrace(_))
    ));
    let cut = "{\"type\":\"header\",\"format\":\"acp-inspector-trace\",\"version\":1,\"at\":0,\"frames\":2,\"dropped\":0}\n{\"type\":\"frame\",\"at\":1,\"connection\":1,\"direction\":\"agent -> client\",\"encoding\":\"utf8\",\"frame\":\"{}\"}\n{\"type\":\"future\"}\n";
    let imported = Imported::parse(cut).expect("readable, with warnings");
    assert_eq!(imported.trace().len(), 1);
    assert_eq!(imported.skipped(), 1);
    assert_eq!(imported.warnings().len(), 2, "{:?}", imported.warnings());
}
