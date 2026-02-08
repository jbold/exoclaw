# exoclaw — Project Instructions

## What Is This

A secure, WASM-sandboxed AI agent runtime written in Rust. Positioned as the "infrastructure layer" for AI agents — like Docker for AI agents. Not a clone of OpenClaw; this is the runtime/substrate layer that doesn't exist yet.

- Repo: `/var/home/bean/work/exoclaw/`
- Reference implementation studied: `/var/home/bean/openclaw/` (OpenClaw, 290K LOC TypeScript)
- Session log: `~/.claude/session-summaries/2026/02/2026-02-07_exoclaw-architecture-and-scaffold.md`

## Project Status

**Phase: Early scaffold — compiles, CLI works, modules NOT wired together yet.**

- 826 LOC across 10 Rust source files
- `cargo check` passes (dead-code warnings only — expected since modules aren't connected)
- `cargo run -- status` works
- Git initialized, NO commits yet, NO GitHub repo yet

## Architecture

```
Client (WebSocket) → gateway/ (axum, auth, JSON-RPC)
                        → router/ (hierarchical session routing)
                            → agent/ (LLM streaming: Anthropic + OpenAI SSE)
                                → sandbox/ (Extism WASM plugin host)
                        → bus/ (NATS JetStream, optional)
                        → store/ (in-memory sessions, future: SurrealDB)
```

**Core loop (NOT YET WIRED):**
1. Client connects via WebSocket, authenticates
2. Sends JSON-RPC `chat.send` → router resolves target agent
3. Agent streams to LLM provider (Anthropic/OpenAI)
4. Tool calls dispatched to WASM sandbox → result fed back to LLM
5. Final response streamed back to client

**Binding priority (same as OpenClaw):** peer > guild > team > account > channel > default

## Key Files

| File | LOC | Purpose |
|------|-----|---------|
| `src/main.rs` | 85 | CLI: gateway, plugin, status subcommands |
| `src/gateway/server.rs` | 114 | Axum WebSocket server + auth enforcement |
| `src/gateway/auth.rs` | 28 | Constant-time token verify (subtle crate) |
| `src/gateway/protocol.rs` | 84 | JSON-RPC dispatch (ping, status, chat.send, plugin.list) |
| `src/router/mod.rs` | 107 | Hierarchical session routing with bindings |
| `src/agent/mod.rs` | 196 | LLM provider streaming (Anthropic + OpenAI SSE) |
| `src/sandbox/mod.rs` | 94 | Extism WASM plugin host (register/call/list) |
| `src/bus/mod.rs` | 54 | NATS JetStream wrapper (degrades to local-only) |
| `src/store/mod.rs` | 59 | In-memory session store |

## Tech Stack

| Layer | Crate | Notes |
|-------|-------|-------|
| Async | tokio 1 (full) | |
| HTTP/WS | axum 0.8 (ws) | |
| WASM plugins | extism 1 | Plugin::new takes Manifest by value (clone needed) |
| LLM HTTP | reqwest 0.12 | Features: stream, rustls-tls, json |
| Message bus | async-nats 0.38 | publish needs .to_string() subject, .to_vec().into() payload |
| CLI | clap 4 | Features: derive, env |
| Auth | subtle 2 | Constant-time comparison |
| Serialization | serde 1 + serde_json 1 + rmp-serde 1 | JSON + MessagePack |

## Build & Run

```bash
cargo check          # type-check
cargo build          # debug build
cargo build --release # release (LTO, strip) — ~21MB binary
cargo run -- status  # quick smoke test
cargo run -- gateway --port 7200  # start gateway on loopback
```

## Gotchas / API Notes

- **Extism 1.x**: `Plugin::new(manifest, [], true)` takes Manifest by value, not reference. Clone the manifest if you need it later.
- **async-nats 0.38**: `publish()` subject param needs `subject.to_string()`, payload needs `payload.to_vec().into()` for lifetime/type reasons.
- **axum 0.8**: WebSocket `.close()` requires `use futures::SinkExt`.
- **Auth enforcement**: Non-loopback bind requires `--token` or `EXOCLAW_TOKEN` env var. Loopback skips auth.
- **Plugin instances**: Created fresh per invocation (not reused) for isolation.

## Design Decisions

1. WebSocket + JSON-RPC protocol (validated by Discord/Slack at scale)
2. Extism for WASM plugins (not raw wasmtime) — stable API, 17 host SDKs
3. NATS JetStream for message bus — subject `exoclaw.{channel}.{account}.{peer}`
4. SurrealDB planned for persistence (Rust-native, embeddable, graph+doc+vector)
5. Per-invocation plugin instances for isolation
6. Capability grants per plugin (config-driven whitelist)
7. Graceful degradation — NATS optional, falls back to in-process

## Immediate Next Steps (Priority Order)

1. Git initial commit
2. Create GitHub repo (github.com/exoclaw/exoclaw — name verified available 2026-02-07)
3. Wire `chat.send` RPC → SessionRouter::resolve → AgentRunner::run → stream back
4. Implement tool execution loop (LLM tool_use → PluginHost::call → feed back)
5. Build + test echo plugin end-to-end
6. Config file loading (TOML, see examples/config.toml)
7. Capability system (config-driven, per-plugin resource whitelist)
8. SurrealDB integration (replace in-memory store)
9. First channel adapter (Telegram or Discord)
10. Tests (router, auth, protocol unit tests; WebSocket integration test)

## Name Research (2026-02-07)

- **exoclaw**: All clear — GitHub, crates.io, npm, DNS
- **carapace**: TAKEN (GitHub org, crates.io, .dev domain, USPTO trademark)
- **rustclaw**: GitHub org squatted Feb 1, two independent repos exist
- Competitors already building Rust+OpenClaw clones: lucas-moraes/rustclaw, shimaenaga1123/rustclaw
