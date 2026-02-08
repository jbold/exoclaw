use exoclaw::config::{ExoclawConfig, save_to_path};
use exoclaw::secrets::{read_key_from, write_key_to};
use std::path::PathBuf;

fn tmp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("exoclaw-onboard-{label}-{nanos}"));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

// ---------------------------------------------------------------------------
// T010: Fresh onboard creates credential file with correct permissions
// ---------------------------------------------------------------------------

#[test]
fn write_key_creates_credential_file() {
    let dir = tmp_dir("write-key");
    let path = write_key_to(&dir, "anthropic", "sk-ant-test-123").expect("write key");

    assert!(path.exists(), "credential file should exist");
    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "sk-ant-test-123");

    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn credential_file_has_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir("perms-file");
    let path = write_key_to(&dir, "anthropic", "sk-test").expect("write key");

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "credential file should be mode 0600, got {mode:04o}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn credentials_dir_has_0700_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tmp_dir("perms-dir");
    write_key_to(&dir, "anthropic", "sk-test").expect("write key");

    let cred_dir = dir.join("credentials");
    let mode = std::fs::metadata(&cred_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o700,
        "credentials dir should be mode 0700, got {mode:04o}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_config_without_api_key_field() {
    let dir = tmp_dir("no-api-key");
    let config_path = dir.join("config.toml");

    let mut config = ExoclawConfig::default();
    config.agent.provider = "anthropic".into();
    config.agent.model = "claude-sonnet-4-5-20250929".into();
    config.agent.api_key = None;

    save_to_path(&config, &config_path).expect("save config");

    let content = std::fs::read_to_string(&config_path).unwrap();
    // The TOML should not contain an api_key field (None is omitted by serde)
    assert!(
        !content.contains("api_key"),
        "config file should not contain api_key field, got:\n{content}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// T012: Re-onboard preserves unrelated config sections
// ---------------------------------------------------------------------------

#[test]
fn re_onboard_preserves_unrelated_sections() {
    let dir = tmp_dir("re-onboard");
    let config_path = dir.join("config.toml");

    // Write an initial config with plugins, bindings, budgets, memory, gateway
    let initial_toml = r#"
[gateway]
port = 9999
bind = "0.0.0.0"

[agent]
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
max_tokens = 4096

[[plugins]]
name = "echo"
path = "/opt/plugins/echo.wasm"
capabilities = ["http:api.example.com"]

[[bindings]]
agent_id = "my-agent"
channel = "telegram"

[budgets]
session = 50000
daily = 500000
monthly = 5000000

[memory]
episodic_window = 10
semantic_enabled = false
"#;
    std::fs::write(&config_path, initial_toml).unwrap();

    // Load the config (simulating what run_onboard does)
    let mut config: ExoclawConfig = toml::from_str(initial_toml).unwrap();

    // Simulate onboard: only change provider and model, clear api_key
    config.agent.provider = "openai".into();
    config.agent.model = "gpt-4o".into();
    config.agent.api_key = None;

    save_to_path(&config, &config_path).expect("save updated config");

    // Re-load and verify unrelated sections are preserved
    let reloaded: ExoclawConfig =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();

    // Agent section updated
    assert_eq!(reloaded.agent.provider, "openai");
    assert_eq!(reloaded.agent.model, "gpt-4o");
    assert!(reloaded.agent.api_key.is_none());

    // Gateway preserved
    assert_eq!(reloaded.gateway.port, 9999);
    assert_eq!(reloaded.gateway.bind, "0.0.0.0");

    // Plugins preserved
    assert_eq!(reloaded.plugins.len(), 1);
    assert_eq!(reloaded.plugins[0].name, "echo");
    assert_eq!(reloaded.plugins[0].path, "/opt/plugins/echo.wasm");
    assert_eq!(
        reloaded.plugins[0].capabilities,
        vec!["http:api.example.com"]
    );

    // Bindings preserved
    assert_eq!(reloaded.bindings.len(), 1);
    assert_eq!(reloaded.bindings[0].agent_id, "my-agent");
    assert_eq!(reloaded.bindings[0].channel.as_deref(), Some("telegram"));

    // Budgets preserved
    assert_eq!(reloaded.budgets.session, Some(50000));
    assert_eq!(reloaded.budgets.daily, Some(500000));
    assert_eq!(reloaded.budgets.monthly, Some(5000000));

    // Memory preserved
    assert_eq!(reloaded.memory.episodic_window, 10);
    assert!(!reloaded.memory.semantic_enabled);

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// T011: Credential resolution tests
// ---------------------------------------------------------------------------

#[test]
fn env_var_takes_precedence_over_credential_file() {
    let dir = tmp_dir("env-precedence");
    let config_path = dir.join("config.toml");

    // Write a credential file
    write_key_to(&dir, "anthropic", "sk-from-file").expect("write key");

    // Write config pointing to this dir
    let config_toml = "[agent]\nprovider = \"anthropic\"\n";
    std::fs::write(&config_path, config_toml).unwrap();

    // Set env vars: EXOCLAW_CONFIG points to our temp config, and set API key env var
    // SAFETY: env var manipulation for testing
    unsafe {
        std::env::set_var("EXOCLAW_CONFIG", config_path.to_str().unwrap());
        std::env::set_var("ANTHROPIC_API_KEY", "sk-from-env");
    }

    let config = exoclaw::config::load().expect("load config");

    unsafe {
        std::env::remove_var("EXOCLAW_CONFIG");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    // Env var should win
    assert_eq!(config.agent.api_key.as_deref(), Some("sk-from-env"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn credential_file_used_when_env_var_absent() {
    // Test read_key_from directly to avoid env var race conditions in parallel tests.
    // The full config::load() -> resolve_api_key() -> load_api_key() chain delegates
    // to read_key_from internally; testing it directly proves the fallback works.
    let dir = tmp_dir("cred-file-fallback");

    write_key_to(&dir, "openai", "sk-from-cred-file").expect("write key");

    let loaded = read_key_from(&dir, "openai");
    assert_eq!(loaded.as_deref(), Some("sk-from-cred-file"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn empty_credential_file_treated_as_missing() {
    let dir = tmp_dir("empty-cred");

    // Write an empty credential file
    let cred_dir = dir.join("credentials");
    std::fs::create_dir_all(&cred_dir).unwrap();
    std::fs::write(cred_dir.join("anthropic.key"), "").unwrap();

    let result = read_key_from(&dir, "anthropic");
    assert!(result.is_none(), "empty credential file should return None");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn whitespace_only_credential_file_treated_as_missing() {
    let dir = tmp_dir("ws-cred");

    let cred_dir = dir.join("credentials");
    std::fs::create_dir_all(&cred_dir).unwrap();
    std::fs::write(cred_dir.join("anthropic.key"), "   \n  ").unwrap();

    let result = read_key_from(&dir, "anthropic");
    assert!(
        result.is_none(),
        "whitespace-only credential file should return None"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn invalid_provider_rejected_by_write() {
    let dir = tmp_dir("bad-provider");
    let err = write_key_to(&dir, "deepmind", "sk-test").expect_err("should reject");
    assert!(
        err.to_string().contains("unsupported provider"),
        "error should mention unsupported provider: {err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn invalid_provider_rejected_by_config_save() {
    let dir = tmp_dir("bad-provider-cfg");
    let config_path = dir.join("config.toml");

    let mut config = ExoclawConfig::default();
    config.agent.provider = "deepmind".into();

    let err = save_to_path(&config, &config_path).expect_err("should reject invalid provider");
    assert!(
        err.to_string().contains("invalid provider"),
        "error should mention invalid provider: {err}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn empty_api_key_rejected_by_write() {
    let dir = tmp_dir("empty-key");
    let err = write_key_to(&dir, "anthropic", "").expect_err("should reject empty key");
    assert!(
        err.to_string().contains("cannot be empty"),
        "error should mention empty: {err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_key_roundtrip_openai() {
    let dir = tmp_dir("openai-rt");

    write_key_to(&dir, "openai", "sk-openai-test-key").expect("write key");
    let loaded = read_key_from(&dir, "openai");
    assert_eq!(loaded.as_deref(), Some("sk-openai-test-key"));

    // Anthropic key should not exist
    let other = read_key_from(&dir, "anthropic");
    assert!(other.is_none());

    std::fs::remove_dir_all(&dir).ok();
}
