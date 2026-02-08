# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Is This

A secure, WASM-sandboxed AI agent runtime written in Rust. The "infrastructure layer" for AI agents — like Docker for AI agents. WebSocket gateway accepts JSON-RPC messages, routes them through a hierarchical session router to LLM providers, and executes tool calls in sandboxed WASM plugins.

**Phase: Early scaffold — compiles, CLI works, modules NOT wired together yet.** Dead-code warnings are expected since modules aren't connected.

## Build & Run

```bash
cargo check                        # type-check (fast feedback loop)
cargo build                        # debug build
cargo build --release              # release build (~21MB, LTO + strip)
cargo run -- onboard               # first-time setup (secure API key entry)
cargo run -- status                # smoke test
cargo run -- gateway --port 7200   # start gateway on loopback (no auth needed)
cargo run -- gateway --bind 0.0.0.0 --token my-secret  # non-loopback requires token
cargo run -- plugin load ./path/to/plugin.wasm          # load a WASM plugin
cargo test                         # run all tests (134 tests)
cargo clippy                       # lint (fix all non-dead-code warnings)
cargo fmt                          # format (canonical style, zero config)
cargo fmt --check                  # check formatting without modifying
RUST_LOG=debug cargo run -- gateway # enable debug logging
```

Dead-code warnings from `cargo clippy` are expected until modules are wired together. All other clippy warnings should be zero.

### Building the Chat UI (WASM)

```bash
# Prerequisites (one-time)
rustup target add wasm32-unknown-unknown
cargo install trunk

# Build the Leptos frontend
trunk build                        # dev build (fast, no wasm-opt)
trunk build --release              # release build (smaller bundle)

# The output goes to ui/dist/ which is embedded in the server binary via rust-embed.
# After rebuilding the UI, recompile the server: cargo build
```

The Chat UI is a Leptos 0.7 CSR app in `ui/`. It compiles to WASM and is embedded in the server binary. The gateway serves it at `/`.

### Building the echo plugin (WASM)

```bash
cd examples/echo-plugin
cargo build --target wasm32-unknown-unknown --release
# Output: target/wasm32-unknown-unknown/release/echo_plugin.wasm
```

Plugin crates use `crate-type = ["cdylib"]` and depend on `extism-pdk`.

## Architecture

```
Browser (http://localhost:7200) → ui/ (Leptos WASM, embedded via rust-embed)
                                      ↓ WebSocket
Client (WebSocket) → gateway/ (axum, auth, JSON-RPC)
                        → router/ (hierarchical session routing)
                            → agent/ (LLM streaming: Anthropic + OpenAI SSE)
                                → sandbox/ (Extism WASM plugin host)
                        → bus/ (NATS JetStream, optional)
                        → store/ (in-memory sessions, future: SurrealDB)
```

**Core loop (NOT YET WIRED):**
1. Client connects via WebSocket, authenticates with `{"token": "..."}` as first message
2. Sends JSON-RPC `chat.send` → router resolves target agent
3. Agent streams to LLM provider (Anthropic/OpenAI SSE)
4. Tool calls dispatched to WASM sandbox → result fed back to LLM
5. Final response streamed back to client via `mpsc::Sender<AgentEvent>`

**Binding priority (same as OpenClaw):** peer > guild > team > account > channel > default

### Module Relationships

- `main.rs` — CLI entry point (clap). Handles `onboard` (API key setup via `run_onboard()`), `gateway` (starts server), `plugin` (loads WASM plugins), and `status` commands.
- `gateway/server.rs` — Owns `AppState` which holds `SessionRouter`, `PluginHost`, `SessionStore`, `MemoryEngine`, `ExoclawConfig`, and per-session locks. Starts axum server, handles WebSocket upgrade, runs auth-then-message-loop per connection. Serves embedded UI at `/` via `rust-embed`.
- `secrets.rs` — Credential storage. Stores API keys at `~/.exoclaw/credentials/{provider}.key` with mode 0600. Functions: `store_api_key()`, `load_api_key()`. Provider whitelist prevents path traversal.
- `gateway/protocol.rs` — JSON-RPC dispatch. Accesses `AppState` to query plugins and router. Methods: `ping`, `status`, `chat.send` (stub), `plugin.list`.
- `gateway/auth.rs` — `verify_connect()` parses first WS message for token, compares with `subtle::ConstantTimeEq`. Returns true if no token configured (loopback mode).
- `router/mod.rs` — `SessionRouter` holds `Vec<Binding>` and `HashMap<String, SessionState>`. `resolve()` walks bindings in priority order, creates/updates sessions keyed as `{agent_id}:{channel}:{account}:{peer}`.
- `agent/mod.rs` — `AgentRunner` holds a `reqwest::Client`. `run()` dispatches to `run_anthropic()` or `run_openai()`, both doing SSE streaming and sending `AgentEvent`s over a channel. Not yet connected to the gateway message loop.
- `sandbox/mod.rs` — `PluginHost` stores `Manifest`s by name. `call()` creates a fresh `Plugin` instance per invocation (isolation). `register()` validates by trial instantiation.
- `bus/mod.rs` — `MessageBus` wraps `Option<async_nats::Client>`. Degrades gracefully if NATS unavailable.
- `store/mod.rs` — `SessionStore` is a `HashMap<String, Session>` with conversation history. Not yet integrated into the gateway.

