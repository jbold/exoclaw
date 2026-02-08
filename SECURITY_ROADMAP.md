# Security Roadmap

Last updated: 2026-02-08

This roadmap is the shortest path from "secure architecture" to "defensible production security posture" for exoclaw.

## Current Baseline

- Strong foundations already in place:
- WASM plugin isolation and capability scoping (`src/sandbox/mod.rs`).
- Loopback default bind and token requirement on non-loopback (`src/gateway/server.rs`).
- Constant-time token comparison (`src/gateway/auth.rs`).
- Secure credential file permissions (`src/secrets.rs`, `src/fs_util.rs`).
- CI test/security jobs for Rust tests, wasm UI tests, E2E, and dependency checks (`.github/workflows/test-suite.yml`).

- Known gaps before production claim:
- TLS termination not implemented in gateway.
- OpenTelemetry and security observability still pending.
- No formal rate limiting/abuse throttling layer.
- No plugin signature verification or trusted plugin provenance policy.
- No explicit RBAC scope model for RPC methods.

## Target Posture

- P0: Safe private beta on trusted networks.
- P1: Internet-exposed beta with strong guardrails.
- P2: Production-grade posture for enterprise-style review.

## P0 Controls (Ship First)

- Transport and exposure:
- Require TLS termination at ingress for any non-loopback deployment.
- Require token auth for all non-loopback binds with startup hard-fail.
- Add WebSocket origin allowlist for browser clients.

- Auth hardening:
- Support token rotation with active+next token validation window.
- Add max token length and strict JSON shape checks on connect.
- Add auth failure cooldown per source IP/session key.

- Abuse prevention:
- Add request body/message size limits.
- Add per-IP and per-session rate limits for `chat.send`.
- Add connection and in-flight stream caps to prevent memory exhaustion.

- Security logging:
- Emit structured security audit events for auth pass/fail, plugin denied capability, rate-limit hits, and stream timeout.
- Include request id, session key, remote address, and method name in every audit event.
- Add panic hook + fatal event breadcrumb in logs.

- CI security gates:
- Keep existing `cargo audit` and `cargo deny`.
- Add secret scanning gate (for example `gitleaks`) on pull requests.
- Add fuzz/safety tests for JSON-RPC parsing and SSE stream parser edge cases.

- Definition of done for P0:
- All non-loopback deployments run behind TLS ingress with auth token.
- PRs fail on secrets, dependency CVEs, and security test regressions.
- Malformed/oversized websocket payloads are rejected safely and tested.

## P1 Controls (Internet Beta)

- Plugin trust and supply chain:
- Enforce plugin signing (signature + digest) before load.
- Maintain allowlist of trusted plugin publishers.
- Persist plugin provenance metadata and verify at startup.

- Authorization model:
- Introduce method-level scopes (for example `chat:send`, `plugin:list`, `admin:*`).
- Require explicit scope binding per token.
- Add deny-by-default policy for admin/control methods.

- Data and secret hardening:
- Encrypt persisted session/history store at rest once DB persistence lands.
- Move API key handling to OS keyring option for local mode.
- Add key rotation command with audit trail.

- Runtime hardening:
- Add bounded queues and backpressure instrumentation for hot paths.
- Add circuit breakers around provider calls and webhook adapters.
- Add host egress allowlist mode for outbound HTTP.

- CI/CD hardening:
- Generate SBOM on every release artifact.
- Add release signing and provenance attestation.
- Block merge without passing security checks and at least one human review.

- Definition of done for P1:
- Only signed plugins can load in default mode.
- Tokens are scoped and least-privilege by default.
- Release artifacts are reproducible, signed, and accompanied by SBOM.

## P2 Controls (Production / Enterprise)

- Advanced controls:
- Optional mTLS for service-to-service traffic in distributed deployments.
- WAF/edge policy templates for exposed gateway endpoints.
- Region-aware key management and secret escrow policy.

- Detection and response:
- OpenTelemetry traces + metrics + logs with SIEM export path.
- Alerting playbooks for auth abuse, token anomalies, plugin denial spikes, and provider failure storms.
- Incident response runbook and recovery drill cadence.

- Resilience and compliance:
- Backup/restore procedures for persistent state.
- Data retention and deletion policy with automated enforcement.
- Security chaos testing and regular threat-model refresh.

- Definition of done for P2:
- Incident MTTR and detection SLIs are tracked and stable.
- Security controls are continuously tested in CI and in staging drills.
- External review can trace controls from policy to code to evidence.

## Coverage and Quality Targets

- Line coverage:
- Keep minimum backend coverage gate at 70% now.
- Raise to 80% after P1 controls land.
- Raise to 85% for security-critical modules (`gateway/auth`, `gateway/protocol`, `sandbox`, `agent/providers`).

- Must-have security test classes:
- Auth bypass and malformed handshake cases.
- JSON-RPC parser robustness and type confusion cases.
- SSE framing/parser fuzz tests (CRLF/LF/CR variants, truncation, long frames).
- Plugin capability denial and sandbox escape regression tests.
- Rate-limit and queue-exhaustion behavior tests.

## CI/CD Enforcement Policy

- Required PR checks:
- `Rust + Coverage`
- `Dependency Security`
- `WASM UI Tests`
- `Playwright E2E`
- `Secrets Scan`
- `Security Regression Tests`

- Branch protection:
- No direct push to `main`.
- No bypass merge except repository admin emergency policy.
- Require at least one code review approval.

## Module-by-Module Implementation Plan

- `src/gateway/server.rs`:
- Add websocket origin checks, body size guards, connection caps, and rate-limit middleware.
- Add structured remote peer extraction and audit event emission.

- `src/gateway/auth.rs`:
- Add rotating token set support and strict handshake schema validation.
- Add explicit auth error codes for observability and client UX.

- `src/gateway/protocol.rs`:
- Add method scope checks and deny-by-default for privileged methods.
- Add central request validation and max payload constraints.

- `src/sandbox/mod.rs`:
- Add plugin signature verification and trusted publisher policy checks.
- Log capability denials with stable event codes.

- `src/agent/providers.rs`:
- Add circuit breaker state and retry budget boundaries.
- Emit security/abuse telemetry around timeout and failure patterns.

- `.github/workflows/test-suite.yml`:
- Add `secrets-scan` and `security-regressions` jobs as required checks.
- Publish security test artifacts for failed runs.

## 30/60/90 Day Execution

- By 2026-03-10:
- Complete all P0 controls and required tests.
- Enforce branch protection with full required checks.

- By 2026-04-09:
- Complete P1 plugin signing, method scopes, and release provenance.
- Raise coverage target to 80%.

- By 2026-05-09:
- Complete P2 telemetry + incident response + resilience drills.
- Publish a security posture report mapped to this roadmap.
