# Data Model: Exoclaw Core Runtime

**Branch**: `001-core-runtime` | **Date**: 2026-02-08
**Input**: Key Entities from [spec.md](spec.md), existing scaffold code, [plan.md](plan.md)

## Entity Relationship Overview

```text
                    ┌──────────────┐
                    │   AgentDef   │
                    │  (config)    │
                    └──────┬───────┘
                           │ 1:N
                    ┌──────▼───────┐
                    │   Binding    │
                    │  (routing)   │
                    └──────┬───────┘
                           │ resolves to
                    ┌──────▼───────┐
              ┌─────│   Session    │─────┐
              │     │  (state)     │     │
              │     └──────┬───────┘     │
              │            │             │
        ┌─────▼────┐  ┌───▼─────┐  ┌───▼────────┐
        │ Episodic │  │Semantic │  │   Token     │
        │ Memory   │  │ Memory  │  │   Budget    │
        │ (turns)  │  │(entities│  │  (metering) │
        └──────────┘  │  graph) │  └────────────┘
                      └─────────┘

        ┌──────────────┐    ┌───────────────┐
        │   Plugin     │    │   Soul        │
        │  (WASM tool) │    │  (personality)│
        └──────────────┘    └───────────────┘
```

## Entities

### AgentDef

An LLM-backed assistant configuration. Defines which provider/model to use, what tools are available, and the agent's personality.

```rust
struct AgentDef {
    id: String,                    // Unique agent identifier (e.g., "personal", "work")
    provider: String,              // "anthropic" | "openai"
    model: String,                 // "claude-sonnet-4-5-20250929", "gpt-4o", etc.
    api_key: String,               // Provider API key (host-only, never exposed to plugins)
    max_tokens: u32,               // Max tokens per LLM response (default: 4096)
    system_prompt: Option<String>, // Optional system prompt override
    soul_path: Option<String>,     // Path to soul document (personality/instructions)
    tools: Vec<String>,            // Plugin names this agent can use (e.g., ["echo", "web-search"])
    fallback: Option<Box<AgentDef>>, // Fallback provider if primary fails
}
```

**Source**: config.toml `[agent]` section + `[agent.fallback]`
**Existing code**: `AgentConfig` in `src/agent/mod.rs:18-23` (will be expanded)

**Validation**:
- `id` MUST be non-empty, alphanumeric + hyphens
- `provider` MUST be one of the supported providers
- `api_key` MUST be non-empty (can come from env var)
- `max_tokens` MUST be > 0 and <= provider limit
- `tools` MUST reference registered plugin names

---

### Binding

A routing rule that maps a channel/account/peer/guild/team pattern to a specific agent.

```rust
struct Binding {
    agent_id: String,              // Target agent for this binding
    channel: Option<String>,       // Channel name (e.g., "telegram", "whatsapp", "websocket")
    account_id: Option<String>,    // Account within channel
    peer_id: Option<String>,       // Specific peer (user/group)
    guild_id: Option<String>,      // Guild/server within channel
    team_id: Option<String>,       // Team within guild
}
```

**Source**: config.toml `[[bindings]]` array
**Existing code**: `Binding` in `src/router/mod.rs:14-26` (already implemented)

**Resolution Priority** (highest to lowest):
1. `peer_id` match
2. `guild_id` match
3. `team_id` match
4. `account_id` match (with no peer/guild)
5. `channel` match (with no account/peer)
6. Default agent

**Validation**:
- `agent_id` MUST reference a defined agent
- At least one of {channel, account_id, peer_id, guild_id, team_id} MUST be set

---

### Session

A conversation thread between a user and an agent. Contains episodic memory and references to semantic memory.

```rust
struct Session {
    key: String,                   // "{agent_id}:{channel}:{account}:{peer}"
    agent_id: String,              // Resolved agent for this session
    messages: Vec<Message>,        // Conversation history (episodic memory source)
    created_at: DateTime<Utc>,     // Session creation timestamp
    updated_at: DateTime<Utc>,     // Last activity timestamp
    message_count: u64,            // Total messages in session
    token_usage: TokenUsage,       // Cumulative token usage for this session
}
```

**Source**: spec FR-005, FR-006
**Existing code**: `Session` in `src/store/mod.rs:13-19` (will be expanded)

**Key format**: `{agent_id}:{channel}:{account}:{peer}` (peer defaults to "main" if absent)

**State Transitions**:
- Created on first message (lazy initialization)
- Updated on each message processed
- Never deleted (historical record), but episodic window slides

**Concurrency**: Messages MUST be serialized per-session (FR-006). Implementation via per-session `tokio::sync::Mutex` or channel-based lane concurrency.

---

### Message

A single turn in a conversation. Part of a session's episodic memory.

```rust
struct Message {
    role: String,                  // "user" | "assistant" | "tool_result"
    content: MessageContent,       // Text, tool use, or tool result
    timestamp: DateTime<Utc>,      // When the message was processed
    token_count: Option<u32>,      // Tokens consumed (set after LLM response)
    metadata: Option<Value>,       // Provider-specific metadata
}

enum MessageContent {
    Text(String),
    ToolUse {
        id: String,                // Tool call ID (from LLM)
        name: String,              // Plugin name
        input: Value,              // Tool input arguments
    },
    ToolResult {
        tool_use_id: String,       // References the ToolUse.id
        content: String,           // Tool execution output
        is_error: bool,            // Whether the tool returned an error
    },
}
```

**Source**: spec FR-007, FR-008
**Existing code**: `ChatMessage` in `src/agent/mod.rs:26-29` (will be expanded), `AgentEvent` in `src/agent/mod.rs:33-42`

