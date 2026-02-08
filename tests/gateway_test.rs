use axum::{Router, http::header, response::IntoResponse, routing::post};
use exoclaw::config::ExoclawConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep, timeout};

async fn mock_openai_handler() -> impl IntoResponse {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":1}}\n\n",
        "data: [DONE]\n\n"
    );
    ([(header::CONTENT_TYPE, "text/event-stream")], body)
}

async fn start_mock_openai_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route("/v1/chat/completions", post(mock_openai_handler));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}/v1/chat/completions"), handle)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct WsClient {
    stream: TcpStream,
    read_buffer: Vec<u8>,
}

impl WsClient {
    async fn connect(host: &str, port: u16, path: &str) -> anyhow::Result<Self> {
        let mut stream = TcpStream::connect((host, port)).await?;
        let request = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}:{port}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n"
        );
        stream.write_all(request.as_bytes()).await?;

        let mut response = Vec::new();
        let header_end;
        loop {
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                anyhow::bail!("websocket handshake closed early");
            }
            response.extend_from_slice(&buf[..n]);
            if let Some(pos) = response.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = pos + 4;
                break;
            }
        }

        let response_text = String::from_utf8_lossy(&response[..header_end]);
        anyhow::ensure!(
            response_text.starts_with("HTTP/1.1 101"),
            "unexpected websocket handshake response: {response_text}"
        );

        let read_buffer = response[header_end..].to_vec();

        Ok(Self {
            stream,
            read_buffer,
        })
    }

    async fn send_text(&mut self, payload: &str) -> anyhow::Result<()> {
        let payload = payload.as_bytes();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81); // FIN + text frame

        let mask_bit = 0x80u8;
        if payload.len() < 126 {
            frame.push(mask_bit | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            frame.push(mask_bit | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(mask_bit | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }

        let mask = [0x12u8, 0x34, 0x56, 0x78];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }

        self.stream.write_all(&frame).await?;
        Ok(())
    }

    async fn read_exact_ws(&mut self, buf: &mut [u8]) -> anyhow::Result<()> {
        let mut offset = 0usize;
        while offset < buf.len() {
            if !self.read_buffer.is_empty() {
                let take = (buf.len() - offset).min(self.read_buffer.len());
                buf[offset..offset + take].copy_from_slice(&self.read_buffer[..take]);
                self.read_buffer.drain(..take);
                offset += take;
                continue;
            }

            let n = self.stream.read(&mut buf[offset..]).await?;
            if n == 0 {
                anyhow::bail!("connection closed while reading websocket frame");
            }
            offset += n;
        }

        Ok(())
    }

    async fn recv_text(&mut self) -> anyhow::Result<String> {
        let mut header = [0u8; 2];
        self.read_exact_ws(&mut header).await?;

        let opcode = header[0] & 0x0f;
        let masked = (header[1] & 0x80) != 0;
        let mut len = (header[1] & 0x7f) as u64;

        if len == 126 {
            let mut ext = [0u8; 2];
            self.read_exact_ws(&mut ext).await?;
            len = u16::from_be_bytes(ext) as u64;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            self.read_exact_ws(&mut ext).await?;
            len = u64::from_be_bytes(ext);
        }

        let mut mask = [0u8; 4];
        if masked {
            self.read_exact_ws(&mut mask).await?;
        }

        let mut payload = vec![0u8; len as usize];
        self.read_exact_ws(&mut payload).await?;

        if masked {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
        }

        match opcode {
            0x1 => Ok(String::from_utf8(payload)?),
            0x8 => anyhow::bail!("received close frame"),
            other => anyhow::bail!("unexpected opcode: {other}"),
        }
    }

    async fn recv_json(&mut self) -> anyhow::Result<serde_json::Value> {
        let text = self.recv_text().await?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        Ok(value)
    }

    async fn recv_json_timeout(&mut self, label: &str) -> anyhow::Result<serde_json::Value> {
        timeout(Duration::from_secs(5), self.recv_json())
            .await
            .map_err(|_| anyhow::anyhow!("timeout waiting for websocket frame: {label}"))?
    }
}

