pub mod metering;
pub mod providers;

use std::sync::Arc;

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};

use crate::sandbox::PluginHost;

/// Minimal LLM agent runner. Calls provider APIs with tool support.
///
/// This is the core loop: send messages -> get response -> if tool_use, execute
/// tool via WASM sandbox -> feed result back -> repeat until text response.

#[derive(Clone)]
pub struct AgentRunner {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub provider: String, // "anthropic", "openai"
    pub model: String,    // "claude-sonnet-4-5-20250929", "gpt-4o"
    pub api_key: String,
    pub max_tokens: u32,
}

/// A streaming chunk from the LLM.
#[derive(Debug)]
pub enum AgentEvent {
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

/// Maximum tool-use loop iterations to prevent infinite loops.
const MAX_TOOL_ITERATIONS: usize = 10;

impl AgentRunner {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Run an agent turn with tool-use loop support.
    ///
    /// Streams events back via the channel. If the LLM responds with tool_use,
    /// dispatches to WASM plugins and continues until a text response or max
    /// iterations are reached.
    pub async fn run_with_tools(
        &self,
        provider: &dyn providers::LlmProvider,
        messages: Vec<serde_json::Value>,
        tools: &[serde_json::Value],
        system_prompt: Option<&str>,
        plugins: &Arc<RwLock<PluginHost>>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let mut current_messages = messages;
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > MAX_TOOL_ITERATIONS {
                warn!("tool-use loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
                let _ = tx
                    .send(AgentEvent::Error(
                        "tool-use loop exceeded max iterations".into(),
                    ))
                    .await;
                let _ = tx.send(AgentEvent::Done).await;
                return Ok(());
            }

            // Create an internal channel to collect events from this LLM call
            let (inner_tx, mut inner_rx) = mpsc::channel::<AgentEvent>(32);

            provider
                .call_streaming(&current_messages, tools, system_prompt, inner_tx)
                .await?;

            // Collect events, forwarding text/usage/error to client,
            // collecting tool_use calls for dispatch
            let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

            while let Some(event) = inner_rx.recv().await {
                match event {
                    AgentEvent::Text(ref _t) => {
                        let _ = tx.send(event).await;
                    }
                    AgentEvent::ToolUse {
                        ref id,
                        ref name,
                        ref input,
                    } => {
                        // Forward to client so they can observe
                        let _ = tx
                            .send(AgentEvent::ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: input.clone(),
                            })
                            .await;
                        tool_calls.push((id.clone(), name.clone(), input.clone()));
                    }
                    AgentEvent::Usage { .. } => {
                        let _ = tx.send(event).await;
                    }
                    AgentEvent::Error(ref _e) => {
                        let _ = tx.send(event).await;
                    }
                    AgentEvent::Done => {
                        // Don't forward Done yet — we may need to continue the loop
                    }
                    AgentEvent::ToolResult { .. } => {
                        // Shouldn't come from provider, but forward if it does
                        let _ = tx.send(event).await;
                    }
                }
            }

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                let _ = tx.send(AgentEvent::Done).await;
                return Ok(());
            }

            // Build the assistant message with tool_use content blocks
            let mut assistant_content: Vec<serde_json::Value> = Vec::new();
            for (id, name, input) in &tool_calls {
                assistant_content.push(serde_json::json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                }));
            }

            // Append assistant message with tool_use blocks
            current_messages.push(serde_json::json!({
                "role": "assistant",
                "content": assistant_content,
            }));

            // Execute tools and build tool results
            let mut tool_result_content: Vec<serde_json::Value> = Vec::new();
            let plugin_host = plugins.read().await;

            for (id, name, input) in &tool_calls {
                let result = if plugin_host.has_plugin(name) {
                    plugin_host.call_tool(name, input)
                } else {
                    crate::sandbox::ToolCallResult {
                        content: format!("unknown tool: {name}"),
                        is_error: true,
                    }
                };

                info!(
                    tool = %name,
                    is_error = result.is_error,
                    "tool call completed"
                );

                // Forward result to client
                let _ = tx
                    .send(AgentEvent::ToolResult {
                        tool_use_id: id.clone(),
                        content: result.content.clone(),
                        is_error: result.is_error,
                    })
                    .await;

                tool_result_content.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": result.content,
                    "is_error": result.is_error,
                }));
            }

            drop(plugin_host);

            // Append tool results as user message
            current_messages.push(serde_json::json!({
                "role": "user",
                "content": tool_result_content,
            }));

            // Loop back to call the LLM again with the updated history
        }
    }

    /// Simple run without tool-use loop (for backward compatibility).
    pub async fn run(
        &self,
        config: &AgentConfig,
        messages: Vec<serde_json::Value>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        info!(provider = %config.provider, model = %config.model, "starting agent run");

        match config.provider.as_str() {
            "anthropic" => self.run_anthropic(config, messages, tx).await,
            "openai" => self.run_openai(config, messages, tx).await,
            other => {
                let _ = tx
                    .send(AgentEvent::Error(format!("unknown provider: {other}")))
                    .await;
                Ok(())
            }
        }
    }

    async fn run_anthropic(
        &self,
        config: &AgentConfig,
        messages: Vec<serde_json::Value>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "messages": messages,
            "stream": true,
        });

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = tx
                .send(AgentEvent::Error(format!("{status}: {text}")))
                .await;
            return Ok(());
        }

        // Stream SSE events
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE events
            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                if let Some(data) = event.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = tx.send(AgentEvent::Done).await;
                        return Ok(());
                    }
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(delta) = parsed.get("delta") {
                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                let _ = tx.send(AgentEvent::Text(text.into())).await;
                            }
                        }
                    }
                }
            }
        }

        let _ = tx.send(AgentEvent::Done).await;
        Ok(())
    }

    async fn run_openai(
        &self,
        config: &AgentConfig,
        messages: Vec<serde_json::Value>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        // OpenAI streaming follows the same SSE pattern
        let body = serde_json::json!({
            "model": config.model,
            "messages": messages,
            "stream": true,
        });

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = tx
                .send(AgentEvent::Error(format!("{status}: {text}")))
                .await;
            return Ok(());
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                if let Some(data) = event.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = tx.send(AgentEvent::Done).await;
                        return Ok(());
                    }
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                            if let Some(delta) = choices.first().and_then(|c| c.get("delta")) {
                                if let Some(text) = delta.get("content").and_then(|t| t.as_str()) {
                                    let _ = tx.send(AgentEvent::Text(text.into())).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = tx.send(AgentEvent::Done).await;
        Ok(())
    }
}

impl Default for AgentRunner {
    fn default() -> Self {
        Self::new()
    }
}
