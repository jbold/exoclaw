# Tasks: Exoclaw Core Runtime

**Input**: Design documents from `/specs/001-core-runtime/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/jsonrpc-spec.md

**Tests**: Included. The spec defines test files and the constitution mandates tests for all public APIs.

**Organization**: Tasks are grouped by user story (US1-US5) to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- All paths are relative to repository root

---

## Phase 1: Setup

**Purpose**: Config loading, test infrastructure, and shared types that all user stories depend on

- [ ] T001 Create config module with TOML deserialization structs in `src/config.rs` — sections: `GatewayConfig`, `AgentDefConfig`, `PluginConfig`, `BindingConfig`, `BudgetConfig`. Load from `~/.exoclaw/config.toml` or `EXOCLAW_CONFIG` env var. Validate at startup with clear error messages. Zero-config fallback when no file exists (FR-021, FR-022, FR-023)
- [ ] T002 [P] Create shared message types in `src/types.rs` — `Message` enum (`Text`, `ToolUse`, `ToolResult`), `AgentMessage` (normalized format from data-model.md), `StreamEvent` enum (`Text`, `ToolUse`, `ToolResult`, `Usage`, `Done`, `Error`) matching contracts/jsonrpc-spec.md streaming response format
- [ ] T003 [P] Create `examples/config.toml` reference config with all sections documented — gateway, agent, agent.fallback, budgets (session/daily/monthly), [[plugins]] with capabilities, [[bindings]] with routing rules. Replace current stub with complete example matching config.rs structs
- [ ] T004 Integrate config loading into `src/main.rs` — load config before gateway startup, pass typed config to `gateway::run()`, populate `SessionRouter` with bindings from config, populate `PluginHost` with plugins from config. Update `AppState` to hold `AgentDefConfig` and `SessionStore`

**Checkpoint**: `cargo build` succeeds. `cargo run -- gateway` loads config (or uses defaults). Config errors produce clear messages.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before any user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

- [ ] T005 Refactor `AppState` in `src/gateway/server.rs` — add `SessionStore` (wrapped in `Arc<RwLock<>>`), `AgentRunner`, and `AgentDefConfig` to `AppState`. Update `run()` to accept the typed `GatewayConfig` from config.rs instead of the current inline `Config` struct
- [ ] T006 [P] Create LLM provider trait abstraction in `src/agent/providers.rs` — `trait LlmProvider` with `async fn call_streaming(&self, messages, tools, tx) -> Result<()>`. Implement `AnthropicProvider` and `OpenAiProvider` extracting logic from current `run_anthropic()`/`run_openai()` in `src/agent/mod.rs`. Add proper SSE event type parsing for both `content_block_delta` (Anthropic) and `chat.completion.chunk` (OpenAI)
- [ ] T007 [P] Add `chat.send` request params struct to `src/gateway/protocol.rs` — `ChatSendParams { channel, account, peer, content, guild, team }` per contracts/jsonrpc-spec.md. Parse from `req.params` in the `chat.send` match arm. Return typed validation errors for missing required fields
- [ ] T008 [P] Write unit tests for auth module in `tests/auth_test.rs` — test `verify_connect()` with valid token, invalid token, no token configured (loopback mode), malformed JSON first message, empty token string. Uses the existing `src/gateway/auth.rs` (FR-002)
- [ ] T009 [P] Write unit tests for router module in `tests/router_test.rs` — test binding resolution priority: peer > guild > team > account > channel > default. Test session key format `{agent_id}:{channel}:{account}:{peer}`. Test session creation on first message. Test session reuse on subsequent messages. Test default agent fallback (FR-004, FR-005)

**Checkpoint**: `cargo test` passes. `AppState` holds all necessary components. Provider trait compiles. `chat.send` params parse correctly.

---

## Phase 3: User Story 1 — Send a Message and Get an AI Response (Priority: P1) MVP

**Goal**: A user sends a text message via WebSocket and receives a streamed AI response. The message is routed to the correct agent, the agent calls an LLM, and the response streams back token-by-token.

**Independent Test**: Connect via WebSocket, authenticate, send `chat.send`, receive streamed text chunks, verify response completes.

**Functional Requirements**: FR-001, FR-002, FR-003, FR-004, FR-005, FR-006, FR-007, FR-009 (partial)

### Tests for User Story 1

- [ ] T010 [P] [US1] Write protocol dispatch tests in `tests/protocol_test.rs` — test `ping` returns `pong`, `status` returns version/plugins/sessions, `chat.send` with valid params triggers agent run, `chat.send` with missing params returns error, unknown method returns error. Mock `AppState` with in-memory router and plugin host
- [ ] T011 [P] [US1] Write gateway integration test in `tests/gateway_test.rs` — start gateway on random port, connect WebSocket, send auth token, send `chat.send`, verify streaming response format matches contracts/jsonrpc-spec.md (`event: text`, `event: done`). Use a mock LLM provider that returns fixed text. Test unauthenticated rejection. Test loopback no-auth mode

### Implementation for User Story 1

- [ ] T012 [US1] Wire `chat.send` end-to-end in `src/gateway/protocol.rs` — replace `{"queued": true}` stub with: parse `ChatSendParams` → call `state.router.resolve()` → create/get session from `state.store` → spawn `AgentRunner::run()` on tokio task → stream `AgentEvent`s back as JSON response chunks per contracts/jsonrpc-spec.md streaming format. Return the `mpsc::Receiver` handle to the WebSocket write loop
- [ ] T013 [US1] Update WebSocket message loop in `src/gateway/server.rs` — after `handle_rpc()` returns for `chat.send`, read from the `mpsc::Receiver<AgentEvent>` and send each event as a WebSocket text frame. Format: `{"id": req_id, "event": "text", "data": "chunk"}`. Send `{"id": req_id, "event": "done"}` on completion. Handle client disconnect mid-stream gracefully (drop receiver)
- [ ] T014 [US1] Implement per-session message serialization in `src/gateway/server.rs` — use a `HashMap<String, tokio::sync::Mutex<()>>` keyed by session_key to serialize concurrent messages to the same session. Acquire lock before processing, release after response complete (FR-006)
- [ ] T015 [US1] Integrate `SessionStore` into the agent loop in `src/agent/mod.rs` — before calling the LLM, load conversation history from `SessionStore` for the session key. After LLM response, append both the user message and assistant response to the store. Pass full message history to provider (will be replaced by memory engine in US4)
- [ ] T016 [US1] Implement `AnthropicProvider` in `src/agent/providers.rs` — extract and refine SSE parsing from current `run_anthropic()`. Handle event types: `message_start` (extract message id), `content_block_start`, `content_block_delta` (extract text delta), `message_delta` (extract `stop_reason`, `usage`), `message_stop`. Parse `usage.input_tokens` and `usage.output_tokens` from `message_delta`. Send `AgentEvent::Usage` with token counts
- [ ] T017 [P] [US1] Implement `OpenAiProvider` in `src/agent/providers.rs` — extract and refine SSE parsing from current `run_openai()`. Handle `choices[0].delta.content` for text chunks. Handle `choices[0].finish_reason == "stop"` for completion. Parse `usage` from final chunk (if present) or from non-streaming fallback. Send `AgentEvent::Usage`
- [ ] T018 [US1] Refactor `AgentRunner::run()` in `src/agent/mod.rs` — replace inline `run_anthropic`/`run_openai` with `LlmProvider` trait dispatch. Construct the appropriate provider from `AgentDefConfig`. Add `AgentEvent::Usage { input_tokens, output_tokens }` variant to the `AgentEvent` enum. Add system prompt support (prepend to messages)
- [ ] T019 [US1] Add error handling for LLM provider failures in `src/agent/mod.rs` — connection refused → `AgentEvent::Error("provider unreachable")`. HTTP 4xx/5xx → parse error body and forward. Timeout → `AgentEvent::Error("provider timeout")`. All errors terminate the stream with `Done` event. Session state remains consistent (no partial writes)

**Checkpoint**: `cargo test` passes. A WebSocket client can send `chat.send` and receive streamed LLM responses. Router resolves bindings correctly. Sessions persist across messages. Auth works for both loopback and token modes.

---

## Phase 4: User Story 2 — Execute Tools in WASM Sandbox (Priority: P2)

**Goal**: When the LLM responds with a tool-use request, the system dispatches it to a WASM plugin, feeds the result back to the LLM, and continues until a final text response.

**Independent Test**: Load echo plugin, trigger a tool call, verify WASM execution and result fed back to LLM.

**Functional Requirements**: FR-010, FR-011, FR-012, FR-013

**Depends on**: US1 (agent loop must work before adding tool dispatch)

### Tests for User Story 2

- [ ] T020 [P] [US2] Write sandbox integration tests in `tests/sandbox_test.rs` — build echo plugin to WASM, load via `PluginHost::register()`, call `handle_tool_call` with JSON input, verify output. Test capability grants: create manifest with `allowed_hosts`, verify HTTP to unlisted host is denied. Test invalid WASM binary rejection at load time. Test fresh instance per invocation (no state leakage between calls)

### Implementation for User Story 2

- [ ] T021 [US2] Create capability parsing module in `src/sandbox/capabilities.rs` — parse capability strings from config (e.g., `"http:api.telegram.org"`, `"store:sessions"`) into typed `Capability` enum. Map capabilities to Extism `Manifest` settings: `allowed_hosts` for HTTP, host function registration for store access. Reject unknown capability types at config validation time
- [ ] T022 [US2] Update `PluginHost::register()` in `src/sandbox/mod.rs` — accept `Vec<Capability>` parameter. Configure `Manifest` with `allowed_hosts` from HTTP capabilities. Add `PluginType` (Tool vs ChannelAdapter) to `PluginEntry`. Store tool schemas (from plugin's `describe()` export if available). Validate WASM binary at load time via trial instantiation (FR-013)
- [ ] T023 [US2] Update `PluginHost::call()` in `src/sandbox/mod.rs` — create fresh `Plugin` instance per invocation with capabilities applied (FR-012). Add execution timeout via `tokio::time::timeout()`. Return structured result: `ToolCallResult { content: String, is_error: bool }`. Catch WASM traps and convert to error results without crashing the host
- [ ] T024 [US2] Implement tool-use loop in `src/agent/mod.rs` — after receiving `AgentEvent::ToolUse { id, name, input }` from provider: (1) look up plugin by name in `PluginHost`, (2) call `handle_tool_call` with JSON-serialized input, (3) construct tool result message, (4) append tool_use + tool_result to message history, (5) call LLM again with updated history, (6) repeat until `AgentEvent::Text` or max iterations reached. Send `AgentEvent::ToolUse` and `AgentEvent::ToolResult` events to the stream so the client can observe tool execution
- [ ] T025 [US2] Parse tool_use blocks from LLM responses in `src/agent/providers.rs` — Anthropic: parse `content_block_start` with `type: "tool_use"`, accumulate `input_json_delta` across `content_block_delta` events, emit `AgentEvent::ToolUse` on `content_block_stop`. OpenAI: parse `tool_calls` array in delta, accumulate `function.arguments` across chunks, emit `AgentEvent::ToolUse` on completion
- [ ] T026 [US2] Add tool schemas to LLM requests in `src/agent/providers.rs` — when tools are available, include tool definitions in the API request body. Anthropic: `tools` array with `name`, `description`, `input_schema`. OpenAI: `tools` array with `type: "function"`, `function: { name, description, parameters }`. Build schemas from loaded plugin metadata
- [ ] T027 [US2] Load plugins from config at gateway startup in `src/main.rs` — iterate `config.plugins`, call `PluginHost::register()` with name, path, and parsed capabilities for each. Log loaded plugins. Skip missing plugin files with a warning (don't crash gateway). Store available tool schemas in `AppState` for use by agent loop

**Checkpoint**: `cargo test` passes (including sandbox tests with real echo plugin WASM). LLM tool calls dispatch to WASM, results feed back, conversation continues. Plugin crashes don't affect the gateway.

---

## Phase 5: User Story 3 — Token Metering and Budget Enforcement (Priority: P3)

**Goal**: Every LLM call is metered. Token counts are logged. Configurable budgets are enforced per-session, per-day, per-month.

**Independent Test**: Configure a per-session budget, send messages until exceeded, verify refusal with budget-exceeded error.

**Functional Requirements**: FR-018, FR-019, FR-020

**Depends on**: US1 (agent loop must exist to meter it)

### Tests for User Story 3

- [ ] T028 [P] [US3] Write metering unit tests in `tests/metering_test.rs` — test token counting from provider response. Test session budget enforcement (allow under budget, refuse over budget). Test daily budget enforcement (accumulate across sessions, refuse when exceeded, reset at midnight UTC). Test monthly budget enforcement. Test token record logging format matches FR-020. Test cost estimation calculation

### Implementation for User Story 3

- [ ] T029 [US3] Create token metering module in `src/agent/metering.rs` — structs: `TokenCounter` (tracks cumulative usage), `TokenBudget` (limit + used + period_start per scope), `TokenRecord` (audit log entry per FR-020). Methods: `check_budget(session_key) -> Result<(), BudgetExceeded>`, `record_usage(session_key, input_tokens, output_tokens, model, provider)`, `get_usage(scope) -> TokenUsage`. Store budgets in-memory with `HashMap<BudgetScope, TokenBudget>`
- [ ] T030 [US3] Implement pre-call budget checking in `src/agent/metering.rs` — before each LLM call, estimate input token count (rough BPE estimation or character-count heuristic). Check against session budget, daily budget, monthly budget. Return `BudgetExceeded { scope, used, limit }` error if any budget would be exceeded. Error includes which budget was hit and current usage vs limit
- [ ] T031 [US3] Implement post-call usage recording in `src/agent/metering.rs` — after each LLM response, extract `input_tokens` and `output_tokens` from provider response (parsed in providers.rs). Create `TokenRecord` with timestamp, session_key, agent_id, provider, model, token counts, and cost estimate. Update cumulative counters for session, daily, monthly scopes. Log the record via `tracing::info!`
- [ ] T032 [US3] Implement cost estimation in `src/agent/metering.rs` — lookup table of per-token prices by provider + model. Calculate `cost_estimate_usd = (input_tokens * input_price + output_tokens * output_price)`. Prices: Anthropic Claude Sonnet input=$3/MTok output=$15/MTok, OpenAI GPT-4o input=$2.50/MTok output=$10/MTok. Make the table configurable (future: load from config)
- [ ] T033 [US3] Integrate metering into agent loop in `src/agent/mod.rs` — before calling provider: `metering.check_budget(session_key)?`. On `BudgetExceeded`: send `AgentEvent::Error("token budget exceeded (session: 9500/10000)")` and return without calling LLM. After provider response: `metering.record_usage(...)`. Send `AgentEvent::Usage { input_tokens, output_tokens }` to client stream
- [ ] T034 [US3] Add budget config to `src/config.rs` — `[budgets]` section with `session: Option<u64>`, `daily: Option<u64>`, `monthly: Option<u64>`. Initialize `TokenCounter` from config at startup. Pass to `AgentRunner`

**Checkpoint**: `cargo test` passes. Token usage is logged for every LLM call. Budgets are enforced. Budget-exceeded returns a clear error instead of calling the LLM.

---

## Phase 6: User Story 4 — Multi-Layer Memory (Priority: P4)

**Goal**: The system maintains episodic, semantic, and soul memory layers. Context is assembled selectively rather than dumping full history. Target: 3-5K tokens per LLM call context.

**Independent Test**: 50+ turn conversation, ask about a fact from 30+ turns ago, verify it's retrieved from semantic memory without including all turns. Assembled context under 5K tokens.

**Functional Requirements**: FR-014, FR-015, FR-016, FR-017

**Depends on**: US1 (agent loop and session store must exist)

### Tests for User Story 4

- [ ] T035 [P] [US4] Write memory module tests in `tests/memory_test.rs` — test episodic: sliding window keeps last N turns, older turns dropped. Test semantic: entity extraction from sample LLM response, entity storage and retrieval, entity supersession (old fact marked superseded, new fact active). Test soul: load from file, always included in context. Test context assembly: verify total token count under 5K for a 50-turn conversation. Test retrieval: fact from turn 5 retrievable at turn 50 via semantic layer

### Implementation for User Story 4

- [ ] T036 [US4] Create memory engine module in `src/memory/mod.rs` — `struct MemoryEngine` with methods: `assemble_context(session_key, query) -> Vec<Message>` (returns soul + relevant entities + recent turns), `process_response(session_key, response)` (extracts entities, appends to episodic). Coordinate across all three memory layers. Target assembled context: 3-5K tokens
- [ ] T037 [P] [US4] Implement episodic memory in `src/memory/episodic.rs` — `struct EpisodicMemory` with sliding window of recent turns per session. Configurable window size (default: last 5 turns, ~1-2K tokens). Methods: `append(session_key, message)`, `recent(session_key, n) -> Vec<Message>`. Oldest turns roll off the window but remain in the session store for semantic extraction
- [ ] T038 [P] [US4] Implement soul document loader in `src/memory/soul.rs` — `struct SoulLoader` loads a markdown file from the path specified in agent config. Methods: `load(path) -> Soul`, `get(agent_id) -> &str`. Pre-compute token count at load time. Target ~500 tokens. Support hot-reload (check file mtime on access, reload if changed)
- [ ] T039 [US4] Implement semantic memory in `src/memory/semantic.rs` — `struct SemanticMemory` stores `MemoryEntity` records (from data-model.md). Methods: `store(entity)`, `query(subject, predicate) -> Vec<MemoryEntity>`, `query_relevant(keywords) -> Vec<MemoryEntity>`, `supersede(old_id, new_entity)`. In-memory storage initially (`HashMap<String, Vec<MemoryEntity>>`). Superseded entities have `superseded_at` set but are not deleted
- [ ] T040 [US4] Implement entity extraction in `src/memory/semantic.rs` — after each LLM response, extract entities/facts/relationships. Strategy: use a simple pattern-based extractor initially (look for "my name is X", "I live in X", "my X is Y" patterns). Future: use a dedicated LLM call for extraction. Create `MemoryEntity` records with `learned_at` timestamp and `confidence` score. Handle entity updates: if entity with same subject+predicate exists, supersede old one
- [ ] T041 [US4] Integrate memory engine into agent loop in `src/agent/mod.rs` — replace direct `SessionStore` message history loading with `MemoryEngine::assemble_context()`. Call `MemoryEngine::process_response()` after each LLM response. Context assembly order: soul (always first) → semantic entities matching query → recent episodic turns → tool schemas. Total target: 3-5K tokens
- [ ] T042 [US4] Add soul and memory config to `src/config.rs` — `soul_path` field on agent config (optional). `[memory]` section with `episodic_window: u32` (default 5), `semantic_enabled: bool` (default true). Pass config to `MemoryEngine` at startup

**Checkpoint**: `cargo test` passes. Context assembly produces ~3-5K tokens for long conversations. Facts from early turns are retrievable via semantic memory. Soul document is always included. Entity updates supersede old values correctly.

---

## Phase 7: User Story 5 — Channel Adapter via WASM Plugin (Priority: P5)

**Goal**: A messaging platform (e.g., Telegram) is connected via a WASM plugin that handles protocol translation. The host manages persistent connections; the plugin handles discrete parse/format events.

**Independent Test**: Load a mock channel adapter plugin, send simulated platform webhook payload, verify plugin parses to normalized `AgentMessage`, response formats back to platform-specific payload.

**Functional Requirements**: FR-010 (channel adapters are WASM), FR-011 (capabilities), FR-012 (isolation)

**Depends on**: US1 (core loop), US2 (WASM sandbox with capabilities)

### Implementation for User Story 5

- [ ] T043 [US5] Define channel adapter plugin interface in `src/sandbox/mod.rs` — channel adapter plugins export: `parse_incoming(payload: bytes) -> JSON` (platform → normalized AgentMessage), `format_outgoing(response: JSON) -> bytes` (normalized → platform format), `describe() -> JSON` (returns channel name, capabilities needed). Add `PluginType::ChannelAdapter` handling to `PluginHost`
- [ ] T044 [US5] Add HTTP webhook endpoint in `src/gateway/server.rs` — `POST /webhook/{channel}` receives platform webhook payloads. Look up channel adapter plugin by channel name. Call `parse_incoming()` to convert to `AgentMessage`. Route through normal agent loop (router → agent → response). Call `format_outgoing()` on the response. Return formatted payload as HTTP response (for platforms that expect synchronous webhook responses)
- [ ] T045 [US5] Implement host-side HTTP proxy for channel adapters in `src/gateway/server.rs` — after `format_outgoing()` returns the platform-specific payload, the host makes the HTTP API call on behalf of the plugin (e.g., `POST https://api.telegram.org/bot{token}/sendMessage`). Plugin never sees API tokens. Use `allowed_hosts` capability to restrict which domains the host will call for this plugin
- [ ] T046 [P] [US5] Create example channel adapter plugin in `examples/mock-channel/` — minimal WASM plugin implementing `parse_incoming` (parse a simple JSON webhook → AgentMessage) and `format_outgoing` (format response → JSON webhook reply). `Cargo.toml` with `extism-pdk`, `crate-type = ["cdylib"]`, target `wasm32-unknown-unknown`. Include build instructions
- [ ] T047 [US5] Write channel adapter integration test in `tests/channel_test.rs` — build mock-channel plugin, load into gateway, POST a simulated webhook payload to `/webhook/mock`, verify plugin parses it, agent processes it, plugin formats the response, HTTP response contains formatted output. Test capability denial: plugin with `http:api.example.com` can't trigger calls to other hosts

