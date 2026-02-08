use serde::Deserialize;
use std::path::PathBuf;
use tracing::info;

/// Top-level configuration loaded from TOML.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ExoclawConfig {
    pub gateway: GatewayConfig,
    pub agent: AgentDefConfig,
    #[serde(default)]
    pub plugins: Vec<PluginConfig>,
    #[serde(default)]
    pub bindings: Vec<BindingConfig>,
    #[serde(default)]
    pub budgets: BudgetConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
}

#[derive(Debug, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            bind: default_bind(),
        }
    }
}

fn default_port() -> u16 {
    7200
}
fn default_bind() -> String {
    "127.0.0.1".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefConfig {
    #[serde(default = "default_agent_id")]
    pub id: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub api_key: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    pub system_prompt: Option<String>,
    pub soul_path: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub fallback: Option<Box<AgentDefConfig>>,
}

impl Default for AgentDefConfig {
    fn default() -> Self {
        Self {
            id: default_agent_id(),
            provider: default_provider(),
            model: default_model(),
            api_key: None,
            max_tokens: default_max_tokens(),
            system_prompt: None,
            soul_path: None,
            tools: Vec::new(),
            fallback: None,
        }
    }
}

fn default_agent_id() -> String {
    "default".into()
}
fn default_provider() -> String {
    "anthropic".into()
}
fn default_model() -> String {
    "claude-sonnet-4-5-20250929".into()
}
fn default_max_tokens() -> u32 {
    4096
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BindingConfig {
    pub agent_id: String,
    pub channel: Option<String>,
    pub account_id: Option<String>,
    pub peer_id: Option<String>,
    pub guild_id: Option<String>,
    pub team_id: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct BudgetConfig {
    pub session: Option<u64>,
    pub daily: Option<u64>,
    pub monthly: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_episodic_window")]
    pub episodic_window: u32,
    #[serde(default = "default_semantic_enabled")]
    pub semantic_enabled: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            episodic_window: default_episodic_window(),
            semantic_enabled: default_semantic_enabled(),
        }
    }
}

fn default_episodic_window() -> u32 {
    5
}
fn default_semantic_enabled() -> bool {
    true
}

/// Load configuration from file or use defaults.
///
/// Search order:
/// 1. `EXOCLAW_CONFIG` env var
/// 2. `~/.exoclaw/config.toml`
/// 3. Zero-config defaults (no file needed)
pub fn load() -> anyhow::Result<ExoclawConfig> {
    let path = config_path();

    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
        let mut config: ExoclawConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("invalid config at {}: {e}", path.display()))?;

        resolve_api_key(&mut config);
        validate(&config)?;

        info!("loaded config from {}", path.display());
        Ok(config)
    } else {
        info!("no config file found, using zero-config defaults");
        let mut config = ExoclawConfig::default();
        resolve_api_key(&mut config);
        Ok(config)
    }
}

fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("EXOCLAW_CONFIG") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".exoclaw").join("config.toml")
}

/// Resolve API key from environment variables if not set in config.
fn resolve_api_key(config: &mut ExoclawConfig) {
    if config.agent.api_key.is_none() {
        config.agent.api_key = match config.agent.provider.as_str() {
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
            "openai" => std::env::var("OPENAI_API_KEY").ok(),
            _ => None,
        };
    }
}

/// Validate the config and return clear error messages.
fn validate(config: &ExoclawConfig) -> anyhow::Result<()> {
    let valid_providers = ["anthropic", "openai"];
    if !valid_providers.contains(&config.agent.provider.as_str()) {
        anyhow::bail!(
            "invalid provider '{}': must be one of {:?}",
            config.agent.provider,
            valid_providers
        );
    }

    if config.agent.max_tokens == 0 {
        anyhow::bail!("agent.max_tokens must be > 0");
    }

    for (i, binding) in config.bindings.iter().enumerate() {
        if binding.channel.is_none()
            && binding.account_id.is_none()
            && binding.peer_id.is_none()
            && binding.guild_id.is_none()
            && binding.team_id.is_none()
        {
            anyhow::bail!(
                "binding[{i}] must have at least one of: channel, account_id, peer_id, guild_id, team_id"
            );
        }
    }

    Ok(())
}
