use axum::body::Bytes;
use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
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
use crate::memory::MemoryEngine;
use crate::router::SessionRouter;
use crate::sandbox::PluginHost;
use crate::store::SessionStore;
use crate::types::Message as AgentMessage;

pub struct AppState {
    pub token: Option<String>,
    pub router: RwLock<SessionRouter>,
    pub plugins: Arc<RwLock<PluginHost>>,
    pub store: RwLock<SessionStore>,
    pub memory: Arc<RwLock<MemoryEngine>>,
    pub config: ExoclawConfig,
    /// Per-session locks for message serialization (FR-006).
    pub session_locks: RwLock<HashMap<String, Arc<Mutex<()>>>>,
}

impl AppState {
    pub async fn session_lock(&self, session_key: &str) -> Arc<Mutex<()>> {
        {
            let locks = self.session_locks.read().await;
            if let Some(lock) = locks.get(session_key) {
                return Arc::clone(lock);
            }
        }

        let mut locks = self.session_locks.write().await;
        Arc::clone(
            locks
                .entry(session_key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }
}

pub async fn run(config: ExoclawConfig, token: Option<String>) -> anyhow::Result<()> {
    let is_loopback = config.gateway.bind == "127.0.0.1" || config.gateway.bind == "::1";

    if !is_loopback && token.is_none() {
        anyhow::bail!(
            "Auth token required when binding to non-loopback address. \
             Set --token or EXOCLAW_TOKEN env var."
        );
    }

    crate::agent::metering::init_global(&config.budgets);

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
        let caps = match crate::sandbox::capabilities::parse_all(&plugin_cfg.capabilities) {
            Ok(c) => c,
            Err(e) => {
                warn!(plugin = %plugin_cfg.name, "skipping plugin (bad capabilities): {e}");
                continue;
            }
        };
        match plugin_host.register(&plugin_cfg.name, &plugin_cfg.path, caps) {
            Ok(()) => {}
            Err(e) => warn!(plugin = %plugin_cfg.name, "skipping plugin: {e}"),
        }
    }
    info!(plugins = plugin_host.count(), "plugins loaded");

    let mut memory = MemoryEngine::new(
        config.memory.episodic_window as usize,
        config.memory.semantic_enabled,
    );
    if let Some(path) = config.agent.soul_path.as_deref() {
        if let Err(e) = memory.soul.load(&config.agent.id, path) {
            warn!(agent = %config.agent.id, path, "failed to load soul: {e}");
        }
    }

    let addr = format!("{}:{}", config.gateway.bind, config.gateway.port);

    let state = Arc::new(AppState {
        token,
        router: RwLock::new(router),
        plugins: Arc::new(RwLock::new(plugin_host)),
        store: RwLock::new(SessionStore::new()),
        memory: Arc::new(RwLock::new(memory)),
        config,
        session_locks: RwLock::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health))
        .route("/webhook/{channel}", post(webhook_handler))
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

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_connection(socket, state))
}

