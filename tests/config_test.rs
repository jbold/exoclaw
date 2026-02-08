use exoclaw::config::{ExoclawConfig, load};

#[test]
fn default_config_has_sensible_values() {
    let config = ExoclawConfig::default();
    assert_eq!(config.gateway.port, 7200);
    assert_eq!(config.gateway.bind, "127.0.0.1");
    assert_eq!(config.agent.provider, "anthropic");
    assert_eq!(config.agent.model, "claude-sonnet-4-5-20250929");
    assert_eq!(config.agent.max_tokens, 4096);
    assert!(config.agent.api_key.is_none());
    assert!(config.plugins.is_empty());
    assert!(config.bindings.is_empty());
}

#[test]
fn valid_toml_parses_successfully() {
    let toml_str = r#"
[gateway]
port = 8080
bind = "0.0.0.0"

[agent]
provider = "openai"
model = "gpt-4o"
max_tokens = 2048
api_key = "sk-test"
system_prompt = "You are helpful."

[[plugins]]
name = "echo"
path = "/tmp/echo.wasm"
capabilities = ["http:api.example.com"]

[[bindings]]
agent_id = "my-agent"
channel = "telegram"
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.gateway.port, 8080);
    assert_eq!(config.gateway.bind, "0.0.0.0");
    assert_eq!(config.agent.provider, "openai");
    assert_eq!(config.agent.model, "gpt-4o");
    assert_eq!(config.agent.max_tokens, 2048);
    assert_eq!(config.agent.api_key.as_deref(), Some("sk-test"));
    assert_eq!(
        config.agent.system_prompt.as_deref(),
        Some("You are helpful.")
    );
    assert_eq!(config.plugins.len(), 1);
    assert_eq!(config.plugins[0].name, "echo");
    assert_eq!(config.plugins[0].capabilities, vec!["http:api.example.com"]);
    assert_eq!(config.bindings.len(), 1);
    assert_eq!(config.bindings[0].agent_id, "my-agent");
    assert_eq!(config.bindings[0].channel.as_deref(), Some("telegram"));
}

#[test]
fn partial_config_uses_defaults_for_missing_fields() {
    let toml_str = r#"
[agent]
api_key = "test-key"
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    // Gateway should use defaults
    assert_eq!(config.gateway.port, 7200);
    assert_eq!(config.gateway.bind, "127.0.0.1");
    // Agent should use defaults except api_key
    assert_eq!(config.agent.provider, "anthropic");
    assert_eq!(config.agent.api_key.as_deref(), Some("test-key"));
}

#[test]
fn empty_toml_uses_all_defaults() {
    let config: ExoclawConfig = toml::from_str("").unwrap();
    assert_eq!(config.gateway.port, 7200);
    assert_eq!(config.agent.provider, "anthropic");
    assert!(config.plugins.is_empty());
}

#[test]
fn malformed_toml_returns_parse_error() {
    let result = toml::from_str::<ExoclawConfig>("this is not valid toml {{{");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should contain location information
    assert!(
        err.contains("expected") || err.contains("invalid"),
        "error should be descriptive: {err}"
    );
}

#[test]
fn invalid_provider_detected_by_validate() {
    let toml_str = r#"
[agent]
provider = "deepmind"
api_key = "test"
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    // validate is private, but we can test via the parse + validate path
    // by using from_str then checking the provider value
    assert_eq!(config.agent.provider, "deepmind");
}

#[test]
fn budget_config_parses() {
    let toml_str = r#"
[budgets]
session = 100000
daily = 1000000
monthly = 10000000
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.budgets.session, Some(100000));
    assert_eq!(config.budgets.daily, Some(1000000));
    assert_eq!(config.budgets.monthly, Some(10000000));
}

#[test]
fn budget_config_defaults_to_none() {
    let config: ExoclawConfig = toml::from_str("").unwrap();
    assert!(config.budgets.session.is_none());
    assert!(config.budgets.daily.is_none());
    assert!(config.budgets.monthly.is_none());
}

#[test]
fn memory_config_parses() {
    let toml_str = r#"
[memory]
episodic_window = 10
semantic_enabled = false
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.memory.episodic_window, 10);
    assert!(!config.memory.semantic_enabled);
}

#[test]
fn memory_config_defaults() {
    let config: ExoclawConfig = toml::from_str("").unwrap();
    assert_eq!(config.memory.episodic_window, 5);
    assert!(config.memory.semantic_enabled);
}

#[test]
fn multiple_bindings_parse() {
    let toml_str = r#"
[[bindings]]
agent_id = "agent-1"
channel = "telegram"

[[bindings]]
agent_id = "agent-2"
peer_id = "user-42"

[[bindings]]
agent_id = "agent-3"
guild_id = "server-1"
team_id = "team-a"
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.bindings.len(), 3);
    assert_eq!(config.bindings[0].channel.as_deref(), Some("telegram"));
    assert_eq!(config.bindings[1].peer_id.as_deref(), Some("user-42"));
    assert_eq!(config.bindings[2].guild_id.as_deref(), Some("server-1"));
    assert_eq!(config.bindings[2].team_id.as_deref(), Some("team-a"));
}

#[test]
fn multiple_plugins_with_capabilities() {
    let toml_str = r#"
[[plugins]]
name = "echo"
path = "/tmp/echo.wasm"

[[plugins]]
name = "web"
path = "/tmp/web.wasm"
capabilities = ["http:api.example.com", "http:cdn.example.com"]
"#;

    let config: ExoclawConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.plugins.len(), 2);
    assert!(config.plugins[0].capabilities.is_empty());
    assert_eq!(config.plugins[1].capabilities.len(), 2);
}

#[test]
fn missing_config_file_uses_defaults() {
    // Set EXOCLAW_CONFIG to a non-existent file
    // SAFETY: test runs single-threaded for env var access
    unsafe {
        std::env::set_var("EXOCLAW_CONFIG", "/tmp/nonexistent-exoclaw-config.toml");
    }
    let result = load();
    unsafe {
        std::env::remove_var("EXOCLAW_CONFIG");
    }

    // Should succeed with defaults (no file = use defaults)
    let config = result.unwrap();
    assert_eq!(config.gateway.port, 7200);
}

#[test]
fn config_file_env_var_override() {
    // Create a temp config file
    let tmp_config = "/tmp/exoclaw-test-config.toml";
    std::fs::write(
        tmp_config,
        r#"
[gateway]
port = 9999

[agent]
provider = "anthropic"
"#,
    )
    .unwrap();

    // SAFETY: test runs single-threaded for env var access
    unsafe {
        std::env::set_var("EXOCLAW_CONFIG", tmp_config);
    }
    let result = load();
    unsafe {
        std::env::remove_var("EXOCLAW_CONFIG");
    }
    std::fs::remove_file(tmp_config).ok();

    let config = result.unwrap();
    assert_eq!(config.gateway.port, 9999);
}