**Checkpoint**: `cargo test` passes. A webhook POST triggers the full pipeline: parse → route → agent → format → respond. Channel adapters run in WASM sandbox with capability restrictions.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Performance validation, documentation, cleanup

- [ ] T048 [P] Write config validation tests in `tests/config_test.rs` — test: valid config loads successfully, missing API key returns clear error, invalid provider name returns clear error, missing config file uses defaults, `EXOCLAW_CONFIG` env var overrides default path, malformed TOML returns specific parse error with location
- [ ] T049 [P] Add `criterion` benchmark for router resolution in `benches/router_bench.rs` — benchmark `SessionRouter::resolve()` with 100 bindings, verify < 100us (Constitution V). Benchmark with 1000, 10000 bindings to check scaling
- [ ] T050 [P] Add `criterion` benchmark for WASM plugin instantiation in `benches/sandbox_bench.rs` — benchmark `PluginHost::call()` cold start (fresh instance creation), verify < 1ms (Constitution V). Benchmark with echo plugin
- [ ] T051 [P] Measure release binary size — `cargo build --release` with LTO + strip (already in Cargo.toml profile). Verify < 25MB (Constitution V). If over budget: identify largest dependencies, consider feature-gating optional deps (NATS, etc.)
- [ ] T052 Update `README.md` with installation, configuration, and usage instructions based on `specs/001-core-runtime/quickstart.md`
- [ ] T053 Run `cargo clippy` and `cargo fmt --check` — fix any new warnings introduced during implementation. Only dead-code warnings acceptable during scaffold phase
- [ ] T054 Run full test suite `cargo test` — verify all tests pass. Run with `RUST_LOG=debug` to verify no panics or unexpected error logs
- [ ] T055 Validate quickstart flow end-to-end — follow `specs/001-core-runtime/quickstart.md` on a clean checkout: build, configure, start gateway, send first message via websocat, verify response

