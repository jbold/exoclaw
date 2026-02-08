use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// A loaded soul document (agent personality/instructions).
#[derive(Debug, Clone)]
pub struct Soul {
    pub agent_id: String,
    pub content: String,
    pub token_count: u32,
    pub loaded_from: String,
    pub loaded_at: DateTime<Utc>,
    file_mtime: Option<std::time::SystemTime>,
}

/// Loads and caches soul documents from the filesystem.
/// Supports hot-reload by checking file mtime on access.
pub struct SoulLoader {
    souls: HashMap<String, Soul>,
}

impl Default for SoulLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl SoulLoader {
    pub fn new() -> Self {
        Self {
            souls: HashMap::new(),
        }
    }

    /// Load a soul document from a file path for the given agent.
    pub fn load(&mut self, agent_id: &str, path: &str) -> anyhow::Result<&Soul> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read soul file {path}: {e}"))?;

        let mtime = Path::new(path)
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok());
        let token_count = estimate_tokens(&content);

        let soul = Soul {
            agent_id: agent_id.to_string(),
            content,
            token_count,
            loaded_from: path.to_string(),
            loaded_at: Utc::now(),
            file_mtime: mtime,
        };

        info!(agent_id, path, token_count, "loaded soul document");

        self.souls.insert(agent_id.to_string(), soul);
        Ok(self.souls.get(agent_id).unwrap())
    }

    /// Get the soul document for an agent, hot-reloading if the file changed.
    pub fn get(&mut self, agent_id: &str) -> Option<&Soul> {
        // Check if reload is needed
        let needs_reload = self.souls.get(agent_id).and_then(|soul| {
            let current_mtime = Path::new(&soul.loaded_from)
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok());

            match (soul.file_mtime, current_mtime) {
                (Some(old), Some(new)) if new > old => Some(soul.loaded_from.clone()),
                _ => None,
            }
        });

        if let Some(path) = needs_reload {
            info!(agent_id, path = %path, "hot-reloading soul document");
            // Reload (ignore errors, keep old version)
            let _ = self.load(agent_id, &path);
        }

        self.souls.get(agent_id)
    }

    /// Get the soul content string for an agent.
    pub fn get_content(&mut self, agent_id: &str) -> Option<String> {
        self.get(agent_id).map(|s| s.content.clone())
    }
}

/// Estimate token count using a simple heuristic: ~4 chars per token.
/// This matches the rough BPE average for English text.
fn estimate_tokens(text: &str) -> u32 {
    (text.len() as f64 / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello"), 2); // 5 / 4 = 1.25 -> 2
        // ~500 tokens for a 2000-char doc
        let doc = "a".repeat(2000);
        assert_eq!(estimate_tokens(&doc), 500);
    }
}
