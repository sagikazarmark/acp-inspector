//! Client services, answered by hand or by a rules file (F3,
//! `docs/client-services.md`), the permission auto-policy (F11), the
//! protocol-version override (F13), and what schema 1.11 made stable (C4).
//!
//! The agents are `/bin/sh` one-liners: what is asserted is the frames this
//! client sent back, read off the trace, and the stores the window renders.

mod common;

use acp_inspector_core::{
    AnsweredBy, ClientServices, EntryKind, Inspector, PermissionPolicy, PermissionState,
    ServiceReply, ServiceRequest, ServiceScript, ServiceState, Unrecognized, v1,
};
use serde_json::{Value, json};

use common::{prompting, sent, shell, until};

const INITIALIZED: &str = r#"'{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'"#;
const OPENED: &str = r#"'{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}'"#;

/// An agent that answers `initialize`, then says `frames` and listens.
fn after_initialize(frames: &[&str]) -> acp_inspector_core::AgentCommand {
    let said: String = frames.iter().map(|frame| format!("'{frame}' ")).collect();
    shell(&format!(
        "read _; printf '%s\\n' {INITIALIZED}; printf '%s\\n' {said}; cat > /dev/null"
    ))
}

/// An agent that opens `s1`, and says `frames` once the prompt arrives.
fn in_a_turn(frames: &[&str]) -> acp_inspector_core::AgentCommand {
    let said: String = frames.iter().map(|frame| format!("'{frame}' ")).collect();
    shell(&format!(
        "read _; printf '%s\\n' {INITIALIZED}; read _; printf '%s\\n' {OPENED}; read _; printf '%s\\n' {said}; cat > /dev/null"
    ))
}

fn connected(services: ClientServices, agent: acp_inspector_core::AgentCommand) -> Inspector {
    let inspector = Inspector::new();
    inspector.set_client_services(services);
    inspector.connect(&agent.factory());
    inspector
}

/// The frame this client sent answering `id`, parsed.
async fn answered(inspector: &Inspector, id: u64) -> Value {
    let needle = format!(r#""id":{id}"#);
    let mut frames = inspector.trace().changes();
    until(&mut frames, "the answer to go out", || {
        sent(inspector).iter().any(|frame| frame.contains(&needle))
    })
    .await;
    let frame = sent(inspector)
        .into_iter()
        .find(|frame| frame.contains(&needle))
        .unwrap();
    serde_json::from_str(&frame).unwrap()
}

async fn waiting_service(inspector: &Inspector) -> ServiceRequest {
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "a service request to wait", || {
        !inspector.pending_services().is_empty()
    })
    .await;
    inspector.pending_services().pop().unwrap()
}

fn initialize_params(inspector: &Inspector) -> Value {
    let frame = sent(inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""method":"initialize""#))
        .expect("initialize went out");
    serde_json::from_str::<Value>(&frame).unwrap()["params"].clone()
}

const READ: &str = r#"{"jsonrpc":"2.0","id":41,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/work/src/a.rs"}}"#;

#[tokio::test]
async fn the_launch_claims_the_services_it_chose_and_the_version_it_asked_for() {
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            write_text_file: true,
            terminal: true,
            protocol_version: 2,
            ..ClientServices::default()
        },
        after_initialize(&[]),
    );
    inspector.initialize().await.expect("it answers");

    let params = initialize_params(&inspector);
    assert_eq!(params["protocolVersion"], 2, "the override is on the wire");
    assert_eq!(
        params["clientCapabilities"]["fs"],
        json!({"readTextFile": true, "writeTextFile": true})
    );
    assert_eq!(params["clientCapabilities"]["terminal"], true);

    let claim = inspector.client_claim().expect("the claim is kept");
    assert_eq!(claim.protocol_version.as_u16(), 2);
    assert!(claim.capabilities.terminal);
    assert_eq!(
        inspector.agent().unwrap().protocol_version.as_u16(),
        1,
        "and the agent's answer is its own, for the display to compare"
    );
}

#[tokio::test]
async fn the_default_launch_claims_what_it_always_did_at_version_one() {
    let inspector = connected(ClientServices::default(), after_initialize(&[]));
    inspector.initialize().await.expect("it answers");

    let params = initialize_params(&inspector);
    assert_eq!(params["protocolVersion"], 1);
    assert_eq!(
        params["clientCapabilities"]["fs"],
        json!({"readTextFile": false, "writeTextFile": false})
    );
    assert_eq!(params["clientCapabilities"]["terminal"], false);
}

