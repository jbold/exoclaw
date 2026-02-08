use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;
use tracing::debug;

use super::AgentEvent;

/// Trait for LLM provider implementations.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn call_streaming(
        &self,
        messages: &[serde_json::Value],
        tools: &[serde_json::Value],
        system_prompt: Option<&str>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()>;
}

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String, max_tokens: u32) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            max_tokens,
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn call_streaming(
        &self,
        messages: &[serde_json::Value],
        tools: &[serde_json::Value],
        system_prompt: Option<&str>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": messages,
            "stream": true,
        });

        if let Some(system) = system_prompt {
            body["system"] = serde_json::json!(system);
        }

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
        }

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
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
            let _ = tx.send(AgentEvent::Done).await;
            return Ok(());
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_input = String::new();
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let event_text = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                // Parse SSE event type and data
                let mut event_type = String::new();
                let mut data = String::new();
                for line in event_text.lines() {
                    if let Some(et) = line.strip_prefix("event: ") {
                        event_type = et.to_string();
                    } else if let Some(d) = line.strip_prefix("data: ") {
                        data = d.to_string();
                    }
                }

                if data.is_empty() || data == "[DONE]" {
                    continue;
                }

                let parsed: serde_json::Value = match serde_json::from_str(&data) {
                    Ok(v) => v,
                    Err(e) => {
                        debug!("skipping unparseable SSE data: {e}");
                        continue;
                    }
                };

                match event_type.as_str() {
                    "message_start" => {
                        // Extract usage from message_start
                        if let Some(usage) = parsed.get("message").and_then(|m| m.get("usage")) {
                            if let Some(it) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                                input_tokens = it as u32;
                            }
                        }
                    }

                    "content_block_start" => {
                        if let Some(cb) = parsed.get("content_block") {
                            if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                current_tool_id = cb
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                current_tool_name = cb
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                current_tool_input.clear();
                            }
                        }
                    }

                    "content_block_delta" => {
                        if let Some(delta) = parsed.get("delta") {
                            let delta_type = delta.get("type").and_then(|t| t.as_str());
                            match delta_type {
                                Some("text_delta") => {
                                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                        let _ = tx.send(AgentEvent::Text(text.into())).await;
                                    }
                                }
                                Some("input_json_delta") => {
                                    if let Some(json) =
                                        delta.get("partial_json").and_then(|t| t.as_str())
                                    {
                                        current_tool_input.push_str(json);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    "content_block_stop" => {
                        if !current_tool_id.is_empty() {
                            let input: serde_json::Value =
                                serde_json::from_str(&current_tool_input)
                                    .unwrap_or(serde_json::Value::Object(Default::default()));
                            let _ = tx
                                .send(AgentEvent::ToolUse {
                                    id: current_tool_id.clone(),
                                    name: current_tool_name.clone(),
                                    input,
                                })
                                .await;
                            current_tool_id.clear();
                            current_tool_name.clear();
                            current_tool_input.clear();
                        }
                    }

                    "message_delta" => {
                        if let Some(usage) = parsed.get("usage") {
                            if let Some(ot) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                                output_tokens = ot as u32;
                            }
                        }
                    }

                    "message_stop" => {
                        let _ = tx
                            .send(AgentEvent::Usage {
                                input_tokens,
                                output_tokens,
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        return Ok(());
                    }

                    _ => {}
                }
            }
        }

        let _ = tx
            .send(AgentEvent::Usage {
                input_tokens,
                output_tokens,
            })
            .await;
        let _ = tx.send(AgentEvent::Done).await;
        Ok(())
    }
}

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String, max_tokens: u32) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            max_tokens,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn call_streaming(
        &self,
        messages: &[serde_json::Value],
        tools: &[serde_json::Value],
        system_prompt: Option<&str>,
        tx: mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        // Prepend system message if provided
        let mut all_messages = Vec::new();
        if let Some(system) = system_prompt {
            all_messages.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }
        all_messages.extend_from_slice(messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": all_messages,
            "max_tokens": self.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
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
            let _ = tx.send(AgentEvent::Done).await;
            return Ok(());
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut tool_calls: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                if let Some(data) = event.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = tx
                            .send(AgentEvent::Usage {
                                input_tokens,
                                output_tokens,
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        return Ok(());
                    }

                    let parsed: serde_json::Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Check for usage in the final chunk
                    if let Some(usage) = parsed.get("usage") {
                        if let Some(it) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                            input_tokens = it as u32;
                        }
                        if let Some(ot) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                            output_tokens = ot as u32;
                        }
                    }

                    if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            let delta = choice.get("delta");
                            let finish_reason =
                                choice.get("finish_reason").and_then(|f| f.as_str());

                            // Handle text content
                            if let Some(text) = delta
                                .and_then(|d| d.get("content"))
                                .and_then(|c| c.as_str())
                            {
                                let _ = tx.send(AgentEvent::Text(text.into())).await;
                            }

                            // Handle tool calls
                            if let Some(tcs) = delta
                                .and_then(|d| d.get("tool_calls"))
                                .and_then(|t| t.as_array())
                            {
                                for tc in tcs {
                                    let index =
                                        tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as usize;
                                    let entry = tool_calls.entry(index).or_insert_with(|| {
                                        (String::new(), String::new(), String::new())
                                    });

                                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                        entry.0 = id.to_string();
                                    }
                                    if let Some(func) = tc.get("function") {
                                        if let Some(name) =
                                            func.get("name").and_then(|n| n.as_str())
                                        {
                                            entry.1 = name.to_string();
                                        }
                                        if let Some(args) =
                                            func.get("arguments").and_then(|a| a.as_str())
                                        {
                                            entry.2.push_str(args);
                                        }
                                    }
                                }
                            }

                            // Emit tool calls on stop
                            if finish_reason == Some("tool_calls") {
                                let mut indices: Vec<usize> = tool_calls.keys().copied().collect();
                                indices.sort();
                                for idx in indices {
                                    if let Some((id, name, args)) = tool_calls.remove(&idx) {
                                        let input: serde_json::Value = serde_json::from_str(&args)
                                            .unwrap_or(serde_json::Value::Object(
                                                Default::default(),
                                            ));
                                        let _ =
                                            tx.send(AgentEvent::ToolUse { id, name, input }).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = tx
            .send(AgentEvent::Usage {
                input_tokens,
                output_tokens,
            })
            .await;
        let _ = tx.send(AgentEvent::Done).await;
        Ok(())
    }
}

/// Create a provider from config.
pub fn from_config(config: &crate::config::AgentDefConfig) -> anyhow::Result<Box<dyn LlmProvider>> {
    let api_key = config.api_key.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "no API key for provider '{}'. Set {} env var.",
            config.provider,
            match config.provider.as_str() {
                "anthropic" => "ANTHROPIC_API_KEY",
                "openai" => "OPENAI_API_KEY",
                _ => "the appropriate API key",
            }
        )
    })?;

    match config.provider.as_str() {
        "anthropic" => Ok(Box::new(AnthropicProvider::new(
            api_key,
            config.model.clone(),
            config.max_tokens,
        ))),
        "openai" => Ok(Box::new(OpenAiProvider::new(
            api_key,
            config.model.clone(),
            config.max_tokens,
        ))),
        other => anyhow::bail!("unknown provider: {other}"),
    }
}

