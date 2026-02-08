use extism_pdk::*;
use serde::{Deserialize, Serialize};

/// Incoming message from the exoclaw host.
#[derive(Deserialize)]
struct IncomingMessage {
    text: String,
    channel: String,
    peer_id: String,
}

/// Response sent back to the host.
#[derive(Serialize)]
struct OutgoingMessage {
    text: String,
}

/// Tool call input (generic JSON).
#[derive(Deserialize)]
struct ToolInput {
    message: Option<String>,
}

/// Tool call result.
#[derive(Serialize)]
struct ToolResult {
    content: String,
    is_error: bool,
}

/// Main entry point called by the exoclaw plugin host.
///
/// Receives a JSON-encoded IncomingMessage and returns a JSON-encoded
/// OutgoingMessage that echoes the input back.
///
/// Build with:
///   cargo build --target wasm32-unknown-unknown --release
///   cp target/wasm32-unknown-unknown/release/echo_plugin.wasm plugins/echo.wasm
#[plugin_fn]
pub fn handle_message(input: String) -> FnResult<String> {
    let msg: IncomingMessage = serde_json::from_str(&input)
        .map_err(|e| Error::msg(format!("bad input: {e}")))?;

    let response = OutgoingMessage {
        text: format!(
            "echo from {}/{}: {}",
            msg.channel, msg.peer_id, msg.text
        ),
    };

    let output = serde_json::to_string(&response)
        .map_err(|e| Error::msg(format!("serialize failed: {e}")))?;

    Ok(output)
}

/// Tool call entry point. Takes JSON input, returns JSON result.
#[plugin_fn]
pub fn handle_tool_call(input: String) -> FnResult<String> {
    let tool_input: ToolInput = serde_json::from_str(&input)
        .map_err(|e| Error::msg(format!("bad tool input: {e}")))?;

    let message = tool_input.message.unwrap_or_else(|| "no message".into());

    let result = ToolResult {
        content: format!("echo: {message}"),
        is_error: false,
    };

    let output = serde_json::to_string(&result)
        .map_err(|e| Error::msg(format!("serialize failed: {e}")))?;

    Ok(output)
}

/// Describe the plugin's tool schema.
#[plugin_fn]
pub fn describe(_input: String) -> FnResult<String> {
    let schema = serde_json::json!({
        "name": "echo",
        "description": "Echoes the input message back. Useful for testing.",
        "input_schema": {
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo back"
                }
            },
            "required": ["message"]
        }
    });

    Ok(serde_json::to_string(&schema).unwrap())
}
