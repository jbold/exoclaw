# Implementation Plan: Exoclaw Core Runtime

**Branch**: `001-core-runtime` | **Date**: 2026-02-08 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-core-runtime/spec.md`

## Summary

Build a capability-gated WASM agent runtime that accepts messages via WebSocket, routes them through hierarchical session bindings to LLM providers (Anthropic/OpenAI), executes tool calls in sandboxed WASM plugins, meters token usage, and maintains multi-layer memory (episodic + semantic + soul). Single static Rust binary, single TOML config file, deny-by-default security.

## Technical Context

**Language/Version**: Rust 2024 edition (1.85+)
**Primary Dependencies**: tokio 1 (async), axum 0.8 (WebSocket/HTTP), extism 1 (WASM plugin host on Wasmtime), reqwest 0.12 (LLM HTTP client), serde/serde_json (serialization), clap 4 (CLI)
**Storage**: SurrealDB embedded (graph + document + vector) for memory; in-memory fallback during initial development
**Testing**: cargo test, tokio-test, criterion (benchmarks)
**Target Platform**: Linux x86_64 (primary), macOS aarch64 (secondary), single static binary
**Project Type**: Single Rust binary crate with WASM plugin examples
**Performance Goals**: 10K concurrent WebSocket connections, <1ms WASM instantiation, <100us route resolution, <100ms runtime overhead per message
**Constraints**: <25MB release binary, <100MB RAM for 1K sessions, zero external runtime dependencies
**Scale/Scope**: Single-user personal assistant (initial), multi-tenant capable (future)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Evidence |
|-----------|--------|----------|
| I. Secure by Default | PASS | All plugins run in Extism WASM sandbox. Capability grants via `allowed_hosts` on Manifest. Fresh instance per invocation. Host manages all persistent connections. |
| II. Cost-Aware by Architecture | PASS | Memory engine retrieves selectively (target 3-5K tokens). Token metering in host agent loop counts wire data. Budgets enforced before LLM call. No heartbeat/cron context dumps. |
| III. Simple Configuration | PASS | Single `~/.exoclaw/config.toml`. Zero-config local mode works (loopback, no auth, defaults). Only API keys mandatory. |
| IV. WASM-First Plugin Model | PASS | Tools and channel adapters are WASM modules. Extism on Wasmtime. Per-invocation isolation. Host functions for controlled APIs. `wasm32-unknown-unknown` target. |
| V. Performance Without Compromise | PASS | tokio for async I/O. Extism WASM instantiation <1ms. In-memory routing. LTO+strip release build. Performance targets in constitution matched by tech choices. |

## Project Structure

### Documentation (this feature)

```text
specs/001-core-runtime/
├── plan.md              # This file
├── research.md          # Phase 0: technology decisions
├── data-model.md        # Phase 1: entity schemas
├── quickstart.md        # Phase 1: getting started guide
├── contracts/           # Phase 1: protocol contracts
│   └── jsonrpc-spec.md  # WebSocket JSON-RPC protocol
└── tasks.md             # Phase 2: implementation tasks
```

### Source Code (repository root)

```text
src/
├── main.rs              # CLI entry point (clap)
├── config.rs            # TOML config loading + validation (NEW)
├── gateway/
│   ├── mod.rs           # Module exports
│   ├── server.rs        # axum WebSocket server + AppState
│   ├── auth.rs          # Constant-time token verification
│   └── protocol.rs      # JSON-RPC dispatch
├── router/
│   └── mod.rs           # Hierarchical session routing + bindings
├── agent/
│   ├── mod.rs           # Agent loop: context assembly → LLM → tool dispatch → stream
│   ├── providers.rs     # LLM provider abstraction (Anthropic, OpenAI SSE) (NEW)
│   └── metering.rs      # Token counting + budget enforcement (NEW)
├── sandbox/
│   ├── mod.rs           # PluginHost: register, call, capability grants
│   └── capabilities.rs  # Capability parsing + Manifest configuration (NEW)
├── memory/
│   ├── mod.rs           # Memory engine: context assembly from all layers (NEW)
│   ├── episodic.rs      # Sliding window of recent turns (NEW)
│   ├── semantic.rs      # Entity/relationship extraction + graph storage (NEW)
│   └── soul.rs          # Soul document loading (NEW)
├── bus/
│   └── mod.rs           # NATS message bus (optional, graceful degradation)
└── store/
    └── mod.rs           # Session store (in-memory now, SurrealDB later)

examples/
├── config.toml          # Reference config with comments
└── echo-plugin/
    ├── Cargo.toml       # Plugin crate: cdylib, extism-pdk
    └── src/lib.rs       # Minimal plugin demonstrating handle_tool_call

