use serde::{Deserialize, Serialize};

/// A message in a conversation. Used for episodic memory and LLM context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,
}

/// Content of a message — text, tool use request, or tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl Message {
    /// Create a text message with current timestamp.
    pub fn text(role: &str, text: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: MessageContent::Text { text: text.into() },
            timestamp: chrono::Utc::now(),
            token_count: None,
        }
    }

    /// Convert to a provider-facing message format.
    pub fn as_provider_message(&self) -> Option<serde_json::Value> {
        match &self.content {
            MessageContent::Text { text } => Some(serde_json::json!({
                "role": self.role,
                "content": text,
            })),
            MessageContent::ToolUse { id, name, input } => Some(serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                }],
            })),
            MessageContent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => Some(serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content,
                    "is_error": is_error,
                }],
            })),
        }
    }
}

/// Normalized incoming message from any channel.
/// Used by the router to determine the target agent and session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub channel: String,
    pub account: String,
    #[serde(default = "default_peer")]
    pub peer: String,
    pub content: String,
    pub guild: Option<String>,
    pub team: Option<String>,
}

fn default_peer() -> String {
    "main".into()
}

/// A streaming event sent over the WebSocket to the client.
///
/// Wire format: `{"id": "req-id", "event": "text", "data": "chunk"}`
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },
    Done,
    Error(String),
}

impl StreamEvent {
    /// Serialize this event as a JSON wire frame with the given request ID.
    pub fn to_frame(&self, request_id: &str) -> serde_json::Value {
        match self {
            StreamEvent::Text(data) => serde_json::json!({
                "id": request_id,
                "event": "text",
                "data": data,
            }),
            StreamEvent::ToolUse { id, name, input } => serde_json::json!({
                "id": request_id,
                "event": "tool_use",
                "data": { "id": id, "name": name, "input": input },
            }),
            StreamEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => serde_json::json!({
                "id": request_id,
                "event": "tool_result",
                "data": { "tool_use_id": tool_use_id, "content": content, "is_error": is_error },
            }),
            StreamEvent::Usage {
                input_tokens,
                output_tokens,
            } => serde_json::json!({
                "id": request_id,
                "event": "usage",
                "data": { "input_tokens": input_tokens, "output_tokens": output_tokens },
            }),
            StreamEvent::Done => serde_json::json!({
                "id": request_id,
                "event": "done",
            }),
            StreamEvent::Error(data) => serde_json::json!({
                "id": request_id,
                "event": "error",
                "data": data,
            }),
        }
    }
}