async fn handle_connection(mut socket: WebSocket, state: Arc<AppState>) {
    if state.token.is_some() {
        // First message must be auth when token auth is enabled.
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
                    RpcResult::Stream {
                        id,
                        session_key,
                        agent_id: _agent_id,
                        user_content,
                        mut rx,
                    } => {
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
                                AgentEvent::ToolUse {
                                    id: call_id,
                                    name,
                                    input,
                                } => {
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
                                AgentEvent::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
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
                                AgentEvent::Usage {
                                    input_tokens,
                                    output_tokens,
                                } => {
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
                                            "content": assistant_text.clone(),
                                        }));
                                    }

                                    let mut memory = state.memory.write().await;
                                    let user_message =
                                        AgentMessage::text("user", user_content.clone());
                                    let assistant_message =
                                        AgentMessage::text("assistant", assistant_text.clone());
                                    memory.process_response(
                                        &session_key,
                                        &user_message,
                                        &assistant_message,
                                    );
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

/// Handle incoming webhook from a messaging platform.
///
/// 1. Look up channel adapter plugin by channel name
/// 2. Call parse_incoming() to normalize the platform payload
/// 3. Route through the agent loop
/// 4. Collect the response
/// 5. Call format_outgoing() to convert back to platform format
/// 6. Return as HTTP response
async fn webhook_handler(
    Path(channel): Path<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    // 1. Find channel adapter plugin
    let adapter_name = {
        let plugins = state.plugins.read().await;
        plugins.find_channel_adapter(&channel).map(String::from)
    };

    let adapter_name = match adapter_name {
        Some(name) => name,
        None => {
            warn!(channel = %channel, "no channel adapter found");
            return (
                StatusCode::NOT_FOUND,
                format!("no channel adapter for '{channel}'"),
            );
        }
    };

    // 2. Parse incoming payload via WASM plugin
    let parsed = {
        let plugins = state.plugins.read().await;
        plugins.call_channel_parse(&adapter_name, &body)
    };

    let parsed = match parsed {
        Ok(v) => v,
        Err(e) => {
            warn!(channel = %channel, "parse_incoming failed: {e}");
            return (
                StatusCode::BAD_REQUEST,
                format!("parse_incoming failed: {e}"),
            );
        }
    };

    // Extract message fields from normalized payload
    let content = parsed
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let account = parsed
        .get("account")
        .and_then(|a| a.as_str())
        .unwrap_or("webhook")
        .to_string();
    let peer = parsed
        .get("peer")
        .and_then(|p| p.as_str())
        .unwrap_or("main")
        .to_string();
    let guild = parsed
        .get("guild")
        .and_then(|g| g.as_str())
        .map(String::from);
    let team = parsed
        .get("team")
        .and_then(|t| t.as_str())
        .map(String::from);

    if content.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty message content".to_string());
    }

    let user_message = AgentMessage::text("user", content.clone());

    // 3. Route to agent
    let route = {
        let mut router = state.router.write().await;
        router.resolve(
            &channel,
            &account,
            Some(&peer),
            guild.as_deref(),
            team.as_deref(),
        )
    };

    let session_lock = state.session_lock(&route.session_key).await;
    let _session_guard = session_lock.lock().await;

    // 4. Get/create session and append user message
    {
        let mut store = state.store.write().await;
        let session = store.get_or_create(&route.session_key, &route.agent_id);
        session.messages.push(serde_json::json!({
            "role": "user",
            "content": content.clone(),
        }));
        session.message_count += 1;
    }

    // 5. Build message history using memory engine context
    let messages = {
        let mut memory = state.memory.write().await;
        let mut context = memory.assemble_context(&route.session_key, &route.agent_id, &content);
        context.push(user_message.clone());
        context
            .into_iter()
            .filter_map(|m| m.as_provider_message())
            .collect::<Vec<_>>()
    };

    // 6. Create provider and run agent synchronously (collect full response)
    let provider = match crate::agent::providers::from_config(&state.config.agent) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("provider error: {e}"),
            );
        }
    };

    let tool_schemas = {
        let plugin_host = state.plugins.read().await;
        let raw_schemas = plugin_host.tool_schemas();
        crate::agent::providers::build_tools_for_provider(
            &state.config.agent.provider,
            &raw_schemas,
        )
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(32);
    let system_prompt = state.config.agent.system_prompt.clone();
    let plugins = Arc::clone(&state.plugins);

    // Spawn agent task
    tokio::spawn(async move {
        let runner = crate::agent::AgentRunner::new();
        let result = runner
            .run_with_tools(
                provider.as_ref(),
                messages,
                &tool_schemas,
                system_prompt.as_deref(),
                &plugins,
                tx.clone(),
            )
            .await;

        if let Err(e) = result {
            let _ = tx
                .send(AgentEvent::Error(format!("agent error: {e}")))
                .await;
            let _ = tx.send(AgentEvent::Done).await;
        }
    });

    // 7. Collect full response text
    let mut response_text = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::Text(text) => response_text.push_str(&text),
            AgentEvent::Done => break,
            AgentEvent::Error(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("agent error: {e}"),
                );
            }
            _ => {}
        }
    }

    // 8. Append assistant response to session
    if !response_text.is_empty() {
        let mut store = state.store.write().await;
        if let Some(session) = store.get_mut(&route.session_key) {
            session.messages.push(serde_json::json!({
                "role": "assistant",
                "content": response_text.clone(),
            }));
            session.message_count += 1;
        }

        let mut memory = state.memory.write().await;
        let assistant_message = AgentMessage::text("assistant", response_text.clone());
        memory.process_response(&route.session_key, &user_message, &assistant_message);
    }

    // 9. Format outgoing via channel adapter plugin
    let formatted = {
        let plugins = state.plugins.read().await;
        plugins.call_channel_format(
            &adapter_name,
            &serde_json::json!({ "content": response_text }),
        )
    };

    let formatted_payload = match formatted {
        Ok(payload) => payload,
        Err(e) => {
            warn!(channel = %channel, "format_outgoing failed: {e}");
            // Return raw text as fallback
            return (StatusCode::OK, response_text);
        }
    };

    // 10. HTTP proxy: if format_outgoing returned JSON with a "url" field,
    //     the host makes the API call on behalf of the plugin (T045).
    //     Plugin never sees API tokens — the host manages credentials.
    let formatted_json: Option<serde_json::Value> = serde_json::from_slice(&formatted_payload).ok();

    if let Some(ref json) = formatted_json {
        if let Some(proxy_url) = json.get("url").and_then(|u| u.as_str()) {
            // Validate against allowed_hosts capability
            let allowed = {
                let plugins = state.plugins.read().await;
                plugins.allowed_hosts(&adapter_name)
            };

            let url_host = url::Url::parse(proxy_url)
                .ok()
                .and_then(|u| u.host_str().map(String::from));

            let is_allowed = match &url_host {
                Some(host) => allowed.iter().any(|h| h == host),
                None => false,
            };

            if is_allowed {
                let proxy_body = json
                    .get("body")
                    .cloned()
                    .unwrap_or(serde_json::json!({"text": response_text}));

                let client = reqwest::Client::new();
                match client.post(proxy_url).json(&proxy_body).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        info!(channel = %channel, url = %proxy_url, %status, "proxy call completed");
                        return (StatusCode::OK, body);
                    }
                    Err(e) => {
                        warn!(channel = %channel, url = %proxy_url, "proxy call failed: {e}");
                        return (StatusCode::BAD_GATEWAY, format!("proxy call failed: {e}"));
                    }
                }
            } else {
                warn!(
                    channel = %channel,
                    url = %proxy_url,
                    "proxy denied: host not in allowed_hosts"
                );
                return (
                    StatusCode::FORBIDDEN,
                    format!(
                        "proxy denied: {} not in allowed_hosts for adapter '{}'",
                        url_host.as_deref().unwrap_or("unknown"),
                        adapter_name
                    ),
                );
            }
        }
    }

    // No proxy URL — return the formatted payload directly
    (
        StatusCode::OK,
        String::from_utf8_lossy(&formatted_payload).to_string(),
    )
}