#[tokio::test]
async fn a_claimed_service_waits_for_the_readers_answer() {
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            ..ClientServices::default()
        },
        after_initialize(&[READ]),
    );
    inspector.initialize().await.unwrap();

    let request = waiting_service(&inspector).await;
    assert_eq!(request.method(), "fs/read_text_file");
    assert!(request.claimed());
    assert_eq!(request.params()["path"], "/work/src/a.rs");
    assert_eq!(request.skeleton(), json!({"content": ""}));
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(&entry.kind, EntryKind::Service(held) if held == &request)),
        "it is a timeline entry"
    );

    request
        .respond(json!({"content": "fn main() {}"}))
        .await
        .expect("the answer goes out");
    let answer = answered(&inspector, 41).await;
    assert_eq!(answer["result"]["content"], "fn main() {}");
    assert!(matches!(
        request.state(),
        ServiceState::Answered(answer) if answer.by == AnsweredBy::Reader
    ));
    assert!(inspector.pending_services().is_empty());
    assert!(
        request.respond(json!({})).await.is_err(),
        "and it is answered once"
    );
}

#[tokio::test]
async fn the_reader_can_answer_with_an_error() {
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            ..ClientServices::default()
        },
        after_initialize(&[READ]),
    );
    inspector.initialize().await.unwrap();
    let request = waiting_service(&inspector).await;

    request
        .refuse(v1::Error::new(-32002, "Resource not found"))
        .await
        .unwrap();
    let answer = answered(&inspector, 41).await;
    assert_eq!(answer["error"]["code"], -32002);
    assert_eq!(answer["error"]["message"], "Resource not found");
}

#[tokio::test]
async fn an_unclaimed_service_is_still_method_not_found() {
    let inspector = connected(ClientServices::default(), after_initialize(&[READ]));
    inspector.initialize().await.unwrap();

    let answer = answered(&inspector, 41).await;
    assert_eq!(answer["error"]["code"], -32601);
    assert!(inspector.pending_services().is_empty());
}

#[tokio::test]
async fn an_unserviced_request_is_answered_by_hand_when_the_reader_opts_in() {
    const PING: &str = r#"{"jsonrpc":"2.0","id":7,"method":"_vendor/ping","params":{"n":1}}"#;
    let inspector = connected(
        ClientServices {
            answer_unserviced: true,
            ..ClientServices::default()
        },
        after_initialize(&[PING]),
    );
    inspector.initialize().await.unwrap();

    let request = waiting_service(&inspector).await;
    assert_eq!(request.method(), "_vendor/ping");
    assert!(!request.claimed(), "nothing was claimed for it");
    assert_eq!(request.skeleton(), json!({}));
    request.respond(json!({"pong": true})).await.unwrap();
    assert_eq!(answered(&inspector, 7).await["result"]["pong"], true);
}

#[tokio::test]
async fn a_rules_file_answers_first_and_says_which_rule() {
    const OTHER: &str = r#"{"jsonrpc":"2.0","id":42,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/etc/passwd"}}"#;
    let script = ServiceScript::parse(
        "rules.json",
        r#"{"unmatched":"method-not-found","rules":[
            {"method":"fs/write_text_file","result":{}},
            {"method":"fs/read_text_file","path":"/work/*.rs","result":{"content":"scripted"}}
        ]}"#,
    )
    .unwrap();
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            script: Some(script),
            ..ClientServices::default()
        },
        after_initialize(&[READ, OTHER]),
    );
    inspector.initialize().await.unwrap();

    assert_eq!(
        answered(&inspector, 41).await["result"]["content"],
        "scripted",
        "the glob matched a nested path"
    );
    assert_eq!(
        answered(&inspector, 42).await["error"]["code"],
        -32601,
        "unmatched is method-not-found, as the file says, though the service was claimed"
    );

    let entries = inspector.timeline().entries();
    let scripted = entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Service(request) => Some(request.state()),
            _ => None,
        })
        .expect("the scripted answer is an entry");
    let ServiceState::Answered(answer) = scripted else {
        panic!("answered: {scripted:?}");
    };
    assert_eq!(answer.by, AnsweredBy::Script { rule: 1 });
    assert_eq!(
        answer.reply,
        ServiceReply::Result(json!({"content": "scripted"}))
    );
    assert!(entries.iter().any(|entry| matches!(
        &entry.kind,
        EntryKind::Unrecognized(Unrecognized::NotServiced { method }) if method == "fs/read_text_file"
    )));
}

