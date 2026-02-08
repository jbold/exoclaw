# Feature Specification: Exoclaw Core Runtime

**Feature Branch**: `001-core-runtime`
**Created**: 2026-02-08
**Status**: Draft
**Input**: Capability-gated WASM agent runtime with multi-layer memory, token metering, and multi-channel messaging. A simpler, more secure, cheaper, faster alternative to OpenClaw.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Send a Message and Get an AI Response (Priority: P1)

A user sends a text message to the exoclaw gateway (via WebSocket) and receives a streamed AI response. The message is routed to the correct agent based on channel/account/peer bindings. The agent calls an LLM (Anthropic or OpenAI), streams the response back token-by-token, and the conversation is stored in memory.

**Why this priority**: This is the core value loop. Without send-message-get-response, nothing else matters. Every other feature builds on this.

**Independent Test**: Connect via WebSocket, authenticate, send a `chat.send` JSON-RPC message with text content, receive streamed response chunks, verify the conversation is persisted.

**Acceptance Scenarios**:

1. **Given** a running gateway with an agent configured, **When** an authenticated client sends `chat.send` with a text message, **Then** the system streams the LLM response back as individual text chunks followed by a completion event.
2. **Given** a running gateway with no agent config, **When** a client sends `chat.send`, **Then** the system returns an error indicating no agent is configured.
3. **Given** a running gateway, **When** an unauthenticated client sends `chat.send`, **Then** the connection is rejected with an auth error before any routing occurs.
4. **Given** a conversation with prior messages, **When** the user sends a follow-up message, **Then** the system includes relevant prior context (not the entire history) when calling the LLM.

---

### User Story 2 - Execute Tools in WASM Sandbox (Priority: P2)

When the LLM responds with a tool-use request (e.g., "call the echo plugin with this input"), the system dispatches the tool call to a WASM-sandboxed plugin, feeds the result back to the LLM, and continues the conversation loop until the LLM produces a final text response.

**Why this priority**: Tool execution is what makes an AI agent more than a chatbot. The WASM sandbox is exoclaw's core differentiator — this is where security is proven.

**Independent Test**: Load the echo plugin, send a message that triggers a tool call, verify the tool executes in the WASM sandbox with only granted capabilities, verify the result is fed back to the LLM, and verify the final response reaches the client.

**Acceptance Scenarios**:

1. **Given** an agent with the echo plugin loaded, **When** the LLM responds with a `tool_use` block referencing the echo plugin, **Then** the system calls the plugin's `handle_tool_call` function in a fresh WASM instance and feeds the result back to the LLM.
2. **Given** a plugin with `capabilities = ["http:api.example.com"]`, **When** the plugin attempts to make an HTTP request to `api.other.com`, **Then** the request is denied by the host.
3. **Given** a plugin that crashes or exceeds resource limits, **When** it is invoked, **Then** the host returns an error to the LLM without affecting other sessions or the gateway.
4. **Given** an LLM response with multiple sequential tool calls, **When** processed, **Then** the system executes each tool call, feeds results back, and continues until a final text response is produced.

---

### User Story 3 - Token Metering and Budget Enforcement (Priority: P3)

Every LLM API call is metered at the host level. The system counts input and output tokens from the actual wire data, logs the usage, and enforces configurable budgets. When a budget is exceeded, the system refuses to make further LLM calls and returns a clear error.

**Why this priority**: Cost control is a key differentiator over OpenClaw (which burns $20/day on heartbeats). Without metering, users can't trust the system with real API keys.

**Independent Test**: Configure a per-session token budget, send messages until the budget is exceeded, verify the system refuses the next LLM call with a budget-exceeded error, verify token counts are accurate in the audit log.

**Acceptance Scenarios**:

1. **Given** an agent with a per-session budget of 10,000 tokens, **When** the session has consumed 9,500 tokens and the next message would exceed the budget, **Then** the system returns a budget-exceeded error instead of calling the LLM.
2. **Given** a completed LLM call, **When** the response is received, **Then** the system logs: input tokens, output tokens, model, provider, estimated cost, timestamp, session key.
3. **Given** a per-day budget of 100,000 tokens, **When** the daily total across all sessions reaches the limit, **Then** all subsequent LLM calls are refused until the next day.

