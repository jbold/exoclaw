use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::server::AppState;

#[derive(Deserialize)]
struct RpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct RpcResponse {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Handle an incoming JSON-RPC-style message. Returns a JSON response string.
pub async fn handle_rpc(msg: &str, state: &Arc<AppState>) -> Option<String> {
    let req: RpcRequest = match serde_json::from_str(msg) {
        Ok(r) => r,
        Err(e) => {
            warn!("malformed rpc: {e}");
            return Some(
                serde_json::to_string(&RpcResponse {
                    id: "0".into(),
                    result: None,
                    error: Some(format!("parse error: {e}")),
                })
                .ok()?,
            );
        }
    };

    let response = match req.method.as_str() {
        "ping" => RpcResponse {
            id: req.id,
            result: Some(serde_json::json!("pong")),
            error: None,
        },

        "status" => RpcResponse {
            id: req.id,
            result: Some(serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "plugins": state.plugins.read().await.count(),
                "sessions": state.router.session_count(),
            })),
            error: None,
        },

        "chat.send" => {
            // TODO: route to agent runner
            RpcResponse {
                id: req.id,
                result: Some(serde_json::json!({"queued": true})),
                error: None,
            }
        }

        "plugin.list" => {
            let plugins = state.plugins.read().await;
            RpcResponse {
                id: req.id,
                result: Some(serde_json::json!(plugins.list())),
                error: None,
            }
        }

        _ => RpcResponse {
            id: req.id,
            result: None,
            error: Some(format!("unknown method: {}", req.method)),
        },
    };

    serde_json::to_string(&response).ok()
}
