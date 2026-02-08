use exoclaw_ui::markdown;
use exoclaw_ui::ws::{StreamEvent, parse_event};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn parse_event_accepts_jsonrpc_string_error() {
    let msg = r#"{"error":"auth_failed","code":4001}"#;
    assert_eq!(
        parse_event(msg),
        Some(StreamEvent::Error("auth_failed".to_string()))
    );
}

#[wasm_bindgen_test]
fn parse_event_accepts_jsonrpc_object_error() {
    let msg = r#"{"error":{"code":-32601,"message":"method not found"}}"#;
    assert_eq!(
        parse_event(msg),
        Some(StreamEvent::Error("method not found".to_string()))
    );
}

#[wasm_bindgen_test]
fn parse_event_parses_text() {
    let msg = r#"{"id":"chat1","event":"text","data":"hello"}"#;
    assert_eq!(
        parse_event(msg),
        Some(StreamEvent::Text("hello".to_string()))
    );
}

#[wasm_bindgen_test]
fn parse_event_parses_tool_use() {
    let msg = r#"{"id":"chat1","event":"tool_use","data":{"name":"search","input":{"q":"rust"}}}"#;
    assert_eq!(
        parse_event(msg),
        Some(StreamEvent::ToolUse {
            name: "search".to_string(),
            input: r#"{"q":"rust"}"#.to_string(),
        })
    );
}

#[wasm_bindgen_test]
fn markdown_escapes_inline_html() {
    let rendered = markdown::render(r#"<script>alert("xss")</script>"#);
    assert!(rendered.contains("&lt;script&gt;alert(\"xss\")&lt;/script&gt;"));
    assert!(!rendered.contains("<script>"));
}
