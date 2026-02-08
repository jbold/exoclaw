# Session Log: Testing Rollout + Gateway Stability

Date: 2026-02-08
Repo: `exoclaw`

## Session Goals

- Stand up a full test stack for Rust + WASM + Leptos.
- Diagnose and fix gateway chat instability (first message works, later messages hang).
- Validate CI/PR gates and merge stabilized changes.
- Assess architecture posture versus OpenClaw and document a security path.

## Work Completed

### 1) Full Testing Suite and CI Gates

- Added layered testing workflow and documentation:
- `TESTING.md`
- `scripts/test-rust.sh`
- `scripts/test-wasm-ui.sh`
- `scripts/test-e2e.sh`
- `scripts/test-all.sh`

- Added CI workflow with required jobs:
- Rust tests + coverage gate
- Dependency security (`cargo-audit`, `cargo-deny`)
- WASM UI tests (`wasm-pack` / `wasm-bindgen-test`)
- Playwright E2E
- File: `.github/workflows/test-suite.yml`

- Added supporting artifacts:
- Playwright config and E2E specs
- WASM UI tests
- Coverage/security config files (`deny.toml`, `.cargo/audit.toml`)

### 2) Gateway Protocol + Streaming Stability Fixes

- Root cause fixed:
- The agent event channel could fill during streaming tool runs, causing deadlock because provider output was not drained concurrently.

- Key fixes:
- Concurrent provider/event draining with `tokio::select!` in `src/agent/mod.rs`.
- Regression test for high-volume streaming drain behavior:
- `run_with_tools_drains_stream_while_provider_is_running`
- Numeric JSON-RPC `id` handling regression covered in `tests/protocol_test.rs` (`ping_accepts_numeric_id`).
- SSE parsing hardening for framing variants and stream timeout behavior in `src/agent/providers.rs`.
- Improved websocket/gateway diagnostics and stream frame logging in `src/gateway/protocol.rs` and `src/gateway/server.rs`.
- UI websocket resilience and timeout/close handling improvements in `ui/src/ws.rs`.

### 3) Validation and Merge

- Validation commands run:
- `cargo test`
- `./scripts/test-wasm-ui.sh`
- `./scripts/test-e2e.sh`

- PR/merge outcomes:
- `dd97b30` — Phase 2 test suite + CI security gates (#6)
- `a1857ea` — Streaming deadlock fix + websocket/SSE hardening (#7)
- Both are now on `main`.

## Operational Notes from Debugging

- Running gateway in a detached background process required proper `nohup/setsid` handling in this environment.
- Recommended foreground debug run:

```bash
RUST_LOG=info,exoclaw::gateway::protocol=debug,exoclaw::gateway::server=debug,exoclaw::agent::providers=debug cargo run -- gateway 2>&1 | tee /tmp/exoclaw.log
```

- Recommended detached run:

```bash
setsid nohup env NO_COLOR=true RUST_LOG=info,exoclaw::gateway::protocol=debug,exoclaw::gateway::server=debug,exoclaw::agent::providers=debug cargo run -- gateway </dev/null >/tmp/exoclaw.log 2>&1 &
```

## Size / Scope Snapshot

- `target/release/exoclaw`: ~23 MB
- `target/debug/exoclaw`: ~365 MB
- exoclaw code footprint (current rough local count): ~9k LOC
- openclaw local source footprint (rough local count): ~603k LOC

Debug-vs-release size spread is expected for Rust due to symbols, debug info, and lower optimization in dev profile.

## Security/Architecture Assessment Outcome

- Direction is strong:
- Rust runtime
- capability-scoped WASM plugins
- loopback-by-default gateway
- token auth on non-loopback bind

- Not yet a full production security claim:
- TLS termination, advanced observability, and additional control layers still needed.

- Added roadmap:
- `SECURITY_ROADMAP.md` with P0/P1/P2 controls, definitions of done, CI enforcement policy, and 30/60/90 day milestones.

## Recommended Next Steps

- Implement P0 controls from `SECURITY_ROADMAP.md`:
- websocket origin and payload-size limits
- rate limiting / abuse controls
- structured security audit events

- Make security checks branch-protection required alongside current test suite.

