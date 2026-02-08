<!-- Sync Impact Report
  Version change: 0.0.0 → 1.0.0
  Modified principles: N/A (initial ratification)
  Added sections: Core Principles (5), Security Model, Performance Standards, Development Workflow, Governance
  Removed sections: All template placeholders
  Templates requiring updates:
    - .specify/templates/plan-template.md ✅ (no changes needed, Constitution Check section is generic)
    - .specify/templates/spec-template.md ✅ (no changes needed, structure accommodates our principles)
    - .specify/templates/tasks-template.md ✅ (no changes needed, phase structure works)
  Follow-up TODOs: None
-->

# Exoclaw Constitution

## Core Principles

### I. Secure by Default

Every external interaction — tools, channels, memory access, LLM calls — flows through a deny-by-default WASM capability boundary. Plugins cannot access the filesystem, network, host memory, or environment variables unless the host explicitly grants specific capabilities.

- Untrusted code (skills, tools, channel adapters) MUST run in WASM sandbox
- Capabilities MUST be granted per-plugin via config-driven allowlists (e.g., `http:api.telegram.org`)
- The host manages all persistent connections (WebSocket, SSE, long-polling); plugins handle discrete events only
- Token authentication MUST use constant-time comparison
- No plugin can bypass host-level metering, budgets, or security enforcement

### II. Cost-Aware by Architecture

Token spend is controlled through architectural design, not bolted-on limits. The memory engine retrieves relevant context (graph traversal + vector similarity) instead of dumping entire conversation history. Token metering lives in the trusted host layer where plugins cannot circumvent it.

- Context assembly MUST use selective retrieval (target 3-5K tokens per request, not 120K)
- Token metering MUST be host-side, counting actual wire data to/from LLM APIs
- Budgets MUST be configurable per-agent, per-session, per-day, per-month
- No cron/heartbeat pattern that sends full context on a timer; scheduled tasks use specific, scoped prompts
- LLM provider calls MUST be auditable (input tokens, output tokens, cost, timestamp logged)

### III. Simple Configuration

Configuration MUST be a single TOML file that a human can write from scratch in under 5 minutes for a basic setup. No config sprawl across multiple files, no wizard-only setup, no hidden state.

- Single config file: `~/.exoclaw/config.toml` (or `EXOCLAW_CONFIG` env var)
- Sane defaults: loopback bind, no auth required for local, default agent model
- Zero-config local mode: `exoclaw gateway` MUST work with no config file for development
- Every config option MUST have a sensible default; only API keys and channel tokens are mandatory
- Config schema MUST be documented in `examples/config.toml` with comments

### IV. WASM-First Plugin Model

All extensibility — channel adapters, tools, skills — ships as WASM modules (.wasm files). Plugins are language-agnostic (Rust, Go, JS, Python via Component Model), sandboxed by specification, and distributed as single files.

- Plugins MUST target `wasm32-unknown-unknown` or `wasm32-wasip2`
- Plugin host is Extism (on Wasmtime); migration to raw Wasmtime Component Model when Extism adds support
- Per-invocation plugin isolation: fresh WASM instance per call, no shared state between invocations
- Host functions expose controlled APIs to plugins (session storage, HTTP proxy, etc.)
- Plugin interfaces defined in the host; plugins implement `handle_message`, `handle_tool_call`, `describe`

### V. Performance Without Compromise

Exoclaw MUST be fast enough that users never wait on the runtime — only on LLM response time. Single static binary, sub-millisecond plugin instantiation, microsecond routing decisions, zero-copy where possible.

- Gateway MUST handle 10K+ concurrent WebSocket connections on commodity hardware
- Plugin instantiation MUST complete in under 1ms (WASM cold start)
- Session routing MUST complete in under 100 microseconds
- Release binary MUST be a single static binary under 25MB (LTO + strip)
- Memory usage MUST stay under 100MB for 1000 active sessions (excluding WASM instance memory)
- Startup to first request MUST complete in under 500ms

## Security Model

**Trust boundary**: The WASM membrane separates trusted host code (Rust) from untrusted plugin code (WASM).

| Layer | Trust | Examples |
|-------|-------|----------|
| Host runtime | Trusted | Gateway, router, agent loop, memory engine, capability system |
| WASM plugins | Untrusted | Channel adapters, tools, skills, community extensions |
| LLM providers | External | Anthropic, OpenAI — host manages connections, plugins never see API keys |
| User data | Protected | Conversation history, memory graph, config — host-only access |

Plugins interact with protected resources ONLY through host functions registered at instantiation. A plugin requesting a host function that wasn't granted fails at instantiation, not at runtime.

## Performance Standards

| Metric | Target | Measurement |
|--------|--------|-------------|
| Concurrent connections | 10,000+ | `wrk` or `k6` benchmark |
| Plugin cold start | < 1ms | `tracing` span timing |
| Route resolution | < 100us | `criterion` benchmark |
| Memory per 1K sessions | < 100MB | `heaptrack` or RSS measurement |
| Binary size (release) | < 25MB | `ls -la target/release/exoclaw` |
| Startup to ready | < 500ms | Time from exec to first accepted connection |
| Context tokens per request | 3-5K typical | Token counter in agent loop |

## Development Workflow

- `cargo check` for fast feedback during development
- `cargo clippy` MUST pass with zero warnings (dead-code warnings excepted during scaffold phase)
- `cargo fmt --check` MUST pass — canonical rustfmt style, zero config
- `cargo test` MUST pass before any commit to main
- All new public APIs MUST have at least one unit test
- Integration tests for the WebSocket protocol use `tokio-test`
- WASM plugins are tested by building to `wasm32-unknown-unknown` and calling via `PluginHost` in tests
- Rust edition 2024 for all crates (main + plugins)

## Governance

This constitution governs all development decisions for exoclaw. Amendments require:

1. A written rationale explaining what changed and why
2. Version bump following semver (MAJOR: principle removal/redefinition, MINOR: new principle/expansion, PATCH: clarification)
3. Update to this file with the Sync Impact Report comment at top
4. Propagation check across `.specify/templates/` for consistency

The constitution supersedes informal practices. If a development decision contradicts a principle, either change the code or amend the constitution — never leave them in conflict.

**Version**: 1.0.0 | **Ratified**: 2026-02-08 | **Last Amended**: 2026-02-08
