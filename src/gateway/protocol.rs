use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

use super::server::AppState;
use crate::agent::AgentEvent;

#[derive(Deserialize)]
struct RpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
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
        rx: mpsc::Receiver<AgentEvent>,
    },
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

    match req.method.as_str() {
        "ping" => {
            let resp = RpcResponse {
                id: req.id,
                result: Some(serde_json::json!("pong")),
                error: None,
            };
            RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default())
        }

        "status" => {
            let router = state.router.read().await;
            let resp = RpcResponse {
                id: req.id,
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
                        id: req.id,
                        result: None,
                        error: Some(format!("invalid chat.send params: {e}")),
                    };
                    return RpcResult::Response(
                        serde_json::to_string(&resp).unwrap_or_default(),
                    );
                }
            };

            handle_chat_send(req.id, params, state).await
        }

        "plugin.list" => {
            let plugins = state.plugins.read().await;
            let resp = RpcResponse {
                id: req.id,
                result: Some(serde_json::json!(plugins.list())),
                error: None,
            };
            RpcResult::Response(serde_json::to_string(&resp).unwrap_or_default())
        }

        _ => {
            let resp = RpcResponse {
                id: req.id,
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

    // 3. Build message history for LLM
    let messages = {
        let store = state.store.read().await;
        match store.get(&route.session_key) {
            Some(session) => session.messages.clone(),
            None => vec![serde_json::json!({
                "role": "user",
                "content": params.content,
            })],
        }
    };

    // 4. Create provider from config
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

    // 5. Spawn agent task and return stream
    let (tx, rx) = mpsc::channel::<AgentEvent>(32);
    let session_key = route.session_key.clone();
    let state_clone = Arc::clone(state);
    let system_prompt = state.config.agent.system_prompt.clone();

    tokio::spawn(async move {
        let result = provider
            .call_streaming(&messages, &[], system_prompt.as_deref(), tx.clone())
            .await;

        if let Err(e) = result {
            let _ = tx.send(AgentEvent::Error(format!("provider error: {e}"))).await;
            let _ = tx.send(AgentEvent::Done).await;
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
        rx,
    }
}