tests/
├── auth_test.rs         # Unit: verify_connect with/without tokens (NEW)
├── router_test.rs       # Unit: binding resolution priority (NEW)
├── protocol_test.rs     # Unit: JSON-RPC dispatch (NEW)
├── metering_test.rs     # Unit: token counting + budget enforcement (NEW)
├── sandbox_test.rs      # Integration: load echo plugin, call, verify isolation (NEW)
└── gateway_test.rs      # Integration: WebSocket connect, auth, chat.send e2e (NEW)
```

**Structure Decision**: Single Rust binary crate. Modules map 1:1 to architectural components. New modules (`config`, `memory/*`, `agent/providers`, `agent/metering`, `sandbox/capabilities`) extend the existing scaffold without restructuring it. Tests live in a top-level `tests/` directory for integration tests; unit tests inline via `#[cfg(test)]` modules.

## Architecture

### Data Flow

```text
Client (WebSocket)
    │
    ▼
gateway/server.rs ── auth ── protocol.rs (JSON-RPC dispatch)
    │                              │
    │                    ┌─────────▼──────────┐
    │                    │   router/mod.rs     │
    │                    │   resolve(channel,  │
    │                    │   account, peer)    │
    │                    │   → agent_id +      │
    │                    │     session_key     │
    │                    └─────────┬──────────┘
    │                              │
    │                    ┌─────────▼──────────┐
    │                    │   agent/mod.rs      │
    │                    │   AGENT LOOP:       │
    │                    │   1. memory.assemble│
    │                    │   2. metering.check │
    │                    │   3. provider.call  │
    │                    │   4. if tool_use:   │
    │                    │      sandbox.call   │
    │                    │      → loop to 3    │
    │                    │   5. stream back    │
    │                    │   6. metering.record│
    │                    │   7. memory.extract │
    │                    └─────────┬──────────┘
    │                              │
    ▼                              ▼
stream chunks ◄──── mpsc::Sender<AgentEvent>
```

### Trust Boundary

```text
┌──────────────────────────────────────────────────────┐
│  TRUSTED HOST (native Rust)                          │
│  gateway, router, agent loop, memory, metering,      │
│  capability grant system                              │
│                                                       │
│  ┌─ WASM BOUNDARY ──────────────────────────────┐    │
│  │  UNTRUSTED PLUGINS (sandboxed)                │    │
│  │  tools, skills, channel protocol adapters     │    │
│  │  • No filesystem access                       │    │
│  │  • No env var access                          │    │
│  │  • HTTP only to allowed_hosts                 │    │
│  │  • Host functions only as granted             │    │
│  │  • Fresh instance per invocation              │    │
│  └───────────────────────────────────────────────┘    │
│                                                       │
│  LLM API calls: host → reqwest → provider API        │
│  (plugins never see API keys)                         │
└──────────────────────────────────────────────────────┘
```

### Memory Architecture

```text
Context Assembly (per LLM call):
  soul.md (~500 tokens, always included)
  + semantic entities relevant to query (~500-1K tokens, graph traversal)
  + recent turns sliding window (~1-2K tokens, last 3-5 turns)
  + tool schemas (~500 tokens, from loaded plugins)
  = ~3-5K tokens total

Post-response processing:
  LLM response → entity extraction → semantic store (graph edges with temporal metadata)
  LLM response → append to episodic store (sliding window)
```

### Token Metering Flow

```text
Before LLM call:
  1. Count assembled context tokens (tiktoken-compatible estimation)
  2. Check session budget, daily budget, monthly budget
  3. If any exceeded → return BudgetExceeded error, skip LLM call

After LLM call:
  4. Parse usage from provider response (input_tokens, output_tokens)
  5. Record: { session_key, provider, model, input_tokens, output_tokens, cost_estimate, timestamp }
  6. Update cumulative counters (session, daily, monthly)
```

## Complexity Tracking

No constitution violations to justify.

## Implementation Phases (Summary)

Implementation is ordered by user story priority (P1 → P5), with each story independently testable:

1. **Foundation**: Config loading, test infrastructure, wire existing modules together
2. **US1 (P1)**: Core message loop — `chat.send` → router → agent → LLM stream → response
3. **US2 (P2)**: Tool-use loop — LLM tool_use → WASM sandbox → result → LLM → repeat
4. **US3 (P3)**: Token metering — counting, budgets, audit logging
5. **US4 (P4)**: Multi-layer memory — episodic, semantic extraction, soul, context assembly
6. **US5 (P5)**: Channel adapters — WASM plugin interface for protocol translation
7. **Polish**: Performance benchmarks, documentation, cleanup

Detailed task breakdown will be generated by `/speckit.tasks`.
