use extism_pdk::*;
use serde::{Deserialize, Serialize};

/// Incoming message from the exoclaw host.
#[derive(Deserialize)]
struct IncomingMessage {
    text: String,
    channel: String,
    peer_id: String,
}

/// Response sent back to the host.
#[derive(Serialize)]
struct OutgoingMessage {
    text: String,
}

/// Main entry point called by the exoclaw plugin host.
///
/// Receives a JSON-encoded IncomingMessage and returns a JSON-encoded
/// OutgoingMessage that echoes the input back.
///
/// Build with:
///   cargo build --target wasm32-unknown-unknown --release
///   cp target/wasm32-unknown-unknown/release/echo_plugin.wasm plugins/echo.wasm
#[plugin_fn]
pub fn handle_message(input: String) -> FnResult<String> {
    let msg: IncomingMessage = serde_json::from_str(&input)
        .map_err(|e| Error::msg(format!("bad input: {e}")))?;

    let response = OutgoingMessage {
        text: format!(
            "echo from {}/{}: {}",
            msg.channel, msg.peer_id, msg.text
        ),
    };

    let output = serde_json::to_string(&response)
        .map_err(|e| Error::msg(format!("serialize failed: {e}")))?;

    Ok(output)
}
