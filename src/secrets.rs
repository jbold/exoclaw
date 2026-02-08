use crate::fs_util::{home_dir, set_secure_dir_permissions, set_secure_file_permissions};
use std::path::{Path, PathBuf};

fn default_state_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".exoclaw")
}

fn state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("EXOCLAW_CONFIG") {
        let config_path = PathBuf::from(path);
        if let Some(parent) = config_path.parent() {
            return parent.to_path_buf();
        }
    }
    default_state_dir()
}

fn credentials_dir_for(state_dir: &Path) -> PathBuf {
    state_dir.join("credentials")
}

fn normalize_provider(provider: &str) -> anyhow::Result<String> {
    let provider = provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "anthropic" | "openai" => Ok(provider),
        _ => anyhow::bail!("unsupported provider for key store: {provider}"),
    }
}

fn key_file_path_for(state_dir: &Path, provider: &str) -> anyhow::Result<PathBuf> {
    let provider = normalize_provider(provider)?;
    Ok(credentials_dir_for(state_dir).join(format!("{provider}.key")))
}

pub fn write_key_to(state_dir: &Path, provider: &str, api_key: &str) -> anyhow::Result<PathBuf> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    let dir = credentials_dir_for(state_dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("failed to create {}: {e}", dir.display()))?;
    set_secure_dir_permissions(&dir)?;

    let path = key_file_path_for(state_dir, provider)?;
    std::fs::write(&path, api_key)
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", path.display()))?;
    set_secure_file_permissions(&path)?;
    Ok(path)
}

pub fn read_key_from(state_dir: &Path, provider: &str) -> Option<String> {
    let path = key_file_path_for(state_dir, provider).ok()?;
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Store a provider API key in ~/.exoclaw/credentials/{provider}.key.
pub fn store_api_key(provider: &str, api_key: &str) -> anyhow::Result<PathBuf> {
    write_key_to(&state_dir(), provider, api_key)
}

/// Load a provider API key from ~/.exoclaw/credentials/{provider}.key.
pub fn load_api_key(provider: &str) -> Option<String> {
    read_key_from(&state_dir(), provider)
}

#[cfg(test)]
mod tests {
    use super::{read_key_from, write_key_to};
    use std::path::PathBuf;

    fn tmp_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("exoclaw-secrets-test-{nanos}"));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn writes_and_reads_provider_key() {
        let dir = tmp_dir();
        let path = write_key_to(&dir, "anthropic", "sk-ant-test").expect("write key");
        assert!(path.exists());
        let loaded = read_key_from(&dir, "anthropic");
        assert_eq!(loaded.as_deref(), Some("sk-ant-test"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_unknown_provider() {
        let dir = tmp_dir();
        let err = write_key_to(&dir, "bad/../../provider", "x").expect_err("should fail");
        assert!(err.to_string().contains("unsupported provider"));
        std::fs::remove_dir_all(dir).ok();
    }
}