---

### Plugin

A WASM module that implements tool execution or channel protocol translation.

```rust
struct PluginDef {
    name: String,                  // Unique plugin name (e.g., "echo", "telegram")
    path: String,                  // Path to .wasm file
    capabilities: Vec<Capability>, // Granted capabilities
    description: Option<String>,   // Human-readable description (from plugin's describe())
    plugin_type: PluginType,       // Tool or channel adapter
}

enum PluginType {
    Tool,                          // Implements handle_tool_call
    ChannelAdapter,                // Implements parse_incoming + format_outgoing
}

enum Capability {
    Http(String),                  // HTTP access to specific host (e.g., "api.telegram.org")
    Store(String),                 // Host storage access (e.g., "sessions")
    HostFunction(String),          // Named host function access
}
```

**Source**: config.toml `[[plugins]]` array, spec FR-010 through FR-013
**Existing code**: `PluginEntry` in `src/sandbox/mod.rs:16-19` (will be expanded)

**Plugin WASM Interface** (functions the plugin exports):
- `handle_tool_call(input: JSON) -> JSON` — Execute a tool call
- `parse_incoming(payload: bytes) -> JSON` — Parse platform message to normalized format
- `format_outgoing(response: JSON) -> bytes` — Format response for platform
- `describe() -> JSON` — Return plugin metadata (name, description, tool schemas)

**Validation**:
- WASM binary MUST be valid (trial instantiation at load time, FR-013)
- `capabilities` MUST be parseable (format: `type:value`)
- Plugin name MUST be unique

---

### MemoryEntity

A fact, relationship, or attribute extracted from conversation. Stored in the semantic memory layer.

```rust
struct MemoryEntity {
    id: String,                    // Unique entity ID (generated)
    entity_type: String,           // "fact" | "relationship" | "attribute"
    subject: String,               // What the entity is about (e.g., "user")
    predicate: String,             // The relationship (e.g., "dog_name", "location")
    object: String,                // The value (e.g., "Luna", "LA")
    session_key: String,           // Session where this was learned
    learned_at: DateTime<Utc>,     // When the fact was first learned
    superseded_at: Option<DateTime<Utc>>, // When replaced by a newer fact
    superseded_by: Option<String>, // ID of the entity that replaced this one
    confidence: f32,               // Extraction confidence (0.0-1.0)
}
```

**Source**: spec FR-015, FR-017, US4 acceptance scenarios
**Existing code**: None (new module: `src/memory/semantic.rs`)

**Key behaviors**:
- Facts are never deleted, only superseded (US4 scenario 4)
- Temporal metadata enables "when did I learn this?" queries
- Graph traversal finds related entities for context assembly
- Superseded facts remain queryable for historical context

---

### TokenBudget

A configurable spending limit for LLM API calls.

```rust
struct TokenBudget {
    scope: BudgetScope,            // What this budget covers
    limit: u64,                    // Maximum tokens allowed
    used: u64,                     // Tokens consumed so far
    period_start: DateTime<Utc>,   // When the current period started
}

enum BudgetScope {
    Session(String),               // Per-session key
    Agent(String),                 // Per-agent ID
    Daily,                         // All usage today (resets at midnight UTC)
    Monthly,                       // All usage this month (resets on 1st)
}
```

**Source**: spec FR-019, US3 acceptance scenarios
**Existing code**: None (new module: `src/agent/metering.rs`)

---

### TokenRecord

An audit log entry for a single LLM API call.

```rust
struct TokenRecord {
    timestamp: DateTime<Utc>,      // When the call was made
    session_key: String,           // Which session
    agent_id: String,              // Which agent
    provider: String,              // "anthropic" | "openai"
    model: String,                 // Model identifier
    input_tokens: u32,             // Tokens sent to LLM
    output_tokens: u32,            // Tokens received from LLM
    cost_estimate_usd: f64,        // Estimated cost in USD
}
```

**Source**: spec FR-018, FR-020
**Existing code**: None (new module: `src/agent/metering.rs`)

---

### Soul

Agent personality and instructions document. Always included in context assembly.

```rust
struct Soul {
    agent_id: String,              // Which agent this soul belongs to
    content: String,               // Full soul document text
    token_count: u32,              // Pre-computed token count (~500 tokens target)
    loaded_from: String,           // File path for hot-reload
    loaded_at: DateTime<Utc>,      // When last loaded
}
```

**Source**: spec FR-016, US4 scenario 2
**Existing code**: None (new module: `src/memory/soul.rs`)

---

## Configuration Schema

Maps config.toml sections to entities:

```toml
[gateway]               → Gateway startup config (not an entity, runtime only)
[agent]                 → AgentDef (primary)
[agent.fallback]        → AgentDef (fallback, nested)
[[plugins]]             → PluginDef (one per plugin)
[[bindings]]            → Binding (one per routing rule)
[budgets]               → TokenBudget defaults (new section)
[budgets.session]       → Per-session token limit
[budgets.daily]         → Daily token limit
[budgets.monthly]       → Monthly token limit
```

## Index Strategy (Future: SurrealDB)

| Entity | Primary Key | Indexes |
|--------|------------|---------|
| Session | `key` (composite) | `agent_id`, `updated_at` |
| Message | `session_key` + `timestamp` | `role`, `timestamp` |
| MemoryEntity | `id` | `subject`, `predicate`, `session_key`, `learned_at`, `superseded_at IS NULL` |
| TokenRecord | `timestamp` | `session_key`, `agent_id`, `provider`, date partition |
| TokenBudget | `scope` | `period_start` |