**Checkpoint**: All tests pass, all benchmarks meet constitution performance targets, binary under 25MB, quickstart works end-to-end.

---

## Dependencies & Execution Order

### Phase Dependencies

```text
Phase 1 (Setup) ─────────► Phase 2 (Foundational) ─────┐
                                                         │
                                    ┌────────────────────▼─────────────────────┐
                                    │         Phase 3: US1 (P1) MVP            │
                                    │    Core message loop + streaming          │
                                    └──────┬──────────────┬──────────┬─────────┘
                                           │              │          │
                               ┌───────────▼───┐  ┌──────▼────┐  ┌─▼──────────┐
                               │ Phase 4: US2  │  │ Phase 5:  │  │ Phase 6:   │
                               │ Tool exec     │  │ US3 Meter │  │ US4 Memory │
                               └───────┬───────┘  └───────────┘  └────────────┘
                                       │
                               ┌───────▼───────┐
                               │ Phase 7: US5  │
                               │ Channels      │
                               └───────────────┘

                               Phase 8 (Polish): After all desired stories complete
```

### User Story Dependencies

- **US1 (P1)**: Depends on Phase 2 only. No dependencies on other stories. **This is the MVP.**
- **US2 (P2)**: Depends on US1 (needs the agent loop to add tool dispatch to)
- **US3 (P3)**: Depends on US1 (needs the agent loop to add metering to). Independent of US2.
- **US4 (P4)**: Depends on US1 (needs the agent loop and session store). Independent of US2, US3.
- **US5 (P5)**: Depends on US1 + US2 (needs both the core loop and WASM sandbox with capabilities)