async fn connect_ws_with_retry(host: &str, port: u16) -> WsClient {
    let mut last_err = None;
    for _ in 0..40 {
        match WsClient::connect(host, port, "/ws").await {
            Ok(client) => return client,
            Err(e) => {
                last_err = Some(e);
                sleep(Duration::from_millis(50)).await;
            }
        }
    }
    panic!("failed to connect websocket: {last_err:?}");
}

fn gateway_config(port: u16, openai: bool) -> ExoclawConfig {
    let mut config = ExoclawConfig::default();
    config.gateway.bind = "127.0.0.1".to_string();
    config.gateway.port = port;
    if openai {
        config.agent.provider = "openai".to_string();
        config.agent.model = "gpt-4o".to_string();
        config.agent.api_key = Some("test-key".to_string());
    }
    config
}

#[tokio::test]
async fn authenticated_chat_send_streams_text_and_done() {
    let (mock_endpoint, mock_handle) = start_mock_openai_server().await;
    // SAFETY: test-scoped env mutation for provider endpoint override.
    unsafe {
        std::env::set_var("EXOCLAW_OPENAI_ENDPOINT", &mock_endpoint);
    }

    let port = free_port();
    let config = gateway_config(port, true);
    let gateway = tokio::spawn(async move {
        let _ = exoclaw::gateway::run(config, Some("secret-token".to_string())).await;
    });

    let mut ws = connect_ws_with_retry("127.0.0.1", port).await;
    ws.send_text(r#"{"token":"secret-token"}"#).await.unwrap();

    let hello = ws.recv_json_timeout("auth hello").await.unwrap();
    assert_eq!(hello["ok"], true);

    ws.send_text(
        r#"{"id":"chat1","method":"chat.send","params":{"channel":"websocket","account":"me","content":"hello there"}}"#,
    )
    .await
    .unwrap();

    let mut saw_text = false;
    let mut saw_done = false;
    for _ in 0..10 {
        let frame = ws.recv_json_timeout("chat frame").await.unwrap();
        if frame["id"] != "chat1" {
            continue;
        }
        match frame["event"].as_str() {
            Some("text") => saw_text = true,
            Some("done") => {
                saw_done = true;
                break;
            }
            Some("error") => panic!("unexpected stream error: {frame}"),
            _ => {}
        }
    }

    assert!(saw_text, "expected at least one text event");
    assert!(saw_done, "expected done event");

    // SAFETY: undo test-scoped env mutation.
    unsafe {
        std::env::remove_var("EXOCLAW_OPENAI_ENDPOINT");
    }
    gateway.abort();
    let _ = gateway.await;
    mock_handle.abort();
    let _ = mock_handle.await;
}

#[tokio::test]
async fn unauthenticated_connection_is_rejected() {
    let port = free_port();
    let config = gateway_config(port, false);
    let gateway = tokio::spawn(async move {
        let _ = exoclaw::gateway::run(config, Some("secret-token".to_string())).await;
    });

    let mut ws = connect_ws_with_retry("127.0.0.1", port).await;
    ws.send_text(r#"{"token":"wrong-token"}"#).await.unwrap();
    let response = ws.recv_json_timeout("unauth response").await.unwrap();
    assert_eq!(response["error"], "auth_failed");
    assert_eq!(response["code"], 4001);

    gateway.abort();
    let _ = gateway.await;
}

#[tokio::test]
async fn loopback_mode_allows_no_auth_ping() {
    let port = free_port();
    let config = gateway_config(port, false);
    let gateway = tokio::spawn(async move {
        let _ = exoclaw::gateway::run(config, None).await;
    });

    let mut ws = connect_ws_with_retry("127.0.0.1", port).await;
    let hello = ws.recv_json_timeout("loopback hello").await.unwrap();
    assert_eq!(hello["ok"], true);

    ws.send_text(r#"{"id":"p1","method":"ping"}"#)
        .await
        .unwrap();
    let pong = ws.recv_json_timeout("loopback pong").await.unwrap();
    assert_eq!(pong["id"], "p1");
    assert_eq!(pong["result"], "pong");

    gateway.abort();
    let _ = gateway.await;
}