#[tokio::test]
async fn an_unmatched_request_falls_back_to_the_reader_by_default() {
    const TERMINAL: &str = r#"{"jsonrpc":"2.0","id":43,"method":"terminal/create","params":{"sessionId":"s1","command":"cargo","args":["build"]}}"#;
    let script = ServiceScript::parse(
        "rules.json",
        r#"{"rules":[{"method":"terminal/create","command":"cargo test","result":{"terminalId":"t"}}]}"#,
    )
    .unwrap();
    let inspector = connected(
        ClientServices {
            terminal: true,
            script: Some(script),
            ..ClientServices::default()
        },
        after_initialize(&[TERMINAL]),
    );
    inspector.initialize().await.unwrap();

    let request = waiting_service(&inspector).await;
    assert_eq!(request.method(), "terminal/create");
    assert_eq!(request.skeleton(), json!({"terminalId": "term-1"}));
}

#[tokio::test]
async fn cancelling_the_turn_answers_a_waiting_service_with_the_cancellation_error() {
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            ..ClientServices::default()
        },
        in_a_turn(&[READ]),
    );
    inspector.initialize().await.unwrap();
    inspector.new_session("/tmp", None).await.unwrap();
    let _turn = prompting(&inspector, "go");

    let request = waiting_service(&inspector).await;
    inspector.cancel().await.unwrap();

    assert_eq!(answered(&inspector, 41).await["error"]["code"], -32800);
    assert_eq!(request.state(), ServiceState::Cancelled);
    assert!(inspector.pending_services().is_empty());
}

#[tokio::test]
async fn the_agent_can_withdraw_a_service_request() {
    const WITHDRAW: &str =
        r#"{"jsonrpc":"2.0","method":"$/cancel_request","params":{"requestId":41}}"#;
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            ..ClientServices::default()
        },
        after_initialize(&[READ, WITHDRAW]),
    );
    inspector.initialize().await.unwrap();

    assert_eq!(answered(&inspector, 41).await["error"]["code"], -32800);
    let entries = inspector.timeline().entries();
    let request = entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Service(request) => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.state(), ServiceState::CancelledByAgent);
}

#[tokio::test]
async fn a_waiting_service_request_blocks_until_the_agent_goes_away() {
    let inspector = connected(
        ClientServices {
            read_text_file: true,
            ..ClientServices::default()
        },
        after_initialize(&[READ]),
    );
    inspector.initialize().await.unwrap();
    waiting_service(&inspector).await;
    assert!(
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| entry.is_waiting()),
        "the entry is waiting on the reader"
    );
    inspector.disconnect();
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the request to be given up on", || {
        inspector.pending_services().is_empty()
    })
    .await;
}

fn asking(options: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":"p1","method":"session/request_permission","params":{{"sessionId":"s1","toolCall":{{"toolCallId":"c1","title":"Write"}},"options":{options}}}}}"#
    )
}

const ALWAYS_FIRST: &str = r#"[{"optionId":"always","name":"Always","kind":"allow_always"},{"optionId":"once","name":"Once","kind":"allow_once"},{"optionId":"no","name":"No","kind":"reject_once"},{"optionId":"never","name":"Never","kind":"reject_always"}]"#;

