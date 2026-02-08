use exoclaw::types::{AgentMessage, Message, MessageContent, StreamEvent};
use serde_json::json;

#[test]
fn text_message_constructor_sets_defaults() {
    let msg = Message::text("user", "hello");
    assert_eq!(msg.role, "user");
    assert!(matches!(msg.content, MessageContent::Text { .. }));
    assert!(msg.token_count.is_none());
}

#[test]
fn provider_message_for_text() {
    let msg = Message::text("user", "hello");
    let provider = msg.as_provider_message().expect("provider message");
    assert_eq!(provider["role"], "user");
    assert_eq!(provider["content"], "hello");
}

#[test]
fn provider_message_for_tool_use() {
    let msg = Message {
        role: "assistant".into(),
        content: MessageContent::ToolUse {
            id: "call-1".into(),
            name: "search".into(),
            input: json!({"q":"rust"}),
        },
        timestamp: chrono::Utc::now(),
        token_count: Some(42),
    };

    let provider = msg.as_provider_message().expect("provider message");
    assert_eq!(provider["role"], "assistant");
    assert_eq!(provider["content"][0]["type"], "tool_use");
    assert_eq!(provider["content"][0]["id"], "call-1");
    assert_eq!(provider["content"][0]["name"], "search");
    assert_eq!(provider["content"][0]["input"]["q"], "rust");
}

#[test]
fn provider_message_for_tool_result() {
    let msg = Message {
        role: "user".into(),
        content: MessageContent::ToolResult {
            tool_use_id: "call-1".into(),
            content: "done".into(),
            is_error: true,
        },
        timestamp: chrono::Utc::now(),
        token_count: None,
    };

    let provider = msg.as_provider_message().expect("provider message");
    assert_eq!(provider["role"], "user");
    assert_eq!(provider["content"][0]["type"], "tool_result");
    assert_eq!(provider["content"][0]["tool_use_id"], "call-1");
    assert_eq!(provider["content"][0]["content"], "done");
    assert_eq!(provider["content"][0]["is_error"], true);
}

#[test]
fn agent_message_defaults_peer_to_main() {
    let raw = json!({
        "channel": "web",
        "account": "acct",
        "content": "hello",
        "guild": null,
        "team": null
    });

    let msg: AgentMessage = serde_json::from_value(raw).expect("deserialize AgentMessage");
    assert_eq!(msg.peer, "main");
    assert_eq!(msg.channel, "web");
    assert_eq!(msg.account, "acct");
    assert_eq!(msg.content, "hello");
}

#[test]
fn stream_event_to_frame_formats_wire_payloads() {
    let request_id = "req-1";

    let text = StreamEvent::Text("chunk".into()).to_frame(request_id);
    assert_eq!(text["id"], request_id);
    assert_eq!(text["event"], "text");
    assert_eq!(text["data"], "chunk");

    let tool_use = StreamEvent::ToolUse {
        id: "call-2".into(),
        name: "lookup".into(),
        input: json!({"x":1}),
    }
    .to_frame(request_id);
    assert_eq!(tool_use["event"], "tool_use");
    assert_eq!(tool_use["data"]["id"], "call-2");
    assert_eq!(tool_use["data"]["name"], "lookup");
    assert_eq!(tool_use["data"]["input"]["x"], 1);

    let tool_result = StreamEvent::ToolResult {
        tool_use_id: "call-2".into(),
        content: "ok".into(),
        is_error: false,
    }
    .to_frame(request_id);
    assert_eq!(tool_result["event"], "tool_result");
    assert_eq!(tool_result["data"]["tool_use_id"], "call-2");
    assert_eq!(tool_result["data"]["content"], "ok");
    assert_eq!(tool_result["data"]["is_error"], false);

    let usage = StreamEvent::Usage {
        input_tokens: 10,
        output_tokens: 4,
    }
    .to_frame(request_id);
    assert_eq!(usage["event"], "usage");
    assert_eq!(usage["data"]["input_tokens"], 10);
    assert_eq!(usage["data"]["output_tokens"], 4);

    let done = StreamEvent::Done.to_frame(request_id);
    assert_eq!(done["event"], "done");

    let error = StreamEvent::Error("boom".into()).to_frame(request_id);
    assert_eq!(error["event"], "error");
    assert_eq!(error["data"], "boom");
}