---

### User Story 4 - Multi-Layer Memory (Priority: P4)

The system maintains three layers of memory: episodic (recent conversation turns), semantic (extracted entities and relationships), and soul (agent personality/instructions). When assembling context for an LLM call, the system retrieves relevant memories selectively rather than including the entire conversation history.

**Why this priority**: Smart memory is what makes cost-awareness possible. Without selective retrieval, the system falls back to OpenClaw's pattern of dumping everything into the context window.

**Independent Test**: Have a long conversation (50+ turns), then ask a question about something mentioned early in the conversation. Verify the system retrieves the relevant fact from the semantic layer without including all 50 turns in the LLM context. Verify the assembled context is under 5K tokens.

**Acceptance Scenarios**:

1. **Given** a conversation where the user mentioned their dog's name is "Luna" 30 turns ago, **When** the user asks "what's my dog's name?", **Then** the system retrieves the entity "dog: Luna" from the semantic layer and includes it in context without replaying all 30 turns.
2. **Given** a fresh session with no prior conversation, **When** the first message is sent, **Then** the context includes the soul (agent personality) and no episodic or semantic memory.
3. **Given** a conversation, **When** each message is processed, **Then** the system extracts entities and relationships from the LLM response and stores them in the semantic layer with temporal metadata (when the fact was learned).
4. **Given** an updated fact ("I moved from NYC to LA"), **When** the system processes this, **Then** the semantic layer updates the "location" entity to "LA" with the old value "NYC" marked as superseded, not deleted.

---

### User Story 5 - Channel Adapter via WASM Plugin (Priority: P5)

A messaging platform (e.g., Telegram) is connected via a WASM plugin that handles protocol translation. The host manages the persistent connection to the platform (polling, webhooks). The plugin's only job is to parse incoming platform messages into a normalized format and format outgoing responses into platform-specific payloads.

**Why this priority**: Multi-channel is the end-user experience that makes exoclaw useful as a personal assistant. But it depends on all prior stories (message routing, tool execution, memory) being solid first.

**Independent Test**: Load a Telegram channel plugin, simulate an incoming Telegram webhook payload, verify the plugin parses it into a normalized AgentMessage, route it through the agent loop, and verify the response is formatted back into Telegram's expected format.

**Acceptance Scenarios**:

1. **Given** a Telegram plugin loaded with `capabilities = ["http:api.telegram.org"]`, **When** the host receives a Telegram webhook payload, **Then** the plugin's `parse_incoming` function converts it to a normalized AgentMessage.
2. **Given** an agent response to a Telegram user, **When** the response is sent, **Then** the plugin's `format_outgoing` function converts it to Telegram's sendMessage API format, and the host makes the HTTP call on the plugin's behalf.
3. **Given** a channel plugin that attempts to access host memory or filesystem, **When** loaded, **Then** the system refuses to instantiate the plugin due to ungrantable capabilities.

---

### Edge Cases

- What happens when the LLM provider is unreachable? The system returns a connection error to the client without retrying indefinitely; the session state remains consistent.
- What happens when a WASM plugin enters an infinite loop? The host enforces execution time limits; the plugin is terminated and an error is returned to the agent loop.
- What happens when the config file is malformed? The system exits with a clear error message pointing to the specific config error, not a stack trace.
- What happens when two clients send messages to the same session concurrently? Messages are serialized per-session (lane-based concurrency) to prevent race conditions in conversation history.
- What happens when the memory store is corrupted or unavailable? The system falls back to ephemeral in-memory sessions and warns the user; it does not crash.
- What happens when a plugin's WASM binary is invalid? The system rejects it at load time (trial instantiation) with a clear error, not at first invocation.

## Requirements *(mandatory)*

### Functional Requirements

**Gateway & Protocol**

- **FR-001**: System MUST accept WebSocket connections and process JSON-RPC messages (methods: `ping`, `status`, `chat.send`, `plugin.list`).
- **FR-002**: System MUST require a constant-time-compared auth token for non-loopback binds; loopback connections MUST work without authentication.
- **FR-003**: System MUST stream LLM responses back to the client as individual text chunks, not buffered full responses.

**Session Routing**

