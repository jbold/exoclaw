use axum::{
    Router,
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures::SinkExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use super::auth;
use super::protocol::RpcResult;
use crate::agent::AgentEvent;
use crate::config::ExoclawConfig;
use crate::router::SessionRouter;
use crate::sandbox::PluginHost;
use crate::store::SessionStore;

pub struct AppState {
    pub token: Option<String>,
    pub router: RwLock<SessionRouter>,
    pub plugins: Arc<RwLock<PluginHost>>,
    pub store: RwLock<SessionStore>,
    pub config: ExoclawConfig,
    /// Per-session locks for message serialization (FR-006).
    pub session_locks: RwLock<HashMap<String, Arc<Mutex<()>>>>,
}

pub async fn run(config: ExoclawConfig, token: Option<String>) -> anyhow::Result<()> {
    let is_loopback = config.gateway.bind == "127.0.0.1" || config.gateway.bind == "::1";

    if !is_loopback && token.is_none() {
        anyhow::bail!(
            "Auth token required when binding to non-loopback address. \
             Set --token or EXOCLAW_TOKEN env var."
        );
    }

    // Populate router with bindings from config
    let mut router = SessionRouter::new();
    for binding in &config.bindings {
        router.add_binding(crate::router::Binding {
            agent_id: binding.agent_id.clone(),
            channel: binding.channel.clone(),
            account_id: binding.account_id.clone(),
            peer_id: binding.peer_id.clone(),
            guild_id: binding.guild_id.clone(),
            team_id: binding.team_id.clone(),
        });
    }
    info!(bindings = config.bindings.len(), "router configured");

    // Load plugins from config (skip missing files with warning)
    let mut plugin_host = PluginHost::new();
    for plugin_cfg in &config.plugins {
        match plugin_host.register(&plugin_cfg.name, &plugin_cfg.path) {
            Ok(()) => {}
            Err(e) => warn!(plugin = %plugin_cfg.name, "skipping plugin: {e}"),
        }
    }
    info!(plugins = plugin_host.count(), "plugins loaded");

    let addr = format!("{}:{}", config.gateway.bind, config.gateway.port);

    let state = Arc::new(AppState {
        token,
        router: RwLock::new(router),
        plugins: Arc::new(RwLock::new(plugin_host)),
        store: RwLock::new(SessionStore::new()),
        config,
        session_locks: RwLock::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("exoclaw gateway listening on {addr}");
    if is_loopback {
        info!("bound to loopback — local access only");
    } else {
        warn!("bound to {addr} — ensure auth token is set");
    }

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_connection(socket, state))
}

async fn handle_connection(mut socket: WebSocket, state: Arc<AppState>) {
    // First message must be auth
    let authed = match socket.recv().await {
        Some(Ok(Message::Text(msg))) => auth::verify_connect(&msg, &state.token),
        _ => false,
    };

    if !authed {
        let _ = socket
            .send(Message::Text(
                r#"{"error":"auth_failed","code":4001}"#.into(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    }

    let _ = socket
        .send(Message::Text(r#"{"ok":true,"version":"0.1.0"}"#.into()))
        .await;

    info!("client connected");

    // Message loop
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                let result = super::protocol::handle_rpc(&text, &state).await;
                match result {
                    RpcResult::Response(resp) => {
                        let _ = socket.send(Message::Text(resp.into())).await;
                    }
                    RpcResult::Stream { id, session_key, mut rx } => {
                        // Stream AgentEvents as JSON frames to the client
                        let mut assistant_text = String::new();
                        while let Some(event) = rx.recv().await {
                            let frame = match &event {
                                AgentEvent::Text(text) => {
                                    assistant_text.push_str(text);
                                    serde_json::json!({
                                        "id": id,
                                        "event": "text",
                                        "data": text,
                                    })
                                }
                                AgentEvent::ToolUse { id: call_id, name, input } => {
                                    serde_json::json!({
                                        "id": id,
                                        "event": "tool_use",
                                        "data": {
                                            "id": call_id,
                                            "name": name,
                                            "input": input,
                                        },
                                    })
                                }
                                AgentEvent::ToolResult { tool_use_id, content, is_error } => {
                                    serde_json::json!({
                                        "id": id,
                                        "event": "tool_result",
                                        "data": {
                                            "tool_use_id": tool_use_id,
                                            "content": content,
                                            "is_error": is_error,
                                        },
                                    })
                                }
                                AgentEvent::Usage { input_tokens, output_tokens } => {
                                    serde_json::json!({
                                        "id": id,
                                        "event": "usage",
                                        "data": {
                                            "input_tokens": input_tokens,
                                            "output_tokens": output_tokens,
                                        },
                                    })
                                }
                                AgentEvent::Done => {
                                    serde_json::json!({
                                        "id": id,
                                        "event": "done",
                                    })
                                }
                                AgentEvent::Error(err) => {
                                    serde_json::json!({
                                        "id": id,
                                        "event": "error",
                                        "data": err,
                                    })
                                }
                            };

                            let is_done = matches!(event, AgentEvent::Done);
                            let frame_str = serde_json::to_string(&frame).unwrap_or_default();
                            if socket.send(Message::Text(frame_str.into())).await.is_err() {
                                // Client disconnected mid-stream
                                break;
                            }

                            if is_done {
                                // Append collected assistant text to session
                                if !assistant_text.is_empty() {
                                    let mut store = state.store.write().await;
                                    if let Some(session) = store.get_mut(&session_key) {
                                        session.messages.push(serde_json::json!({
                                            "role": "assistant",
                                            "content": assistant_text,
                                        }));
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    info!("client disconnected");
}
