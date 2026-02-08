use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::get,
};
use futures::SinkExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::auth;
use crate::router::SessionRouter;
use crate::sandbox::PluginHost;

pub struct Config {
    pub port: u16,
    pub bind: String,
    pub token: Option<String>,
}

pub struct AppState {
    pub token: Option<String>,
    pub router: SessionRouter,
    pub plugins: Arc<RwLock<PluginHost>>,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let is_loopback = config.bind == "127.0.0.1" || config.bind == "::1";

    if !is_loopback && config.token.is_none() {
        anyhow::bail!(
            "Auth token required when binding to non-loopback address. \
             Set --token or EXOCLAW_TOKEN env var."
        );
    }

    let state = Arc::new(AppState {
        token: config.token,
        router: SessionRouter::new(),
        plugins: Arc::new(RwLock::new(PluginHost::new())),
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health))
        .with_state(state);

    let addr = format!("{}:{}", config.bind, config.port);
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
        .send(Message::Text(
            r#"{"ok":true,"version":"0.1.0"}"#.into(),
        ))
        .await;

    info!("client connected");

    // Message loop
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                if let Some(response) = super::protocol::handle_rpc(&text, &state).await {
                    let _ = socket.send(Message::Text(response.into())).await;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    info!("client disconnected");
}
