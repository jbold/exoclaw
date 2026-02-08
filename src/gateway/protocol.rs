use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::server::AppState;
use crate::agent::AgentEvent;
use crate::agent::metering;
use crate::types::Message as AgentMessage;

#[derive(Deserialize)]
struct RpcRequest {
    id: RpcId,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RpcId {
    String(String),
    Number(serde_json::Number),
}

impl RpcId {
    fn into_string(self) -> String {
        match self {
            RpcId::String(value) => value,
            RpcId::Number(value) => value.to_string(),
        }
    }
}

/// Parameters for the `chat.send` RPC method.
#[derive(Debug, Deserialize)]
pub struct ChatSendParams {
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

#[derive(Serialize)]
struct RpcResponse {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Result of handling an RPC request.
/// Either a single JSON response or a stream of events.
pub enum RpcResult {
    Response(String),
    Stream {
        id: String,
        session_key: String,
        agent_id: String,
        user_content: String,
        rx: mpsc::Receiver<AgentEvent>,
    },
}

fn event_kind(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::Text(_) => "text",
        AgentEvent::ToolUse { .. } => "tool_use",
        AgentEvent::ToolResult { .. } => "tool_result",
        AgentEvent::Usage { .. } => "usage",
        AgentEvent::Done => "done",
        AgentEvent::Error(_) => "error",
    }
}

/// Handle an incoming JSON-RPC-style message.
pub async fn handle_rpc(msg: &str, state: &Arc<AppState>) -> RpcResult {
    let req: RpcRequest = match serde_json::from_str(msg) {
        Ok(r) => r,
        Err(e) => {
            warn!("malformed rpc: {e}");
            let resp = serde_json::to_string(&RpcResponse {
                id: "0".into(),
                result: None,
                error: Some(format!("parse error: {e}")),
            })
            .unwrap_or_default();
            return RpcResult::Response(resp);
        }
    };

    let request_id = req.id.into_string();

    match req.method.as_str() {
        "ping" => {
            let resp = RpcResponse {
                id: request_id,
                result: Some(serde_json::json!("pong")),
                error: None,
            };
            RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default())
        }

        "status" => {
            let router = state.router.read().await;
            let resp = RpcResponse {
                id: request_id,
                result: Some(serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "plugins": state.plugins.read().await.count(),
                    "sessions": router.session_count(),
                })),
                error: None,
            };
            RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default())
        }

        "chat.send" => {
            let params: ChatSendParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => {
                    let resp = RpcResponse {
                        id: request_id,
                        result: None,
                        error: Some(format!("invalid chat.send params: {e}")),
                    };
                    return RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default());
                }
            };

            handle_chat_send(request_id, params, state).await
        }

        "plugin.list" => {
            let plugins = state.plugins.read().await;
            let resp = RpcResponse {
                id: request_id,
                result: Some(serde_json::json!(plugins.list())),
                error: None,
            };
            RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default())
        }

        _ => {
            let resp = RpcResponse {
                id: request_id,
                result: None,
                error: Some(format!("unknown method: {}", req.method)),
            };
            RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default())
        }
    }
}