### Within Each User Story

- Tests written first (where included), verified to compile
- Data structures before logic
- Core implementation before integration
- Integration before error handling polish
- Story checkpoint validates independently

### Parallel Opportunities

After US1 is complete, US2, US3, and US4 can proceed **in parallel** (they modify different files):
- US2 touches: `sandbox/capabilities.rs`, `sandbox/mod.rs`, `agent/mod.rs` (tool loop), `agent/providers.rs` (tool_use parsing)
- US3 touches: `agent/metering.rs` (new file), `agent/mod.rs` (budget check integration), `config.rs` (budget config)
- US4 touches: `memory/` (all new files), `agent/mod.rs` (context assembly integration), `config.rs` (memory config)

US3 and US4 both modify `agent/mod.rs` at different insertion points (metering wraps the provider call; memory replaces message loading). If done in parallel, one will need a minor merge.

---

## Parallel Example: User Story 1

```text
# After Phase 2 is complete, launch US1 tests in parallel:
T010 [P] Protocol dispatch tests (tests/protocol_test.rs)
T011 [P] Gateway integration test (tests/gateway_test.rs)

# Then launch provider implementations in parallel:
T016 AnthropicProvider (src/agent/providers.rs — Anthropic section)
T017 [P] OpenAiProvider (src/agent/providers.rs — OpenAI section)

# Sequential core wiring:
T012 → T013 → T014 → T015 → T018 → T019
```

