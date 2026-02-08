use futures::{FutureExt, SinkExt, StreamExt};
use gloo_net::websocket::{Message, futures::WebSocket};
use gloo_timers::future::TimeoutFuture;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Text(String),
    ToolUse { name: String, input: String },
    Done,
    Error(String),
}

fn ws_url() -> Result<String, String> {
    let window = web_sys::window().ok_or("no window")?;
    let location = window.location();
    let protocol = location.protocol().map_err(|_| "no protocol")?;
    let host = location.host().map_err(|_| "no host")?;
    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    Ok(format!("{}//{}/ws", ws_protocol, host))
}

pub fn parse_event(msg: &str) -> Option<StreamEvent> {
    let v: Value = serde_json::from_str(msg).ok()?;

    // Handle JSON-RPC error responses (no "event" field, has "error" field)
    if let Some(err) = v.get("error") {
        if let Some(msg) = err.as_str() {
            return Some(StreamEvent::Error(msg.to_string()));
        }
        if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
            return Some(StreamEvent::Error(msg.to_string()));
        }
        return Some(StreamEvent::Error(err.to_string()));
    }

    let event = v.get("event")?.as_str()?;
    match event {
        "text" => {
            let data = v.get("data")?.as_str()?;
            Some(StreamEvent::Text(data.to_string()))
        }
        "tool_use" => {
            let data = v.get("data")?;
            let name = data.get("name")?.as_str()?.to_string();
            let input = data.get("input").map(|v| v.to_string()).unwrap_or_default();
            Some(StreamEvent::ToolUse { name, input })
        }
        "done" => Some(StreamEvent::Done),
        "error" => {
            let data = v
                .get("data")
                .and_then(|d| d.as_str())
                .unwrap_or("unknown error");
            Some(StreamEvent::Error(data.to_string()))
        }
        _ => None,
    }
}

pub struct WsConnection {
    pub write: futures::stream::SplitSink<WebSocket, Message>,
    pub read: futures::stream::SplitStream<WebSocket>,
}

pub async fn connect(token: Option<String>) -> Result<WsConnection, String> {
    let url = ws_url()?;
    let ws = WebSocket::open(&url).map_err(|e| format!("WebSocket open failed: {}", e))?;
    let (mut write, mut read) = ws.split();

    if let Some(tok) = token {
        let auth_msg = json!({"token": tok}).to_string();
        write
            .send(Message::Text(auth_msg))
            .await
            .map_err(|e| format!("send auth failed: {}", e))?;

        // Wait for auth response
        let resp = read
            .next()
            .await
            .ok_or("connection closed during auth")?
            .map_err(|e| format!("read auth response failed: {}", e))?;

        let resp_text = match resp {
            Message::Text(t) => t,
            Message::Bytes(b) => {
                String::from_utf8(b).map_err(|_| "invalid utf8 in auth response")?
            }
        };

        let v: Value =
            serde_json::from_str(&resp_text).map_err(|_| "invalid JSON in auth response")?;
        if v.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            return Err("authentication failed".to_string());
        }
    } else {
        // In no-token mode, loopback gateway sends {"ok":true,...} immediately.
        // Token-protected gateway waits for an auth frame and sends nothing.
        // Use a short timeout to detect that auth is required.
        let read_next = read.next().fuse();
        let timeout = TimeoutFuture::new(800).fuse();
        futures::pin_mut!(read_next, timeout);

        match futures::select! {
            msg = read_next => Some(msg),
            _ = timeout => None,
        } {
            Some(Some(Ok(resp))) => {
                let resp_text = match resp {
                    Message::Text(t) => t,
                    Message::Bytes(b) => {
                        String::from_utf8(b).map_err(|_| "invalid utf8 in auth response")?
                    }
                };

                let v: Value =
                    serde_json::from_str(&resp_text).map_err(|_| "invalid JSON in response")?;
                if v.get("ok").and_then(|v| v.as_bool()) != Some(true) {
                    if let Some(err) = v.get("error") {
                        if let Some(msg) = err.as_str() {
                            return Err(msg.to_string());
                        }
                        if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
                            return Err(msg.to_string());
                        }
                        return Err(err.to_string());
                    }
                    return Err("authentication failed".to_string());
                }
            }
            Some(Some(Err(e))) => return Err(format!("read response failed: {}", e)),
            Some(None) => return Err("connection closed during handshake".to_string()),
            None => return Err("authentication required".to_string()),
        }
    }

    Ok(WsConnection { write, read })
}

pub async fn send_chat(
    write: &mut futures::stream::SplitSink<WebSocket, Message>,
    content: &str,
    id: u32,
) -> Result<(), String> {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "chat.send",
        "params": {
            "channel": "web",
            "account": "browser",
            "content": content,
        }
    });
    write
        .send(Message::Text(msg.to_string()))
        .await
        .map_err(|e| format!("send failed: {}", e))
}
