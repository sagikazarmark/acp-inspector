mod common;

use acp_inspector_core::{AgentCapability, CallError, Driven, Inspector, v1};
use common::{sent, shell};

#[tokio::test]
async fn text_image_and_audio_cross_in_order_with_original_data_and_mime() {
    let inspector = Inspector::new();
    inspector.start(&shell(concat!(
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"promptCapabilities\":{\"image\":true,\"audio\":true,\"embeddedContext\":true}}}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"s\"}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"stopReason\":\"end_turn\"}}'; cat >/dev/null"
    ))).await.unwrap();
    inspector
        .prompt_content(vec![
            v1::ContentBlock::from("Describe this"),
            v1::ContentBlock::Image(v1::ImageContent::new("iVBORw0KGgo=", "image/png")),
            v1::ContentBlock::Audio(v1::AudioContent::new("UklGRgQAAABXQVZF", "audio/wav")),
            text_resource(),
            v1::ContentBlock::Image(v1::ImageContent::new("R0lGODlh", "image/gif")),
        ])
        .await
        .unwrap();
    let frames = sent(&inspector);
    let prompt: serde_json::Value = serde_json::from_str(
        frames
            .iter()
            .find(|frame| frame.contains("session/prompt"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        prompt["params"]["prompt"],
        serde_json::json!([
            {"type":"text","text":"Describe this"},
            {"type":"image","data":"iVBORw0KGgo=","mimeType":"image/png"},
            {"type":"audio","data":"UklGRgQAAABXQVZF","mimeType":"audio/wav"},
            {"type":"resource","resource":{"uri":"file:///tmp/a%20%23.rs","mimeType":"text/plain","text":"\u{feff}<hello>é\r\n"}},
            {"type":"image","data":"R0lGODlh","mimeType":"image/gif"}
        ])
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Image),
        Driven::Answered
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Audio),
        Driven::Answered
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::EmbeddedContext),
        Driven::Answered
    );
    inspector.disconnect();
}

fn text_resource() -> v1::ContentBlock {
    v1::ContentBlock::Resource(v1::EmbeddedResource::new(
        v1::EmbeddedResourceResource::TextResourceContents(
            v1::TextResourceContents::new("\u{feff}<hello>é\r\n", "file:///tmp/a%20%23.rs")
                .mime_type("text/plain".to_owned()),
        ),
    ))
}

#[tokio::test]
async fn embedded_only_agent_accepts_text_resources_but_not_blobs_or_oversized_frames() {
    let inspector = Inspector::new();
    inspector.start(&shell(concat!(
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"promptCapabilities\":{\"embeddedContext\":true}}}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"s\"}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"stopReason\":\"end_turn\"}}'; cat >/dev/null"
    ))).await.unwrap();
    let blob = v1::ContentBlock::Resource(v1::EmbeddedResource::new(
        v1::EmbeddedResourceResource::BlobResourceContents(v1::BlobResourceContents::new(
            "AA==",
            "file:///binary",
        )),
    ));
    assert_eq!(
        inspector.prompt_content(vec![blob]).await,
        Err(CallError::UnsupportedPromptContent)
    );
    let large = v1::ContentBlock::Resource(v1::EmbeddedResource::new(
        v1::EmbeddedResourceResource::TextResourceContents(v1::TextResourceContents::new(
            "\t".repeat(5 * 1024 * 1024),
            "file:///large.txt",
        )),
    ));
    assert_eq!(
        inspector.prompt_content(vec![large]).await,
        Err(CallError::PromptTooLarge)
    );
    assert!(
        !sent(&inspector)
            .iter()
            .any(|frame| frame.contains("session/prompt"))
    );
    inspector
        .prompt_content(vec![text_resource()])
        .await
        .unwrap();
    assert_eq!(
        inspector.driven().of(AgentCapability::EmbeddedContext),
        Driven::Answered
    );
    assert_eq!(
        inspector.driven().of(AgentCapability::Image),
        Driven::Unasked
    );
    inspector.disconnect();
}

#[tokio::test]
async fn media_require_advertisement_and_oversized_prompts_send_no_frame() {
    let inspector = Inspector::new();
    inspector.start(&shell(concat!(
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"s\"}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"stopReason\":\"end_turn\"}}'; cat >/dev/null"
    ))).await.unwrap();
    assert_eq!(
        inspector
            .prompt_content(vec![v1::ContentBlock::Image(v1::ImageContent::new(
                "abc",
                "image/png"
            ))])
            .await,
        Err(CallError::ImageNotAdvertised)
    );
    assert_eq!(
        inspector
            .prompt_content(vec![v1::ContentBlock::Audio(v1::AudioContent::new(
                "abc",
                "audio/mpeg"
            ))])
            .await,
        Err(CallError::AudioNotAdvertised)
    );
    // JSON escaping, not just raw text size, determines whether a Frame fits.
    assert_eq!(
        inspector.prompt_content(vec![text_resource()]).await,
        Err(CallError::EmbeddedContextNotAdvertised)
    );
    assert_eq!(
        inspector.prompt(&"\"".repeat(6 * 1024 * 1024)).await,
        Err(CallError::PromptTooLarge)
    );
    assert!(
        !sent(&inspector)
            .iter()
            .any(|frame| frame.contains("session/prompt"))
    );
    inspector.prompt("still connected").await.unwrap();
    assert_eq!(
        sent(&inspector)
            .iter()
            .filter(|frame| frame.contains("session/prompt"))
            .count(),
        1
    );
    inspector.disconnect();
}

#[tokio::test]
async fn admitted_prompt_keeps_its_original_session_when_sending_is_scheduled() {
    let inspector = Inspector::new();
    inspector.start(&shell(concat!(
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}'; ",
        "read _; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"original\"}}'; ",
        "while read _; do printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"stopReason\":\"end_turn\"}}'; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":4,\"result\":{\"sessionId\":\"next\"}}'; done"
    ))).await.unwrap();
    let pending = inspector
        .submit_prompt(vec![v1::ContentBlock::from("original draft")])
        .unwrap();
    assert!(
        sent(&inspector).last().unwrap().contains("session/prompt"),
        "admission enqueues before subsequent cancellation"
    );
    let _ = inspector.new_session("/tmp", None).await;
    pending.await.unwrap().unwrap();
    let frame = sent(&inspector)
        .into_iter()
        .find(|frame| frame.contains("session/prompt"))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&frame).unwrap();
    assert_eq!(value["params"]["sessionId"], "original");
    inspector.disconnect();
}