## Parallel Example: Post-US1 Stories

```text
# After US1 checkpoint passes, launch three stories in parallel:
Agent A: US2 (T020 → T021 → T022 → T023 → T024 → T025 → T026 → T027)
Agent B: US3 (T028 → T029 → T030 → T031 → T032 → T033 → T034)
Agent C: US4 (T035 → T036 → T037+T038 [P] → T039 → T040 → T041 → T042)

# Then after US2 completes:
Agent D: US5 (T043 → T044 → T045 → T046 [P] → T047)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T004)
2. Complete Phase 2: Foundational (T005-T009)
3. Complete Phase 3: User Story 1 (T010-T019)
4. **STOP and VALIDATE**: WebSocket → chat.send → LLM stream → response works end-to-end
5. This is a working AI chatbot with routing and sessions

### Incremental Delivery

1. Setup + Foundational → Config loads, types compile
2. **US1** → Core chat loop works → **Usable product** (MVP)
3. **US2** → Tool execution works → AI agent (not just chatbot)
4. **US3** → Metering works → Cost-controlled agent
5. **US4** → Memory works → Context-aware agent (3-5K tokens vs 120K)
6. **US5** → Channels work → Multi-platform agent
7. Polish → Benchmarked, documented, validated

Each story adds a layer of capability without breaking previous stories.

---

## Summary

| Phase | Story | Tasks | Parallel Tasks |
|-------|-------|-------|----------------|
| 1. Setup | — | T001-T004 (4) | T002, T003 |
| 2. Foundational | — | T005-T009 (5) | T006, T007, T008, T009 |
| 3. US1 (MVP) | P1 | T010-T019 (10) | T010, T011, T016, T017 |
| 4. US2 Tools | P2 | T020-T027 (8) | T020 |
| 5. US3 Metering | P3 | T028-T034 (7) | T028 |
| 6. US4 Memory | P4 | T035-T042 (8) | T035, T037, T038 |
| 7. US5 Channels | P5 | T043-T047 (5) | T046 |
| 8. Polish | — | T048-T055 (8) | T048, T049, T050, T051 |
| **Total** | | **55 tasks** | **17 parallelizable** |

**Suggested MVP scope**: Phase 1 + Phase 2 + Phase 3 (US1) = **19 tasks** to a working streamed AI chatbot with routing and sessions.

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps each task to its user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate the story independently
- The echo plugin (`examples/echo-plugin/`) already exists and compiles — reuse it for US2 sandbox tests
