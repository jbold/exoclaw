use exoclaw::agent::AgentEvent;
use exoclaw::config::ExoclawConfig;
use exoclaw::gateway::protocol::{RpcResult, handle_rpc};
use exoclaw::gateway::server::AppState;
use exoclaw::memory::MemoryEngine;
use exoclaw::router::SessionRouter;
use exoclaw::sandbox::PluginHost;
use exoclaw::store::SessionStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Duration, timeout};

fn build_state(config: ExoclawConfig) -> Arc<AppState> {
    Arc::new(AppState {
        token: None,
        router: RwLock::new(SessionRouter::new()),
        plugins: Arc::new(RwLock::new(PluginHost::new())),
        store: RwLock::new(SessionStore::new()),
        memory: Arc::new(RwLock::new(MemoryEngine::new(
            config.memory.episodic_window as usize,
            config.memory.semantic_enabled,
        ))),
        config,
        session_locks: RwLock::new(HashMap::<String, Arc<Mutex<()>>>::new()),
    })
}

#[tokio::test]
async fn ping_returns_pong() {
    let state = build_state(ExoclawConfig::default());
    let result = handle_rpc(r#"{"id":"1","method":"ping"}"#, &state).await;
    let RpcResult::Response(resp) = result else {
        panic!("expected response");
    };
    let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["id"], "1");
    assert_eq!(parsed["result"], "pong");
}

#[tokio::test]
async fn status_returns_version_plugins_sessions() {
    let state = build_state(ExoclawConfig::default());
    let result = handle_rpc(r#"{"id":"2","method":"status"}"#, &state).await;
    let RpcResult::Response(resp) = result else {
        panic!("expected response");
    };
    let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["id"], "2");
    assert!(parsed["result"]["version"].is_string());
    assert_eq!(parsed["result"]["plugins"], 0);
    assert_eq!(parsed["result"]["sessions"], 0);
}

#[tokio::test]
async fn chat_send_missing_params_returns_error() {
    let state = build_state(ExoclawConfig::default());
    let result = handle_rpc(
        r#"{"id":"3","method":"chat.send","params":{"channel":"ws"}}"#,
        &state,
    )
    .await;
    let RpcResult::Response(resp) = result else {
        panic!("expected response");
    };
    let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["id"], "3");
    assert!(
        parsed["error"]
            .as_str()
            .unwrap_or("")
            .contains("invalid chat.send params")
    );
}

#[tokio::test]
async fn unknown_method_returns_error() {
    let state = build_state(ExoclawConfig::default());
    let result = handle_rpc(r#"{"id":"4","method":"nope.method"}"#, &state).await;
    let RpcResult::Response(resp) = result else {
        panic!("expected response");
    };
    let parsed: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["id"], "4");
    assert_eq!(parsed["error"], "unknown method: nope.method");
}

#[tokio::test]
async fn chat_send_valid_params_streams_response() {
    let mut config = ExoclawConfig::default();
    config.agent.provider = "mock".to_string();
    let state = build_state(config);

    let result = handle_rpc(
        r#"{"id":"5","method":"chat.send","params":{"channel":"websocket","account":"me","content":"hello world"}}"#,
        &state,
    )
    .await;

    let RpcResult::Stream {
        id,
        session_key,
        mut rx,
        ..
    } = result
    else {
        panic!("expected stream");
    };

    assert_eq!(id, "5");
    assert_eq!(session_key, "default:websocket:me:main");

    let mut text = String::new();
    let mut saw_usage = false;
    let mut saw_done = false;

    loop {
        let next = timeout(Duration::from_secs(5), rx.recv()).await.unwrap();
        match next {
            Some(AgentEvent::Text(chunk)) => text.push_str(&chunk),
            Some(AgentEvent::Usage { .. }) => saw_usage = true,
            Some(AgentEvent::Done) => {
                saw_done = true;
                break;
            }
            Some(AgentEvent::Error(err)) => panic!("unexpected stream error: {err}"),
            Some(_) => {}
            None => break,
        }
    }

    assert!(text.contains("mock response"));
    assert!(saw_usage);
    assert!(saw_done);
}
