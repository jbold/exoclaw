# Architecture

## Layer diagram

```
 Clients (WebSocket)
       |
 +-----v-----------+
 |  gateway/        |  Axum HTTP/WS server, auth, JSON-RPC protocol
 +-----+------------+
       |
 +-----v-----------+
 |  router/         |  Hierarchical session routing (peer>guild>team>account>channel)
 +-----+------------+
       |
 +-----v-----------+
 |  agent/          |  LLM provider runner (Anthropic, OpenAI), SSE streaming
 +-----+------------+
       |
 +-----v-----------+
 |  sandbox/        |  Extism/WASM plugin host — isolated per-plugin execution
 +-----+------------+
       |
 +-----v-----------+           +------------------+
 |  store/          |           |  bus/             |
 |  In-memory       |           |  NATS JetStream   |
 |  session store   |           |  (optional)       |
 +------------------+           +------------------+
```

## Core loop

1. Client opens a WebSocket to `/ws` and sends an auth token.
2. Gateway authenticates (constant-time compare via `subtle`) and enters the message loop.
3. Each incoming JSON-RPC call is dispatched by `protocol::handle_rpc`.
4. `chat.send` resolves the target agent via `SessionRouter::resolve` (binding priority: peer > guild > team > account > channel > default).
5. `AgentRunner::run` streams the request to the configured LLM provider (Anthropic or OpenAI).
6. If the LLM returns a `tool_use` block, the tool is executed inside the WASM sandbox via `PluginHost::call`, and the result is fed back to the LLM.
7. Steps 5-6 repeat until the LLM produces a final text response.
8. The response is streamed back to the client over the WebSocket.

## Security model

- **WASM isolation**: each plugin runs in its own Extism sandbox. No filesystem, network, or host memory access unless explicitly granted via capability list.
- **Capability grants**: declared per-plugin in config (e.g. `["http:api.telegram.org", "store:sessions"]`). The host only exposes allowed resources.
- **Auth**: non-loopback connections require a bearer token checked with constant-time comparison. Loopback binds skip auth.
- **Transport**: WebSocket-based JSON-RPC. Token sent in the first message; rejected connections are closed immediately.

## Modules

| Path | Purpose |
|---|---|
| `src/main.rs` | CLI entry point (clap): gateway, plugin, status subcommands |
| `src/gateway/server.rs` | Axum WS server, connection lifecycle, health endpoint |
| `src/gateway/auth.rs` | Token verification with constant-time equality |
| `src/gateway/protocol.rs` | JSON-RPC dispatch (ping, status, chat.send, plugin.list) |
| `src/router/mod.rs` | Hierarchical session router with binding priority chain |
| `src/agent/mod.rs` | LLM agent runner, Anthropic + OpenAI SSE streaming |
| `src/sandbox/mod.rs` | Extism WASM plugin host, load/call/list lifecycle |
| `src/bus/mod.rs` | Optional NATS JetStream message bus (subject: exoclaw.{channel}.{account}.{peer}) |
| `src/store/mod.rs` | In-memory session/conversation store (future: SurrealDB) |
