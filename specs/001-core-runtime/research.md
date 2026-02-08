# Phase 0 Research: Exoclaw Core Runtime

**Branch**: `001-core-runtime` | **Date**: 2026-02-08
**Input**: Technology decisions from brainstorming + architecture design phase

## Summary

All technology choices were resolved during the architecture design phase. No NEEDS CLARIFICATION markers remain in the plan. This document consolidates the decisions, rationale, and alternatives considered.

## Decisions

### 1. WASM Runtime: Extism on Wasmtime

**Decision**: Use Extism 1.x (which wraps Wasmtime) as the WASM plugin host.

**Rationale**:
- Wasmtime is the reference WASM runtime, maintained by the Bytecode Alliance
- WASI 0.2 support is stable (HTTP, filesystem, sockets capabilities)
- Extism adds a high-level plugin API on top: manifest-driven capability grants, `allowed_hosts`, host function registration, per-invocation isolation
- 17 host SDKs (Rust, Go, Python, JS, etc.) for writing plugins
- Production-proven at Fermyon, Shopify, Fastly

**Alternatives Considered**:
- **WasmEdge**: Rejected. Its AI/WASI-NN features are irrelevant — exoclaw calls external LLM APIs via HTTP, it doesn't run local inference. Component Model support lags behind Wasmtime. Proprietary WASI extensions reduce portability.
- **Wasmer + WASIX**: Rejected. WASIX is a proprietary superset of WASI with lock-in risk. Smaller ecosystem than Wasmtime.
- **Raw Wasmtime (no Extism)**: Considered for future migration. Extism provides a cleaner plugin API today; when Extism adds full Component Model support, migration path is smooth.

### 2. Async Runtime: tokio

**Decision**: tokio 1.x as the async runtime.

**Rationale**:
- De facto standard for async Rust
- Required by axum, reqwest, async-nats — all our key dependencies
- Multi-threaded work-stealing scheduler handles 10K+ concurrent connections
- Well-tested, production-proven at scale

**Alternatives Considered**:
- **async-std**: Smaller ecosystem, fewer library integrations. No compelling advantage.
- **smol**: Lightweight but would require replacing axum, reqwest, and other tokio-native deps.

### 3. HTTP/WebSocket Server: axum 0.8

**Decision**: axum 0.8 for the WebSocket gateway and HTTP endpoints.

**Rationale**:
- Built on top of tokio and hyper — minimal overhead
- First-class WebSocket support with upgrade handling
- Tower middleware ecosystem (compression, tracing, rate limiting)
- Maintained by the tokio team
- Type-safe extractors reduce boilerplate

**Alternatives Considered**:
- **actix-web**: Mature but uses its own runtime (not pure tokio). Harder to integrate with tokio-native crates.
- **warp**: Filter-based API is elegant but less intuitive for complex routing. Maintenance has slowed.

### 4. HTTP Client: reqwest 0.12

**Decision**: reqwest 0.12 for LLM provider API calls.

**Rationale**:
- De facto standard Rust HTTP client
- Built on hyper + tokio — shares the runtime with axum
- Supports streaming responses (SSE) via `bytes_stream()`
- Connection pooling, TLS, compression built-in

**Alternatives Considered**:
- **hyper (direct)**: Lower-level, more boilerplate for JSON APIs. No advantage for our use case.
- **ureq**: Blocking HTTP client. Incompatible with our async architecture.

### 5. Serialization: serde + serde_json

**Decision**: serde 1.x + serde_json for all serialization.

**Rationale**:
- Standard Rust serialization framework, zero-cost abstraction via derive macros
- JSON-RPC protocol requires JSON; serde_json is the canonical implementation
- TOML config parsing uses serde via the `toml` crate
- Already used throughout the scaffold code

**Alternatives Considered**: None. serde is the only viable choice in Rust.

### 6. Configuration: TOML via the `toml` crate

**Decision**: Single TOML config file, parsed by the `toml` crate with serde integration.

**Rationale**:
- Human-readable and writable — a user can write a config from scratch in 5 minutes (Constitution III)
- Native serde support via `toml::from_str`
- Hierarchical structure maps cleanly to `[gateway]`, `[agent]`, `[[plugins]]`, `[[bindings]]`
- Used by Cargo.toml — familiar to Rust developers