#[tokio::test]
async fn allow_once_by_policy_never_picks_an_always_option() {
    let asked = asking(ALWAYS_FIRST);
    let inspector = connected(ClientServices::default(), in_a_turn(&[&asked]));
    inspector.initialize().await.unwrap();
    inspector.new_session("/tmp", None).await.unwrap();
    inspector.set_permission_policy(PermissionPolicy::AllowOnce);
    let _turn = prompting(&inspector, "go");

    let mut frames = inspector.trace().changes();
    until(&mut frames, "the policy's answer", || {
        sent(&inspector)
            .iter()
            .any(|frame| frame.contains(r#""id":"p1""#))
    })
    .await;
    let answer = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains(r#""id":"p1""#))
        .unwrap();
    let answer: Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(answer["result"]["outcome"]["optionId"], "once");

    let request = inspector
        .timeline()
        .entries()
        .into_iter()
        .find_map(|entry| match entry.kind {
            EntryKind::Permission(request) => Some(request),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.by_policy(), Some(PermissionPolicy::AllowOnce));
    assert!(matches!(request.state(), PermissionState::Answered(_)));
    assert!(inspector.pending_permissions().is_empty());
}

#[test]
fn reject_once_by_policy_picks_reject_once_and_ask_picks_nothing() {
    let options = serde_json::from_str::<Vec<v1::PermissionOption>>(ALWAYS_FIRST).unwrap();
    assert_eq!(
        PermissionPolicy::RejectOnce
            .pick(&options)
            .map(|option| option.option_id.to_string()),
        Some("no".to_owned())
    );
    assert!(PermissionPolicy::Ask.pick(&options).is_none());
}

#[tokio::test]
async fn a_policy_with_only_always_options_leaves_the_request_to_the_reader() {
    let asked = asking(
        r#"[{"optionId":"always","name":"Always","kind":"allow_always"},{"optionId":"never","name":"Never","kind":"reject_always"}]"#,
    );
    let inspector = connected(ClientServices::default(), in_a_turn(&[&asked]));
    inspector.initialize().await.unwrap();
    inspector.new_session("/tmp", None).await.unwrap();
    inspector.set_permission_policy(PermissionPolicy::AllowOnce);
    let _turn = prompting(&inspector, "go");

    let request = common::blocked(&inspector).await;
    assert_eq!(request.by_policy(), None);
    assert_eq!(request.state(), PermissionState::Waiting);
    for policy in [PermissionPolicy::AllowOnce, PermissionPolicy::RejectOnce] {
        assert!(policy.pick(request.options()).is_none());
    }
}

#[tokio::test]
async fn the_policy_resets_when_the_agent_disconnects() {
    let inspector = connected(ClientServices::default(), after_initialize(&[]));
    inspector.initialize().await.unwrap();
    inspector.set_permission_policy(PermissionPolicy::RejectOnce);
    assert_eq!(inspector.permission_policy(), PermissionPolicy::RejectOnce);

    inspector.disconnect();
    let mut changes = inspector.permission_policy_changes();
    until(&mut changes, "the policy to reset", || {
        inspector.permission_policy() == PermissionPolicy::Ask
    })
    .await;
}

#[tokio::test]
async fn schema_1_11_tool_names_compaction_and_notices_decode() {
    const CALL: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"c1","title":"Reading","name":"read_file"}}}"#;
    const RENAMED: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"completed"}}}"#;
    const CHUNK: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"compaction_summary_chunk","compactionId":"k1","content":{"type":"text","text":"first"}}}}"#;
    const MORE: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"compaction_summary_chunk","compactionId":"k1","content":{"type":"text","text":"second"}}}}"#;
    const DONE: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"compaction_update","compactionId":"k1","status":"completed"}}}"#;
    const NOTICE: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"notice","severity":"warning","title":"Rate limited","description":"Retrying soon"}}}"#;

    let inspector = connected(
        ClientServices::default(),
        after_initialize(&[CALL, RENAMED, CHUNK, MORE, DONE, NOTICE]),
    );
    inspector.initialize().await.unwrap();
    let mut changes = inspector.timeline().changes();
    until(&mut changes, "the notice to land", || {
        inspector
            .timeline()
            .entries()
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::Update(v1::SessionUpdate::Notice(_))))
    })
    .await;

    let entries = inspector.timeline().entries();
    assert_eq!(
        entries.len(),
        3,
        "a tool call, a compaction and a notice: {entries:#?}"
    );
    let EntryKind::Update(v1::SessionUpdate::ToolCall(call)) = &entries[0].kind else {
        panic!("a tool call: {:?}", entries[0].kind);
    };
    assert_eq!(
        call.name.as_deref(),
        Some("read_file"),
        "the name survives a patch"
    );
    assert_eq!(call.status, v1::ToolCallStatus::Completed);

    let EntryKind::Update(v1::SessionUpdate::CompactionUpdate(compaction)) = &entries[1].kind
    else {
        panic!("one compaction entry: {:?}", entries[1].kind);
    };
    assert_eq!(entries[1].frames.len(), 3, "made of all three frames");
    assert_eq!(compaction.status, v1::CompactionStatus::Completed);
    let acp_inspector_core::MaybeUndefined::Value(summary) = &compaction.summary else {
        panic!("a summary: {:?}", compaction.summary);
    };
    assert_eq!(summary.len(), 2, "the chunks appended in order");

    let EntryKind::Update(v1::SessionUpdate::Notice(notice)) = &entries[2].kind else {
        panic!("a notice");
    };
    assert_eq!(notice.title, "Rate limited");
    assert_eq!(notice.severity, v1::NoticeSeverity::Warning);
}
