# Quickstart: Exoclaw Core Runtime

**Branch**: `001-core-runtime` | **Date**: 2026-02-08

## Prerequisites

- Rust 1.85+ (edition 2024)
- An Anthropic or OpenAI API key

## 1. Build

```bash
git clone https://github.com/youruser/exoclaw.git
cd exoclaw
cargo build --release
```

The release binary is at `target/release/exoclaw` (~21MB, statically linked).

## 2. Configure

Create `~/.exoclaw/config.toml`:

```toml
[gateway]
port = 7200
bind = "127.0.0.1"

[agent]
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
max_tokens = 4096

[budgets]
session = 50000     # 50K tokens per session
daily = 500000      # 500K tokens per day
monthly = 5000000   # 5M tokens per month
```

Set your API key:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

Or for OpenAI:

```toml
[agent]
provider = "openai"
model = "gpt-4o"
```

```bash
export OPENAI_API_KEY="sk-..."
```

## 3. Start the Gateway

```bash
# Local development (no auth required)
exoclaw gateway

# Remote access (auth required)
export EXOCLAW_TOKEN="my-secret-token"
exoclaw gateway --bind 0.0.0.0 --port 7200
```

## 4. Send Your First Message

Connect via WebSocket and send a JSON-RPC message:

```bash
# Using websocat (install: cargo install websocat)
websocat ws://127.0.0.1:7200/ws

# Then type:
{"id":"1","method":"ping"}
# Response: {"id":"1","result":"pong"}

{"id":"2","method":"chat.send","params":{"channel":"websocket","account":"me","content":"Hello!"}}
# Response: streamed text chunks...
```

For authenticated connections:

```bash
websocat ws://example.com:7200/ws

# First message must be auth:
{"token":"my-secret-token"}

# Then RPC messages:
{"id":"1","method":"chat.send","params":{"channel":"websocket","account":"me","content":"Hello!"}}
```

## 5. Load a Plugin

Build the echo plugin:

```bash
cd examples/echo-plugin
cargo build --target wasm32-unknown-unknown --release
cd ../..
```

Add it to your config:

```toml
[[plugins]]
name = "echo"
path = "examples/echo-plugin/target/wasm32-unknown-unknown/release/echo_plugin.wasm"
capabilities = []
```

Or load it via CLI:

```bash
exoclaw plugin load examples/echo-plugin/target/wasm32-unknown-unknown/release/echo_plugin.wasm
```

## 6. Check Status

```bash
# Via CLI
exoclaw status

# Via WebSocket RPC
{"id":"1","method":"status"}
# Response: {"id":"1","result":{"version":"0.1.0","plugins":1,"sessions":0}}
```

## 7. Add Routing Bindings

Route different channels to different agents:

```toml
[[bindings]]
channel = "websocket"
agent_id = "personal"

[[bindings]]
channel = "telegram"
agent_id = "personal"

[[bindings]]
channel = "whatsapp"
peer_id = "work-group-123"
agent_id = "work"
```

Binding priority: peer > guild > team > account > channel > default.

## Zero-Config Mode

Exoclaw works with no config file for local development:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
exoclaw gateway
```

Defaults:
- Bind: `127.0.0.1:7200`
- No auth (loopback only)
- Default agent: Anthropic Claude Sonnet
- No plugins loaded
- No token budgets (unlimited)

## Debug Logging

```bash
RUST_LOG=debug exoclaw gateway
RUST_LOG=exoclaw=trace exoclaw gateway  # verbose module-level tracing
```

## What's Next

- Write a custom WASM plugin: See `examples/echo-plugin/` for a minimal example
- Connect a channel adapter: Load a Telegram/WhatsApp plugin to receive messages from messaging platforms
- Configure memory: Set up soul documents and semantic memory for context-aware conversations
- Set budgets: Configure token limits to control LLM spend