**Alternatives Considered**:
- **YAML**: More error-prone (indentation sensitivity, implicit type coercion).
- **JSON**: No comments, verbose for human editing.
- **RON**: Rust-specific, unfamiliar to non-Rust users.

### 7. CLI Framework: clap 4

**Decision**: clap 4 with derive macros for CLI argument parsing.

**Rationale**:
- Standard Rust CLI framework
- Derive macros eliminate boilerplate
- Subcommand support maps cleanly to `gateway`, `status`, `plugin` commands
- Built-in help generation, shell completions

**Alternatives Considered**:
- **argh**: Simpler but fewer features. No shell completions.
- **structopt**: Merged into clap 4 as the derive API.

### 8. Storage Strategy: In-Memory Now, SurrealDB Later

**Decision**: Start with `HashMap`-based in-memory stores. Migrate to embedded SurrealDB when the core loop is proven.

**Rationale**:
- In-memory stores let us validate the architecture without storage complexity
- SurrealDB is Rust-native, embeddable, supports graph + document + vector queries in one engine
- Graph queries serve semantic memory (entity relationships)
- Vector search serves context retrieval (similarity-based recall)
- Document storage serves session/conversation persistence
- Single embedded database vs. three separate systems

**Alternatives Considered**:
- **SQLite + pgvector**: Two databases to manage. No native graph queries.
- **Redis**: Not embeddable in a single binary. Requires external process.
- **RocksDB**: Key-value only. Would need to build graph and vector layers.
- **Qdrant**: Vector-only. Would still need a document store.

### 9. Message Bus: NATS JetStream (Optional)

**Decision**: NATS JetStream for inter-process messaging, with graceful degradation when unavailable.

**Rationale**:
- Subject-based routing maps to `exoclaw.{channel}.{account}.{peer}`
- JetStream provides persistence and replay for offline message delivery
- Graceful degradation: gateway works without NATS via in-process routing
- Lightweight, single-binary server

**Alternatives Considered**:
- **RabbitMQ**: Heavier, requires Erlang runtime.
- **Kafka**: Overkill for single-user personal assistant scope.
- **No bus**: Valid for v1, but NATS enables future multi-instance deployment.

### 10. LLM Provider Strategy: Anthropic + OpenAI via Direct HTTP

**Decision**: Call LLM APIs directly via reqwest SSE streaming. No SDK dependencies.

**Rationale**:
- Both Anthropic and OpenAI use SSE streaming over HTTPS — simple to implement
- Direct HTTP avoids SDK version churn and dependency bloat
- Host controls all API keys — plugins never see credentials (Constitution I)
- Provider abstraction in `agent/providers.rs` makes adding new providers trivial

**Alternatives Considered**:
- **Official SDKs (anthropic-sdk, openai-sdk)**: Additional dependencies, may not support streaming as cleanly, version lock-in.
- **LiteLLM proxy**: External process dependency. Violates single-binary constraint.

### 11. Token Counting: Host-Side Estimation + Wire Verification

**Decision**: Estimate tokens before LLM call (for budget checking), then use actual token counts from provider response (for metering).

**Rationale**:
- Pre-call estimation enables budget enforcement before spending money
- Post-call wire data provides accurate metering (Constitution II: "counting actual wire data")
- tiktoken-compatible estimation for pre-call checks (BPE tokenizer)
- Provider response always includes `usage.input_tokens` / `usage.output_tokens`

**Alternatives Considered**:
- **Only provider-reported counts**: Can't enforce budgets before the call is made.
- **Only estimation**: Estimation drift over time would make metering inaccurate.

### 12. Plugin Interface Pattern: Shopify Functions / Fermyon Spin

**Decision**: Plugins handle discrete events only. Host manages all persistent connections.

**Rationale**:
- Proven pattern at Shopify (serverless WASM functions) and Fermyon (Spin)
- Channel plugins implement `parse_incoming` and `format_outgoing` — pure data transformation
- Host manages WebSocket, polling, webhooks to messaging platforms
- Fresh WASM instance per invocation — no shared state, no connection leaks
- Plugin can't hold open connections, can't bypass metering (Constitution I, II)

**Alternatives Considered**:
- **Long-running plugin processes**: WASM sandboxing would need to handle persistent state, connection management, and cleanup. Much more complex with weaker isolation guarantees.