- **FR-004**: System MUST resolve the target agent for each message using hierarchical binding priority: peer > guild > team > account > channel > default.
- **FR-005**: System MUST maintain session state keyed as `{agent_id}:{channel}:{account}:{peer}` with per-session conversation isolation.
- **FR-006**: System MUST serialize message processing per-session to prevent concurrent writes to the same conversation history.

**Agent Loop**

- **FR-007**: System MUST call LLM providers (Anthropic, OpenAI) via streaming SSE and handle `text`, `tool_use`, and `error` response types.
- **FR-008**: System MUST implement the tool-use loop: dispatch tool calls to WASM plugins, feed results back to the LLM, repeat until a final text response.
- **FR-009**: System MUST assemble context from the memory engine (soul + relevant entities + recent turns) rather than including the full conversation history.

**WASM Sandbox**

- **FR-010**: System MUST execute all tool calls and channel protocol translations in WASM-sandboxed plugin instances.
- **FR-011**: System MUST enforce per-plugin capability grants: allowed HTTP hosts, host function access, execution time limits.
- **FR-012**: System MUST create a fresh WASM instance per invocation for isolation (no shared state between calls).
- **FR-013**: System MUST reject plugins at load time if their WASM binary is invalid (trial instantiation).

**Memory**

- **FR-014**: System MUST maintain episodic memory (recent conversation turns as a sliding window).
- **FR-015**: System MUST extract and store semantic memory (entities, relationships, facts) from conversations with temporal metadata.
- **FR-016**: System MUST support a soul document (agent personality, instructions) that is always included in context.
- **FR-017**: System MUST retrieve relevant context selectively (graph traversal + similarity) rather than dumping full history.

**Token Metering**

- **FR-018**: System MUST count input and output tokens for every LLM API call from the actual wire data.
- **FR-019**: System MUST enforce configurable token budgets (per-session, per-day, per-month) and refuse LLM calls when exceeded.
- **FR-020**: System MUST log token usage with: input tokens, output tokens, model, provider, estimated cost, timestamp, session key.

**Configuration**

- **FR-021**: System MUST load configuration from a single TOML file (`~/.exoclaw/config.toml` or `EXOCLAW_CONFIG` env var).
- **FR-022**: System MUST work with zero configuration for local development (default loopback bind, no auth, no config file required).
- **FR-023**: System MUST validate configuration at startup and report clear errors for invalid values.

### Key Entities

- **Session**: A conversation thread between a user and an agent. Keyed by agent, channel, account, and peer. Contains episodic memory (turns) and references to semantic memory.
- **Agent**: A configured LLM-backed assistant with a specific model, provider, personality (soul), and set of available tools/plugins.
- **Binding**: A routing rule that maps a channel/account/peer/guild/team pattern to a specific agent.
- **Plugin**: A WASM module that implements tool execution or channel protocol translation. Has a name, manifest, and set of granted capabilities.
- **Memory Entity**: A fact, relationship, or attribute extracted from conversation (e.g., "user's dog: Luna", "user location: LA"). Has temporal metadata (when learned, when superseded).
- **Token Budget**: A configurable spending limit for LLM API calls. Scoped to session, agent, day, or month. Tracks cumulative token usage against the limit.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can send a message and receive a streamed AI response within 2 seconds of the first LLM token arriving (runtime overhead under 100ms).
- **SC-002**: The system handles 10,000 concurrent WebSocket connections without degradation on a single machine with 4 cores and 8GB RAM.
- **SC-003**: Tool calls execute in under 5ms of runtime overhead (excluding the tool's own execution time), with WASM instantiation under 1ms.
- **SC-004**: Context assembly for a 50-turn conversation produces a context window under 5,000 tokens (vs. 120,000+ tokens for full-history approaches), while correctly retrieving facts mentioned 30+ turns ago.
- **SC-005**: Token metering is accurate to within 1% of the provider's reported usage.
- **SC-006**: A new user can go from `cargo install exoclaw` to sending their first AI message in under 10 minutes with a single config file.
- **SC-007**: The release binary is a single static file under 25MB that runs on Linux and macOS without external dependencies.
- **SC-008**: A malicious WASM plugin cannot access any host resource (filesystem, network, memory, env vars) beyond its explicitly granted capabilities.