/// Build tool schemas for the Anthropic API from plugin describe() output.
///
/// Anthropic format:
/// ```json
/// { "name": "echo", "description": "...", "input_schema": { "type": "object", ... } }
/// ```
pub fn build_anthropic_tools(schemas: &[serde_json::Value]) -> Vec<serde_json::Value> {
    schemas
        .iter()
        .map(|schema| {
            serde_json::json!({
                "name": schema.get("name").and_then(|n| n.as_str()).unwrap_or("unknown"),
                "description": schema.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                "input_schema": schema.get("input_schema").cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
            })
        })
        .collect()
}

/// Build tool schemas for the OpenAI API from plugin describe() output.
///
/// OpenAI format:
/// ```json
/// { "type": "function", "function": { "name": "echo", "description": "...", "parameters": { ... } } }
/// ```
pub fn build_openai_tools(schemas: &[serde_json::Value]) -> Vec<serde_json::Value> {
    schemas
        .iter()
        .map(|schema| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": schema.get("name").and_then(|n| n.as_str()).unwrap_or("unknown"),
                    "description": schema.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                    "parameters": schema.get("input_schema").cloned()
                        .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
                }
            })
        })
        .collect()
}

/// Build tool schemas in the right format for a given provider.
pub fn build_tools_for_provider(
    provider: &str,
    schemas: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    match provider {
        "anthropic" => build_anthropic_tools(schemas),
        "openai" => build_openai_tools(schemas),
        _ => build_anthropic_tools(schemas), // default to Anthropic format
    }
}
