use extism::{Manifest, Plugin, Wasm};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// WASM plugin host — loads and manages sandboxed plugin modules.
///
/// Each plugin runs in its own WASM sandbox with explicit capability grants.
/// A plugin cannot access the filesystem, network, or host memory unless
/// the host explicitly provides those capabilities.
pub struct PluginHost {
    plugins: HashMap<String, PluginEntry>,
}

struct PluginEntry {
    name: String,
    manifest: Manifest,
    // Plugin instances are created per-invocation for isolation
}

#[derive(Serialize)]
pub struct PluginInfo {
    pub name: String,
}

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

    /// Register a WASM plugin from a file path.
    pub fn register(&mut self, name: &str, wasm_path: &str) -> anyhow::Result<()> {
        let path = Path::new(wasm_path);
        anyhow::ensure!(path.exists(), "plugin file not found: {wasm_path}");

        let wasm = Wasm::file(path);
        let manifest = Manifest::new([wasm]);

        // Validate by attempting to instantiate
        let _plugin = Plugin::new(manifest.clone(), [], true)?;

        self.plugins.insert(
            name.into(),
            PluginEntry {
                name: name.into(),
                manifest,
            },
        );

        info!("plugin loaded: {name} ({wasm_path})");
        Ok(())
    }

    /// Call a function on a loaded plugin.
    pub fn call(&self, plugin_name: &str, function: &str, input: &[u8]) -> anyhow::Result<Vec<u8>> {
        let entry = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {plugin_name}"))?;

        let mut plugin = Plugin::new(entry.manifest.clone(), [], true)?;
        let output = plugin.call::<&[u8], Vec<u8>>(function, input)?;
        Ok(output)
    }
}

/// CLI entrypoint for loading a plugin.
pub async fn load_plugin(path: &str) -> anyhow::Result<()> {
    let name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let mut host = PluginHost::new();
    host.register(name, path)?;
    println!("plugin '{name}' loaded successfully from {path}");
    Ok(())
}