## Gotchas / API Notes

- **Extism 1.x**: `Plugin::new(manifest, [], true)` takes `Manifest` by value, not reference. Clone the manifest if you need it later.
- **async-nats 0.38**: `publish()` subject needs `.to_string()`, payload needs `.to_vec().into()` for lifetime/type reasons.
- **axum 0.8**: WebSocket `.close()` requires `use futures::SinkExt`.
- **Auth enforcement**: Non-loopback bind requires `--token` or `EXOCLAW_TOKEN` env var. Loopback skips auth entirely (returns true with no token check).
- **Plugin instances**: Created fresh per `call()` invocation for isolation — the `PluginEntry` stores the `Manifest`, not a live `Plugin`.
- **Rust edition 2024**: All workspace crates use `edition = "2024"`.
- **Cargo workspace**: Root crate `exoclaw` (server binary) + `ui/` crate `exoclaw-ui` (Leptos WASM frontend). They share a `Cargo.lock` and `target/` directory.
- **Leptos 0.7 CSR**: The UI crate uses client-side rendering (`features = ["csr"]`). Built with `trunk build` which produces `ui/dist/` (index.html + WASM + JS glue). The server binary embeds these files via `rust-embed`.
- **UI asset embedding**: `rust-embed` embeds `ui/dist/` at compile time. After changing UI code, rebuild with `trunk build` then `cargo build` to re-embed.
- **Credential storage**: API keys stored at `~/.exoclaw/credentials/{provider}.key` (mode 0600). Resolution: env var > credential file > None. Config file never contains the key.

## Design Decisions

1. WebSocket + JSON-RPC protocol (validated by Discord/Slack at scale)
2. Extism for WASM plugins (not raw wasmtime) — stable API, 17 host SDKs
3. NATS JetStream for message bus — subject pattern `exoclaw.{channel}.{account}.{peer}`
4. SurrealDB planned for persistence (Rust-native, embeddable, graph+doc+vector)
5. Per-invocation plugin instances for isolation (no shared state between calls)
6. Capability grants per plugin (config-driven whitelist, see `examples/config.toml`)
7. Graceful degradation — NATS optional, falls back to in-process routing

## Config Format

See `examples/config.toml` for the planned config schema. Key sections: `[gateway]`, `[agent]`, `[[plugins]]` (with capability grants), `[[bindings]]` (routing rules).

## Specification Artifacts

See `specs/001-core-runtime/` for the full feature specification:
- `spec.md` — User stories, functional requirements, success criteria
- `plan.md` — Implementation plan, architecture, project structure
- `research.md` — Technology decisions and rationale
- `data-model.md` — Entity schemas and relationships
- `contracts/jsonrpc-spec.md` — WebSocket JSON-RPC protocol
- `quickstart.md` — Getting started guide

## Active Technologies
- Rust 2024 edition, Leptos 0.7 CSR + axum 0.8, gloo-net (WASM WebSocket client), pulldown-cmark (WASM markdown), rpassword 7, rust-embed (static asset embedding)
- Filesystem only — `~/.exoclaw/credentials/` for keys, `~/.exoclaw/config.toml` for config

## Recent Changes
- 002-onboard-chat-ui: Added Leptos 0.7 CSR chat UI (trunk + rust-embed), onboarding CLI, gloo-net WebSocket client, pulldown-cmark markdown rendering, 14 integration tests