/// Handle chat.send: resolve route, get/create session, run agent, return stream.
async fn handle_chat_send(
    request_id: String,
    params: ChatSendParams,
    state: &Arc<AppState>,
) -> RpcResult {
    // 1. Route to agent
    let route = {
        let mut router = state.router.write().await;
        router.resolve(
            &params.channel,
            &params.account,
            Some(&params.peer),
            params.guild.as_deref(),
            params.team.as_deref(),
        )
    };
    info!(
        request_id = %request_id,
        session = %route.session_key,
        agent = %route.agent_id,
        provider = %state.config.agent.provider,
        message_chars = params.content.chars().count(),
        "chat.send accepted"
    );

    // 2. Get/create session and append user message
    {
        let mut store = state.store.write().await;
        let session = store.get_or_create(&route.session_key, &route.agent_id);
        session.messages.push(serde_json::json!({
            "role": "user",
            "content": params.content.clone(),
        }));
        session.message_count += 1;
    }

    // 3. Build message history from memory context + current user message.
    let user_message = AgentMessage::text("user", params.content.clone());
    let messages = {
        let mut memory = state.memory.write().await;
        let mut context =
            memory.assemble_context(&route.session_key, &route.agent_id, &params.content);
        context.push(user_message);
        context
            .into_iter()
            .filter_map(|m| m.as_provider_message())
            .collect::<Vec<_>>()
    };

    // 4. Budget check before LLM call (T033)
    {
        let counter_mutex = metering::get_or_init_global(&state.config.budgets);
        let estimated = metering::estimate_input_tokens(&messages);
        let mut counter = counter_mutex.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(exceeded) = counter.check_budget(&route.session_key, estimated) {
            let resp = RpcResponse {
                id: request_id,
                result: None,
                error: Some(exceeded.to_string()),
            };
            return RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default());
        }
    }

    // 5. Create provider from config
    let provider = match crate::agent::providers::from_config(&state.config.agent) {
        Ok(p) => p,
        Err(e) => {
            let resp = RpcResponse {
                id: request_id,
                result: None,
                error: Some(format!("provider error: {e}")),
            };
            return RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    // 6. Build tool schemas from loaded plugins
    let tool_schemas = {
        let plugin_host = state.plugins.read().await;
        let raw_schemas = plugin_host.tool_schemas();
        crate::agent::providers::build_tools_for_provider(
            &state.config.agent.provider,
            &raw_schemas,
        )
    };

    // 7. Spawn agent task and return stream
    let (tx, rx) = mpsc::channel::<AgentEvent>(32);
    let (meter_tx, mut meter_rx) = mpsc::channel::<AgentEvent>(32);
    let session_key = route.session_key.clone();
    let state_clone = Arc::clone(state);
    let system_prompt = state.config.agent.system_prompt.clone();
    let agent_provider = state.config.agent.provider.clone();
    let agent_model = state.config.agent.model.clone();
    let agent_id = route.agent_id.clone();
    let meter_session_key = route.session_key.clone();
    let plugins = Arc::clone(&state.plugins);
    let budget_config = state.config.budgets.clone();
    let session_lock = state.session_lock(&route.session_key).await;
    let relay_request_id = request_id.clone();
    let runner_request_id = request_id.clone();

    // Metering relay: intercepts events to record usage, then forwards to client.
    tokio::spawn(async move {
        while let Some(event) = meter_rx.recv().await {
            debug!(
                request_id = %relay_request_id,
                event = event_kind(&event),
                "relaying agent event"
            );
            // Record usage when we see a Usage event (T031/T033)
            if let AgentEvent::Usage {
                input_tokens,
                output_tokens,
            } = &event
            {
                let counter_mutex = metering::get_or_init_global(&budget_config);
                let mut counter = counter_mutex.lock().unwrap_or_else(|e| e.into_inner());
                counter.record_usage(
                    &meter_session_key,
                    &agent_id,
                    &agent_provider,
                    &agent_model,
                    *input_tokens,
                    *output_tokens,
                );
            }
            if tx.send(event).await.is_err() {
                debug!(request_id = %relay_request_id, "client receiver dropped");
                break;
            }
        }
        debug!(request_id = %relay_request_id, "agent relay channel closed");
    });

    tokio::spawn(async move {
        // Serialize all processing for this session across connections.
        debug!(
            request_id = %runner_request_id,
            session = %session_key,
            "waiting for session lock"
        );
        let _session_guard = session_lock.lock().await;
        debug!(
            request_id = %runner_request_id,
            session = %session_key,
            "acquired session lock"
        );

        let runner = crate::agent::AgentRunner::new();
        let result = runner
            .run_with_tools(
                provider.as_ref(),
                messages,
                &tool_schemas,
                system_prompt.as_deref(),
                &plugins,
                meter_tx.clone(),
            )
            .await;

        if let Err(e) = result {
            warn!(
                request_id = %runner_request_id,
                session = %session_key,
                "provider run failed: {e}"
            );
            let _ = meter_tx
                .send(AgentEvent::Error(format!("provider error: {e}")))
                .await;
            let _ = meter_tx.send(AgentEvent::Done).await;
        } else {
            debug!(
                request_id = %runner_request_id,
                session = %session_key,
                "provider run completed"
            );
        }

        // Collect assistant response text and append to session
        // Note: the full response is assembled from streamed events by the caller.
        // We mark the session as updated here.
        let mut store = state_clone.store.write().await;
        if let Some(session) = store.get_mut(&session_key) {
            session.message_count += 1;
        }
    });

    RpcResult::Stream {
        id: request_id,
        session_key: route.session_key,
        agent_id: route.agent_id,
        user_content: params.content,
        rx,
    }
}
