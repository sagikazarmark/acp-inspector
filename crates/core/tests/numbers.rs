//! Numeric spellings through the real Frame → Value → ACP path (§8).
//! Literal text matters here: `json!` would erase the spelling under test.

mod common;

use acp_inspector_core::{CallError, EntryKind, Inspector, v1};
use common::{received, shell, until};

const SPELLINGS: [(&str, f64); 7] = [
    ("0.10", 0.1),
    ("1e2", 100.0),
    ("1.50", 1.5),
    ("4.5e-2", 0.045),
    ("2.5E0", 2.5),
    ("0.146675314082485333", 0.14667531408248535),
    ("0.10031360309748868086", 0.10031360309748867),
];

async fn arriving(frame: &str) -> Inspector {
    let inspector = Inspector::new();
    let mut changes = inspector.timeline().changes();
    inspector.connect(&shell(&format!("printf '%s\\n' '{frame}'; cat > /dev/null")).factory());
    until(&mut changes, "the Frame to reach the Timeline", || {
        !inspector.timeline().is_empty()
    })
    .await;
    inspector
}

#[tokio::test]
async fn negative_zero_keeps_the_stock_sign_and_numeric_kind() {
    let frame = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"_meta":{"zero":-0,"text":"-0 and \"-0\"","nested":[-0,0]}}}"#;
    let inspector = Inspector::new();
    inspector.connect(
        &shell(&format!(
            "read _; printf '%s\\n' '{frame}'; cat > /dev/null"
        ))
        .factory(),
    );
    let initialized = inspector.initialize().await.unwrap();
    let meta = initialized.meta.unwrap();
    assert_eq!(meta["zero"].to_string(), "-0.0");
    assert_eq!(meta["nested"][0].to_string(), "-0.0");
    assert_eq!(meta["nested"][1].to_string(), "0");
    assert_eq!(meta["text"], "-0 and \"-0\"");
    assert_eq!(received(&inspector), [frame]);
}

#[tokio::test]
async fn results_and_error_bodies_use_the_same_numeric_boundary() {
    let result = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"_meta":{"numbers":[0.10,1e2,1.50,4.5e-2,2.5E0]}}}"#;
    let error = r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"refused","data":{"numbers":[0.10,1e2,1.50,4.5e-2,2.5E0]}}}"#;
    let inspector = Inspector::new();
    inspector.connect(
        &shell(&format!(
            "read _; printf '%s\\n' '{result}'; read _; printf '%s\\n' '{error}'; cat > /dev/null"
        ))
        .factory(),
    );

    let initialized = inspector.initialize().await.expect("initialize decodes");
    let expected = serde_json::json!([0.1, 100.0, 1.5, 0.045, 2.5]);
    assert_eq!(
        initialized.meta.expect("metadata remains present")["numbers"],
        expected
    );
    let refused = inspector
        .new_session("/tmp", None)
        .await
        .expect_err("the Agent refused");
    let CallError::Rejected(refused) = refused else {
        panic!("a typed refusal: {refused:?}");
    };
    assert_eq!(refused.message, "refused");
    assert_eq!(
        refused.data.expect("error data remains present")["numbers"],
        expected
    );
    assert_eq!(received(&inspector), [result, error]);
}

#[tokio::test]
async fn usage_cost_survives_every_numeric_spelling() {
    for (spelling, expected) in SPELLINGS {
        let frame = format!(
            r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"usage_update","used":1,"size":100,"cost":{{"amount":{spelling},"currency":"USD"}}}}}}}}"#
        );
        let inspector = arriving(&frame).await;

        let entries = inspector.timeline().entries();
        let EntryKind::Update(v1::SessionUpdate::UsageUpdate(usage)) = &entries[0].kind else {
            panic!("{spelling} must decode as usage: {entries:?}");
        };
        let cost = usage
            .cost
            .as_ref()
            .expect("the Agent's cost must not silently disappear");
        assert_eq!(cost.amount, expected, "{spelling}");
        assert_eq!(cost.currency, "USD");
        assert_eq!(entries[0].frames[0].frame.as_str(), frame);
        assert_eq!(received(&inspector), [frame]);
    }
}

#[tokio::test]
async fn annotation_priority_survives_decimal_spelling() {
    let frame = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello","annotations":{"priority":0.50}}}}}"#;
    let inspector = arriving(frame).await;

    let entries = inspector.timeline().entries();
    let EntryKind::Update(v1::SessionUpdate::AgentMessageChunk(chunk)) = &entries[0].kind else {
        panic!("a typed message chunk: {entries:?}");
    };
    let v1::ContentBlock::Text(text) = &chunk.content else {
        panic!("text content: {chunk:?}");
    };
    let annotations = text
        .annotations
        .as_ref()
        .expect("annotations remain present");
    assert_eq!(annotations.priority, Some(0.5));
    assert_eq!(entries[0].frames[0].frame.as_str(), frame);
    assert_eq!(received(&inspector), [frame]);
}

#[tokio::test]
async fn elicitation_number_and_integer_bounds_and_defaults_survive() {
    for (spelling, expected) in SPELLINGS {
        let frame = format!(
            r#"{{"jsonrpc":"2.0","id":"ask","method":"elicitation/create","params":{{"sessionId":"s1","mode":"form","message":"Numbers","requestedSchema":{{"type":"object","properties":{{"number":{{"type":"number","minimum":{spelling},"maximum":{spelling},"default":{spelling}}},"integer":{{"type":"integer","minimum":-9223372036854775808,"maximum":9223372036854775807,"default":9007199254740993}}}}}}}}}}"#
        );
        let inspector = arriving(&frame).await;

        let entries = inspector.timeline().entries();
        let requests = inspector.pending_elicitations();
        let request = requests
            .first()
            .unwrap_or_else(|| panic!("{spelling} must produce a Blocking Request: {entries:?}"));
        let schema = request.form().expect("the form carries its schema");
        let v1::ElicitationPropertySchema::Number(number) = &schema.properties["number"] else {
            panic!("a number property: {schema:?}");
        };
        assert_eq!(number.minimum, Some(expected), "{spelling}");
        assert_eq!(number.maximum, Some(expected), "{spelling}");
        assert_eq!(number.default, Some(expected), "{spelling}");
        let v1::ElicitationPropertySchema::Integer(integer) = &schema.properties["integer"] else {
            panic!("an integer property: {schema:?}");
        };
        assert_eq!(integer.minimum, Some(i64::MIN));
        assert_eq!(integer.maximum, Some(i64::MAX));
        assert_eq!(integer.default, Some(9_007_199_254_740_993));
        assert_eq!(entries[0].frames[0].frame.as_str(), frame);
        assert_eq!(received(&inspector), [frame]);
    }
}
