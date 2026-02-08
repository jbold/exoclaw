# WebSocket JSON-RPC Protocol: Exoclaw Core Runtime

**Branch**: `001-core-runtime` | **Date**: 2026-02-08
**Input**: FR-001 through FR-003 from [spec.md](../spec.md), existing `protocol.rs`

## Overview

Exoclaw communicates over WebSocket using a JSON-RPC-inspired protocol. The connection lifecycle is: connect → optional authenticate → receive hello → send/receive JSON-RPC messages → close.

This is **not strict JSON-RPC 2.0** — it omits `jsonrpc: "2.0"` and uses a simplified error model. The protocol is designed for simplicity and WebSocket streaming.

## Connection Lifecycle

```text
Client                              Gateway
  │                                    │
  ├──── WebSocket upgrade ────────────►│
  │                                    │
  │◄─── 101 Switching Protocols ──────┤
  │                                    │
  ├──── {"token": "secret"} ──────────►│  (1) Auth message (required if token auth enabled)
  │                                    │
  │◄─── {"ok":true,"version":"0.1.0"} ┤  (2) Hello frame
  │                                    │
  ├──── {"id":"1","method":"ping"} ───►│  (3) RPC messages
  │◄─── {"id":"1","result":"pong"} ───┤
  │                                    │
  ├──── {"id":"2","method":"chat.send",│  (4) Streaming RPC
  │      "params":{...}} ─────────────►│
  │◄─── {"id":"2","event":"text", ────┤      Response chunks
  │       "data":"Hello"} ────────────┤
  │◄─── {"id":"2","event":"text", ────┤
  │       "data":" world"} ───────────┤
  │◄─── {"id":"2","event":"done"} ────┤      Completion signal
  │                                    │
  ├──── WebSocket close ──────────────►│  (5) Disconnect
  └────────────────────────────────────┘
```

## Authentication

### Non-Loopback Bind

The **first message** after WebSocket upgrade MUST be an authentication payload:

```json
{"token": "your-auth-token"}
```

The gateway compares the token using constant-time comparison (`subtle::ConstantTimeEq`). On failure, it sends:

```json
{"error":"auth_failed","code":4001}
```

then closes the socket.

**Source**: `src/gateway/auth.rs`, spec FR-002

### Loopback Bind (127.0.0.1)

Authentication is skipped entirely. No token message is required.

**Source**: `src/gateway/server.rs` + `src/gateway/auth.rs`

## Request Format

