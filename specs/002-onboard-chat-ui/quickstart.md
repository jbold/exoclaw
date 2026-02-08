# Quickstart: Onboarding & Chat UI

**Feature**: `002-onboard-chat-ui`

## Prerequisites

- Rust toolchain (rustup)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- cargo-leptos: `cargo install cargo-leptos`
- An API key from Anthropic or OpenAI

## First-Time Setup

```bash
# Clone and build
git clone https://github.com/exoclaw/exoclaw.git
cd exoclaw
cargo leptos build --release

# Run onboarding (one-time)
cargo run -- onboard
# Select provider: anthropic (default) or openai
# Enter your API key (hidden input)

# Start the gateway
cargo run -- gateway
```

## Open the Chat UI

Navigate to `http://localhost:7200` in your browser. Type a message and press Enter.

That's it. Three commands from clone to conversation:
1. `cargo leptos build --release`
2. `cargo run -- onboard`
3. `cargo run -- gateway`

## What Happened

- `onboard` saved your API key to `~/.exoclaw/credentials/anthropic.key` (mode 0600)
- `onboard` wrote a config to `~/.exoclaw/config.toml` (mode 0600, no key in file)
- `gateway` loaded the config, resolved the key from the credential file, and started listening
- The chat UI at `/` connects via WebSocket to `/ws` and sends JSON-RPC messages

## Re-configuring

```bash
# Switch providers
cargo run -- onboard --provider openai

# Or use environment variables (takes precedence over credential file)
ANTHROPIC_API_KEY=sk-ant-... cargo run -- gateway
OPENAI_API_KEY=sk-... cargo run -- gateway
```

## Development Mode

```bash
# Hot-reload during frontend development
cargo leptos watch

# Server-only checks (no WASM rebuild)
cargo check
cargo test
cargo clippy
```

## Non-Loopback (Remote Access)

```bash
# Bind to all interfaces with auth token
cargo run -- gateway --bind 0.0.0.0 --token my-secret-token

# The chat UI will prompt for the token when it loads
```
