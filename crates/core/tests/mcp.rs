mod common;
use acp_inspector_core::{CallError, Inspector, McpDraft, McpEnv, Restore, StdioMcp, v1};
use common::{sent, shell};

fn draft() -> McpDraft {
    McpDraft {
        servers: vec![StdioMcp {
            name: "tools".into(),
            command: "/not-installed/mcp".into(),
            args: vec!["".into(), "two words".into()],
            env: vec![
                McpEnv {
                    name: "TOKEN".into(),
                    value: "".into(),
                },
                McpEnv {
                    name: "TOKEN".into(),
                    value: " a=b ".into(),
                },
            ],
        }],
    }
}

fn agent() -> acp_inspector_core::AgentCommand {
    shell(concat!(
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true,\"sessionCapabilities\":{\"resume\":{}}}}}'; ",
        "i=2; while read line; do printf '{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"sessionId\":\"s\"}}\\n' \"$i\"; i=$((i+1)); done"
    ))
}

#[tokio::test]
async fn every_opening_and_reconnect_carries_the_shared_list_without_launching_mcp_locally() {
    let inspector = Inspector::new();
    let command = agent();
    inspector.set_mcp_draft(draft());
    inspector.start(&command).await.unwrap();
    inspector.open_session(&command, None).await.unwrap();
    let session = v1::SessionInfo::new("saved", "/tmp");
    inspector
        .restore_session(&session, Restore::Load, None)
        .await
        .unwrap();
    inspector
        .restore_session(&session, Restore::Resume, None)
        .await
        .unwrap();
    let expected = serde_json::json!([{"name":"tools","command":"/not-installed/mcp","args":["","two words"],"env":[{"name":"TOKEN","value":""},{"name":"TOKEN","value":" a=b "}]}]);
    let frames: Vec<serde_json::Value> = sent(&inspector)
        .iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .filter(|frame: &serde_json::Value| {
            matches!(
                frame["method"].as_str(),
                Some("session/new" | "session/load" | "session/resume")
            )
        })
        .collect();
    assert_eq!(frames.len(), 4);
    for frame in frames {
        assert_eq!(frame["params"]["mcpServers"], expected);
    }
    inspector.disconnect();
    assert_eq!(inspector.mcp_draft(), draft());
    inspector.start(&command).await.unwrap();
    let last: serde_json::Value = serde_json::from_str(sent(&inspector).last().unwrap()).unwrap();
    assert_eq!(last["params"]["mcpServers"], expected);
    inspector.set_mcp_draft(McpDraft::default());
    inspector.open_session(&command, None).await.unwrap();
    let last: serde_json::Value = serde_json::from_str(sent(&inspector).last().unwrap()).unwrap();
    assert_eq!(last["params"]["mcpServers"], serde_json::json!([]));
    inspector.disconnect();
}

#[tokio::test]
async fn invalid_edits_do_not_switch_sessions_send_frames_or_replace_the_connection() {
    let inspector = Inspector::new();
    let command = agent();
    inspector.start(&command).await.unwrap();
    let session = inspector.session();
    let count = sent(&inspector).len();
    inspector.set_mcp_draft(McpDraft {
        servers: vec![StdioMcp::default()],
    });
    assert!(matches!(
        inspector.open_session(&command, None).await,
        Err(CallError::InvalidMcp(_))
    ));
    assert!(matches!(
        inspector
            .restore_session(&v1::SessionInfo::new("saved", "/tmp"), Restore::Load, None)
            .await,
        Err(CallError::InvalidMcp(_))
    ));
    assert!(matches!(
        inspector.start(&command).await,
        Err(CallError::InvalidMcp(_))
    ));
    assert_eq!(inspector.session(), session);
    assert_eq!(sent(&inspector).len(), count);
    inspector.disconnect();
}

#[tokio::test]
async fn launch_snapshots_definitions_before_initialize_answers() {
    let inspector = Inspector::new();
    inspector.set_mcp_draft(draft());
    let command = shell(concat!(
        "read _; sleep 0.1; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"s\"}}'; cat >/dev/null"
    ));
    let starting = inspector.clone();
    let pending = tokio::spawn(async move { starting.start(&command).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while sent(&inspector).is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    inspector.set_mcp_draft(McpDraft::default());
    pending.await.unwrap().unwrap();
    let frame: serde_json::Value = serde_json::from_str(sent(&inspector).last().unwrap()).unwrap();
    assert_eq!(frame["params"]["mcpServers"][0]["name"], "tools");
    inspector.disconnect();
}