```json
{
  "id": "unique-request-id",
  "method": "method.name",
  "params": { ... }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Client-generated request identifier. Echoed in all responses. |
| `method` | string | Yes | RPC method name (see Methods below). |
| `params` | object | No | Method-specific parameters. Defaults to `{}`. |

## Response Format

### Standard Response (non-streaming)

```json
{
  "id": "request-id",
  "result": { ... },
  "error": null
}
```

### Error Response

```json
{
  "id": "request-id",
  "result": null,
  "error": "human-readable error message"
}
```

### Streaming Response (chat.send)

Multiple messages sent for a single request ID:

```json
{"id": "request-id", "event": "text", "data": "chunk of text"}
{"id": "request-id", "event": "text", "data": " more text"}
{"id": "request-id", "event": "tool_use", "data": {"id": "call_123", "name": "echo", "input": {"text": "hi"}}}
{"id": "request-id", "event": "tool_result", "data": {"tool_use_id": "call_123", "content": "hi"}}
{"id": "request-id", "event": "usage", "data": {"input_tokens": 150, "output_tokens": 42}}
{"id": "request-id", "event": "done"}
{"id": "request-id", "event": "error", "data": "error message"}
```

| Event Type | Description |
|-----------|-------------|
| `text` | A chunk of the LLM's text response. `data` is a string. |
| `tool_use` | LLM requested a tool call. `data` contains `id`, `name`, `input`. |
| `tool_result` | Result of a tool execution. `data` contains `tool_use_id`, `content`, optional `is_error`. |
| `usage` | Token usage for this request. `data` contains `input_tokens`, `output_tokens`. |
| `done` | Stream is complete. No more events for this request ID. |
| `error` | An error occurred. `data` is a string. Terminates the stream. |

## Methods

### `ping`

Health check. Returns immediately.

**Params**: None

**Response**:
```json
{"id": "1", "result": "pong"}
```

**Existing code**: Implemented in `src/gateway/protocol.rs`

---

### `status`

Returns gateway status information.

**Params**: None

**Response**:
```json
{
  "id": "1",
  "result": {
    "version": "0.1.0",
    "plugins": 2,
    "sessions": 5
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Crate version from Cargo.toml |
| `plugins` | number | Number of loaded plugins |
| `sessions` | number | Number of active sessions |

**Existing code**: Implemented in `src/gateway/protocol.rs`

---

### `chat.send`

Send a message and receive a streamed AI response.

**Params**:
```json
{
  "channel": "websocket",
  "account": "user-123",
  "peer": "main",
  "content": "Hello, how are you?",
  "guild": null,
  "team": null
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `channel` | string | Yes | Channel identifier (e.g., "websocket", "telegram") |
| `account` | string | Yes | Account within the channel |
| `peer` | string | No | Specific peer. Defaults to "main". |
| `content` | string | Yes | User message text |
| `guild` | string | No | Guild/server for Discord-like channels |
| `team` | string | No | Team within a guild |

**Response**: Streaming (see Streaming Response format above)

**Flow**:
1. Router resolves agent from bindings (FR-004)
2. Session created/retrieved by key `{agent_id}:{channel}:{account}:{peer}` (FR-005)
3. Message serialized per-session (FR-006)
4. Memory engine assembles context: soul + semantic entities + recent turns (FR-009)
5. Token budget checked (FR-019)
6. LLM called with streaming SSE (FR-007)
7. If `tool_use` in response → dispatch to WASM sandbox → feed result back → loop (FR-008)
8. Text chunks streamed to client (FR-003)
9. Token usage recorded (FR-018, FR-020)
10. Entities extracted for semantic memory (FR-015)

**Error Cases**:
- No agent configured for route: `{"error": "no agent configured for channel:account:peer"}`
- Budget exceeded: `{"error": "token budget exceeded (session: 9500/10000)"}`
- LLM provider unreachable: `{"error": "provider error: connection refused"}`

**Existing code**: Implemented in `src/gateway/protocol.rs` and streamed by `src/gateway/server.rs`

---

### `plugin.list`

List all loaded plugins.

**Params**: None

**Response**:
```json
{
  "id": "1",
  "result": [
    {"name": "echo"},
    {"name": "telegram"}
  ]
}
```

**Existing code**: Implemented in `src/gateway/protocol.rs`

---

### Future Methods (Not in v1)

| Method | Description |
|--------|-------------|
| `chat.history` | Retrieve conversation history for a session |
| `session.list` | List active sessions |
| `session.clear` | Clear a session's episodic memory |
| `plugin.load` | Dynamically load a plugin at runtime |
| `plugin.unload` | Unload a plugin |
| `budget.status` | Query current token budget usage |
| `memory.search` | Search semantic memory |

## Error Handling

- **Malformed JSON**: Returns error with `id: "0"` since the request ID can't be parsed.
- **Unknown method**: Returns error with the request's `id`.
- **Provider errors**: Sent as `error` events in the stream, then `done`.
- **WASM plugin errors**: Sent as `tool_result` with `is_error: true`, agent loop continues.
- **WebSocket disconnect mid-stream**: Stream is dropped, no cleanup needed (tokio channels handle this).

## Rate Limiting (Future)

Not implemented in v1. When added:
- Per-connection message rate limit
- Per-session concurrent request limit (1 — messages serialized per-session)
- Token budget enforcement serves as a natural rate limit
