use extism_pdk::*;
use serde::{Deserialize, Serialize};

/// Simulated platform webhook payload (what the platform sends us).
#[derive(Deserialize)]
struct WebhookPayload {
    /// The user's message text.
    text: String,
    /// Platform user ID.
    user_id: String,
    /// Optional conversation/thread ID.
    thread_id: Option<String>,
}

/// Normalized message returned to the host.
#[derive(Serialize)]
struct NormalizedMessage {
    content: String,
    account: String,
    peer: String,
}

/// Outgoing response to format for the platform.
#[derive(Deserialize)]
struct OutgoingResponse {
    content: String,
}

/// Platform-formatted reply.
#[derive(Serialize)]
struct PlatformReply {
    text: String,
    channel: String,
}

/// Parse an incoming platform webhook payload into a normalized AgentMessage.
///
/// Input: raw platform JSON (e.g., `{"text": "hello", "user_id": "u123"}`)
/// Output: normalized JSON `{"content": "hello", "account": "u123", "peer": "main"}`
#[plugin_fn]
pub fn parse_incoming(input: String) -> FnResult<String> {
    let payload: WebhookPayload = serde_json::from_str(&input)
        .map_err(|e| Error::msg(format!("invalid webhook payload: {e}")))?;

    let normalized = NormalizedMessage {
        content: payload.text,
        account: payload.user_id,
        peer: payload.thread_id.unwrap_or_else(|| "main".into()),
    };

    let output = serde_json::to_string(&normalized)
        .map_err(|e| Error::msg(format!("serialize failed: {e}")))?;

    Ok(output)
}

/// Format a normalized agent response into platform-specific payload.
///
/// Input: `{"content": "response text"}`
/// Output: `{"text": "response text", "channel": "mock"}`
#[plugin_fn]
pub fn format_outgoing(input: String) -> FnResult<String> {
    let response: OutgoingResponse = serde_json::from_str(&input)
        .map_err(|e| Error::msg(format!("invalid response: {e}")))?;

    let reply = PlatformReply {
        text: response.content,
        channel: "mock".into(),
    };

    let output = serde_json::to_string(&reply)
        .map_err(|e| Error::msg(format!("serialize failed: {e}")))?;

    Ok(output)
}

/// Describe this channel adapter.
#[plugin_fn]
pub fn describe(_input: String) -> FnResult<String> {
    let schema = serde_json::json!({
        "name": "mock",
        "type": "channel_adapter",
        "channel": "mock",
        "description": "Mock channel adapter for testing the webhook pipeline"
    });

    Ok(serde_json::to_string(&schema).unwrap())
}
