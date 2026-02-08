pub mod capabilities;

use extism::{Manifest, Plugin, Wasm};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tracing::info;

use capabilities::Capability;

/// WASM plugin host — loads and manages sandboxed plugin modules.
///
/// Each plugin runs in its own WASM sandbox with explicit capability grants.
/// A plugin cannot access the filesystem, network, or host memory unless
/// the host explicitly provides those capabilities.
pub struct PluginHost {
    plugins: HashMap<String, PluginEntry>,
}

/// Whether a plugin is a tool (handle_tool_call) or a channel adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginType {
    Tool,
    ChannelAdapter,
}

struct PluginEntry {
    name: String,
    manifest: Manifest,
    plugin_type: PluginType,
    capabilities: Vec<Capability>,
    /// Tool schema from the plugin's `describe()` export, if available.
    tool_schema: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct PluginInfo {
    pub name: String,
}

/// Result of a tool call invocation.
#[derive(Debug)]
pub struct ToolCallResult {
    pub content: String,
    pub is_error: bool,
}

/// Default execution timeout for plugin calls.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    pub fn list(&self) -> Vec<PluginInfo> {
        self.plugins
            .values()
            .map(|p| PluginInfo {
                name: p.name.clone(),
            })
            .collect()
    }

    /// Check if a plugin exists by name.
    pub fn has_plugin(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Get the tool schema for a plugin, if available.
    pub fn tool_schema(&self, name: &str) -> Option<&serde_json::Value> {
        self.plugins.get(name).and_then(|p| p.tool_schema.as_ref())
    }

    /// Get all tool schemas for building LLM request tool lists.
    pub fn tool_schemas(&self) -> Vec<serde_json::Value> {
        self.plugins
            .values()
            .filter(|p| p.plugin_type == PluginType::Tool && p.tool_schema.is_some())
            .filter_map(|p| p.tool_schema.clone())
            .collect()
    }

    /// Register a WASM plugin from a file path with capabilities.
    pub fn register(
        &mut self,
        name: &str,
        wasm_path: &str,
        caps: Vec<Capability>,
    ) -> anyhow::Result<()> {
        let path = Path::new(wasm_path);
        anyhow::ensure!(path.exists(), "plugin file not found: {wasm_path}");

        let wasm = Wasm::file(path);
        let mut manifest = Manifest::new([wasm]);

        // Apply HTTP capabilities as allowed_hosts
        let hosts = capabilities::allowed_hosts(&caps);
        if !hosts.is_empty() {
            manifest = manifest.with_allowed_hosts(hosts.into_iter());
        }

        // Set timeout on the manifest
        manifest = manifest.with_timeout(DEFAULT_TIMEOUT);

        // Validate by attempting to instantiate
        let mut plugin = Plugin::new(manifest.clone(), [], true)?;

        // Detect plugin type and extract tool schema
        let (plugin_type, tool_schema) = detect_plugin_type(&mut plugin);

        self.plugins.insert(
            name.into(),
            PluginEntry {
                name: name.into(),
                manifest,
                plugin_type,
                capabilities: caps,
                tool_schema,
            },
        );

        info!("plugin loaded: {name} ({wasm_path})");
        Ok(())
    }

    /// Call a function on a loaded plugin.
    ///
    /// Creates a fresh Plugin instance per invocation for isolation (no shared
    /// state between calls). Catches WASM traps and converts to error results.
    pub fn call(&self, plugin_name: &str, function: &str, input: &[u8]) -> anyhow::Result<Vec<u8>> {
        let entry = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {plugin_name}"))?;

        // Fresh instance per invocation for isolation
        let mut plugin = Plugin::new(entry.manifest.clone(), [], true)?;
        let output = plugin.call::<&[u8], Vec<u8>>(function, input)?;
        Ok(output)
    }

    /// Call a tool plugin's `handle_tool_call` with JSON input and return a structured result.
    ///
    /// Creates a fresh Plugin instance per invocation for isolation. Catches WASM
    /// traps and converts to error results without crashing the host.
    pub fn call_tool(&self, plugin_name: &str, input: &serde_json::Value) -> ToolCallResult {
        let input_bytes = match serde_json::to_vec(input) {
            Ok(b) => b,
            Err(e) => {
                return ToolCallResult {
                    content: format!("failed to serialize tool input: {e}"),
                    is_error: true,
                };
            }
        };

        let output = match self.call(plugin_name, "handle_tool_call", &input_bytes) {
            Ok(bytes) => bytes,
            Err(e) => {
                return ToolCallResult {
                    content: format!("tool execution failed: {e}"),
                    is_error: true,
                };
            }
        };

        // Try to parse as structured ToolResult JSON
        match serde_json::from_slice::<serde_json::Value>(&output) {
            Ok(v) => {
                let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);

                if content.is_empty() {
                    // Use full output as content
                    ToolCallResult {
                        content: String::from_utf8_lossy(&output).to_string(),
                        is_error,
                    }
                } else {
                    ToolCallResult {
                        content: content.to_string(),
                        is_error,
                    }
                }
            }
            Err(_) => {
                // Not JSON — return raw output as content
                ToolCallResult {
                    content: String::from_utf8_lossy(&output).to_string(),
                    is_error: false,
                }
            }
        }
    }

    /// Call a channel adapter's `parse_incoming` to convert platform payload to normalized message.
    ///
    /// Returns JSON with at minimum `{ "content": "...", "account": "...", "peer": "..." }`.
    /// Creates a fresh Plugin instance per invocation for isolation.
    pub fn call_channel_parse(
        &self,
        plugin_name: &str,
        payload: &[u8],
    ) -> anyhow::Result<serde_json::Value> {
        let output = self.call(plugin_name, "parse_incoming", payload)?;
        let parsed: serde_json::Value = serde_json::from_slice(&output)
            .map_err(|e| anyhow::anyhow!("channel adapter returned invalid JSON: {e}"))?;
        Ok(parsed)
    }

    /// Call a channel adapter's `format_outgoing` to convert normalized response to platform format.
    ///
    /// Takes the agent response text and returns the platform-specific payload bytes.
    /// Creates a fresh Plugin instance per invocation for isolation.
    pub fn call_channel_format(
        &self,
        plugin_name: &str,
        response: &serde_json::Value,
    ) -> anyhow::Result<Vec<u8>> {
        let input = serde_json::to_vec(response)?;
        self.call(plugin_name, "format_outgoing", &input)
    }

    /// Get the plugin type for a named plugin.
    pub fn plugin_type(&self, name: &str) -> Option<&PluginType> {
        self.plugins.get(name).map(|p| &p.plugin_type)
    }

    /// Find a channel adapter plugin by channel name.
    ///
    /// Looks for a plugin with `PluginType::ChannelAdapter` whose name matches the channel.
    pub fn find_channel_adapter(&self, channel: &str) -> Option<&str> {
        self.plugins
            .iter()
            .find(|(_, entry)| {
                entry.plugin_type == PluginType::ChannelAdapter && entry.name == channel
            })
            .map(|(name, _)| name.as_str())
    }

    /// Get the allowed HTTP hosts for a plugin (from its capabilities).
    pub fn allowed_hosts(&self, plugin_name: &str) -> Vec<String> {
        self.plugins
            .get(plugin_name)
            .map(|entry| capabilities::allowed_hosts(&entry.capabilities))
            .unwrap_or_default()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect whether a plugin is a Tool or ChannelAdapter, and extract its tool schema.
fn detect_plugin_type(plugin: &mut Plugin) -> (PluginType, Option<serde_json::Value>) {
    // Check if the plugin has a `describe()` export
    if let Ok(output) = plugin.call::<&[u8], Vec<u8>>("describe", b"{}") {
        if let Ok(schema) = serde_json::from_slice::<serde_json::Value>(&output) {
            let declared_type = schema
                .get("type")
                .and_then(|v| v.as_str())
                .or_else(|| schema.get("plugin_type").and_then(|v| v.as_str()));

            if matches!(declared_type, Some("channel_adapter")) {
                return (PluginType::ChannelAdapter, None);
            }

            return (PluginType::Tool, Some(schema));
        }
    }

    // Fall back to function probing when describe() is unavailable.
    if plugin
        .call::<&[u8], Vec<u8>>("parse_incoming", b"{}")
        .is_ok()
        || plugin
            .call::<&[u8], Vec<u8>>("format_outgoing", br#"{"content":"ok"}"#)
            .is_ok()
    {
        return (PluginType::ChannelAdapter, None);
    }

    if plugin
        .call::<&[u8], Vec<u8>>("handle_tool_call", b"{}")
        .is_ok()
    {
        return (PluginType::Tool, None);
    }

    // Default to Tool type
    (PluginType::Tool, None)
}

/// CLI entrypoint for loading a plugin.
pub async fn load_plugin(path: &str) -> anyhow::Result<()> {
    let name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let mut host = PluginHost::new();
    host.register(name, path, vec![])?;
    println!("plugin '{name}' loaded successfully from {path}");
    Ok(())
}
