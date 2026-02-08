use exoclaw::sandbox::PluginHost;
use exoclaw::sandbox::capabilities::{self, Capability};

/// Path to the echo plugin WASM binary.
/// Built via: cd examples/echo-plugin && cargo build --target wasm32-unknown-unknown --release
fn echo_wasm_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!(
        "{manifest_dir}/examples/echo-plugin/target/wasm32-unknown-unknown/release/echo_plugin.wasm"
    )
}

#[test]
fn register_and_call_echo_plugin() {
    let mut host = PluginHost::new();
    host.register("echo", &echo_wasm_path(), vec![]).unwrap();

    assert_eq!(host.count(), 1);
    assert!(host.has_plugin("echo"));

    // Call handle_tool_call
    let input = serde_json::json!({"message": "hello world"});
    let result = host.call_tool("echo", &input);

    assert!(!result.is_error);
    assert!(
        result.content.contains("hello world"),
        "got: {}",
        result.content
    );
}

#[test]
fn register_invalid_wasm_rejected() {
    let mut host = PluginHost::new();
    let result = host.register("bad", "/tmp/nonexistent.wasm", vec![]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn register_invalid_binary_rejected() {
    // Create a temp file with invalid WASM content
    let tmp_path = "/tmp/exoclaw_test_bad_wasm.wasm";
    std::fs::write(tmp_path, b"this is not wasm").unwrap();

    let mut host = PluginHost::new();
    let result = host.register("bad", tmp_path, vec![]);
    assert!(result.is_err(), "should reject invalid WASM binary");

    std::fs::remove_file(tmp_path).ok();
}

#[test]
fn call_nonexistent_plugin_returns_error() {
    let host = PluginHost::new();
    let input = serde_json::json!({"message": "test"});
    let result = host.call_tool("nonexistent", &input);
    assert!(result.is_error);
    assert!(
        result.content.contains("plugin not found") || result.content.contains("not found"),
        "got: {}",
        result.content,
    );
}

#[test]
fn fresh_instance_per_invocation() {
    // Verify that calling the same plugin twice gives independent results
    // (no state leakage between calls)
    let mut host = PluginHost::new();
    host.register("echo", &echo_wasm_path(), vec![]).unwrap();

    let input1 = serde_json::json!({"message": "first"});
    let result1 = host.call_tool("echo", &input1);
    assert!(!result1.is_error);
    assert!(result1.content.contains("first"));

    let input2 = serde_json::json!({"message": "second"});
    let result2 = host.call_tool("echo", &input2);
    assert!(!result2.is_error);
    assert!(result2.content.contains("second"));
    assert!(
        !result2.content.contains("first"),
        "state leaked between calls"
    );
}

#[test]
fn plugin_describes_tool_schema() {
    let mut host = PluginHost::new();
    host.register("echo", &echo_wasm_path(), vec![]).unwrap();

    let schema = host.tool_schema("echo");
    assert!(schema.is_some(), "echo plugin should have a tool schema");

    let schema = schema.unwrap();
    assert_eq!(schema.get("name").and_then(|n| n.as_str()), Some("echo"));
    assert!(schema.get("description").is_some());
    assert!(schema.get("input_schema").is_some());
}

#[test]
fn tool_schemas_returns_all_tool_plugins() {
    let mut host = PluginHost::new();
    host.register("echo", &echo_wasm_path(), vec![]).unwrap();

    let schemas = host.tool_schemas();
    assert_eq!(schemas.len(), 1);
    assert_eq!(
        schemas[0].get("name").and_then(|n| n.as_str()),
        Some("echo")
    );
}

#[test]
fn register_with_capabilities() {
    let mut host = PluginHost::new();
    let caps = vec![
        Capability::Http("api.example.com".into()),
        Capability::Store("sessions".into()),
    ];
    host.register("echo", &echo_wasm_path(), caps).unwrap();
    assert!(host.has_plugin("echo"));
}

#[test]
fn capability_parsing() {
    let cap = capabilities::parse("http:api.telegram.org").unwrap();
    assert_eq!(cap, Capability::Http("api.telegram.org".into()));

    let cap = capabilities::parse("store:sessions").unwrap();
    assert_eq!(cap, Capability::Store("sessions".into()));

    let cap = capabilities::parse("host_function:my_func").unwrap();
    assert_eq!(cap, Capability::HostFunction("my_func".into()));

    // Invalid formats
    assert!(capabilities::parse("bad").is_err());
    assert!(capabilities::parse("http:").is_err());
    assert!(capabilities::parse("unknown:val").is_err());
}

#[test]
fn capability_parse_all() {
    let caps =
        capabilities::parse_all(&["http:api.example.com".into(), "store:data".into()]).unwrap();
    assert_eq!(caps.len(), 2);

    // Fails on first invalid
    let result = capabilities::parse_all(&["http:ok".into(), "bad".into()]);
    assert!(result.is_err());
}

#[test]
fn allowed_hosts_from_capabilities() {
    let caps = vec![
        Capability::Http("api.example.com".into()),
        Capability::Store("sessions".into()),
        Capability::Http("api.other.com".into()),
    ];
    let hosts = capabilities::allowed_hosts(&caps);
    assert_eq!(hosts, vec!["api.example.com", "api.other.com"]);
}

#[test]
fn list_plugins() {
    let mut host = PluginHost::new();
    assert!(host.list().is_empty());

    host.register("echo", &echo_wasm_path(), vec![]).unwrap();
    let list = host.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "echo");
}

// Test the tool schema format builders
#[test]
fn build_anthropic_tool_format() {
    let schemas = vec![serde_json::json!({
        "name": "echo",
        "description": "Echoes input",
        "input_schema": {
            "type": "object",
            "properties": {
                "message": {"type": "string"}
            }
        }
    })];

    let tools = exoclaw::agent::providers::build_anthropic_tools(&schemas);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "echo");
    assert_eq!(tools[0]["description"], "Echoes input");
    assert!(tools[0].get("input_schema").is_some());
}

#[test]
fn build_openai_tool_format() {
    let schemas = vec![serde_json::json!({
        "name": "echo",
        "description": "Echoes input",
        "input_schema": {
            "type": "object",
            "properties": {
                "message": {"type": "string"}
            }
        }
    })];

    let tools = exoclaw::agent::providers::build_openai_tools(&schemas);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["function"]["name"], "echo");
    assert_eq!(tools[0]["function"]["description"], "Echoes input");
    assert!(tools[0]["function"].get("parameters").is_some());
}
