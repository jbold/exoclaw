use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Minimal LLM agent runner. Calls provider APIs with tool support.
///
/// This is the core loop: send messages → get response → if tool_use, execute
/// tool via WASM sandbox → feed result back → repeat until text response.

#[derive(Clone)]
pub struct AgentRunner {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub provider: String,      // "anthropic", "openai"
    pub model: String,         // "claude-sonnet-4-5-20250929", "gpt-4o"
    pub api_key: String,
    pub max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// A streaming chunk from the LLM.
#[derive(Debug)]
pub enum AgentEvent {
    Text(String),
    ToolUse { id: String, name: String, input: serde_json::Value },
    Done,
    Error(String),
}

impl AgentRunner {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Run an agent turn. Streams events back via the channel.
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
                let _ = tx.send(AgentEvent::Error(format!("unknown provider: {other}"))).await;
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
            let _ = tx.send(AgentEvent::Error(format!("{status}: {text}"))).await;
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
            let _ = tx.send(AgentEvent::Error(format!("{status}: {text}"))).await;
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
