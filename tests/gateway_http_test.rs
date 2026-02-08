use exoclaw::config::ExoclawConfig;
use tokio::time::{Duration, sleep};

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .expect("local addr")
        .port()
}

fn loopback_config(port: u16) -> ExoclawConfig {
    let mut config = ExoclawConfig::default();
    config.gateway.bind = "127.0.0.1".to_string();
    config.gateway.port = port;
    config
}

async fn wait_for_health(port: u16) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/health");

    for _ in 0..80 {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }

    panic!("gateway did not become healthy at {url}");
}

#[tokio::test]
async fn run_rejects_non_loopback_without_token() {
    let mut config = ExoclawConfig::default();
    config.gateway.bind = "0.0.0.0".to_string();
    config.gateway.port = free_port();

    let err = exoclaw::gateway::run(config, None)
        .await
        .expect_err("non-loopback run without token must fail");
    assert!(err.to_string().contains("Auth token required"));
}

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let port = free_port();
    let config = loopback_config(port);
    let gateway = tokio::spawn(async move {
        let _ = exoclaw::gateway::run(config, None).await;
    });

    wait_for_health(port).await;

    let url = format!("http://127.0.0.1:{port}/health");
    let response = reqwest::get(url).await.expect("health response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("health body");
    assert_eq!(body, "ok");

    gateway.abort();
    let _ = gateway.await;
}

#[tokio::test]
async fn ui_root_and_spa_fallback_routes_serve_html() {
    let port = free_port();
    let config = loopback_config(port);
    let gateway = tokio::spawn(async move {
        let _ = exoclaw::gateway::run(config, None).await;
    });

    wait_for_health(port).await;

    let client = reqwest::Client::new();
    let root = client
        .get(format!("http://127.0.0.1:{port}/"))
        .send()
        .await
        .expect("root response");
    assert_eq!(root.status(), reqwest::StatusCode::OK);
    let root_content_type = root
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(root_content_type.contains("text/html"));

    let fallback = client
        .get(format!("http://127.0.0.1:{port}/app/some/client-route"))
        .send()
        .await
        .expect("fallback response");
    assert_eq!(fallback.status(), reqwest::StatusCode::OK);
    let fallback_content_type = fallback
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(fallback_content_type.contains("text/html"));

    gateway.abort();
    let _ = gateway.await;
}

#[tokio::test]
async fn webhook_without_adapter_returns_not_found() {
    let port = free_port();
    let config = loopback_config(port);
    let gateway = tokio::spawn(async move {
        let _ = exoclaw::gateway::run(config, None).await;
    });

    wait_for_health(port).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{port}/webhook/discord"))
        .body(r#"{"content":"hello"}"#)
        .send()
        .await
        .expect("webhook response");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let body = response.text().await.expect("webhook body");
    assert!(body.contains("no channel adapter for 'discord'"));

    gateway.abort();
    let _ = gateway.await;
}
