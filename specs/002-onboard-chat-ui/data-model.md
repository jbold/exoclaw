# Data Model: Onboarding & Built-in Chat UI

**Feature**: `002-onboard-chat-ui`
**Date**: 2026-02-08

## Entities

### Credential File

A filesystem artifact, not a runtime data structure.

| Attribute | Type | Description |
|-----------|------|-------------|
| provider | string | `"anthropic"` or `"openai"` — determines filename |
| api_key | string | Raw API key value (plaintext) |
| path | filesystem path | `~/.exoclaw/credentials/{provider}.key` |
| permissions | unix mode | `0600` (owner read/write only) |
| parent_dir_permissions | unix mode | `0700` (owner only) |

**Lifecycle**:
- Created by `exoclaw onboard`
- Overwritten by re-running `exoclaw onboard`
- Read at gateway startup by `config::resolve_api_key()`
- Never read by the chat UI or any WASM plugin

**Uniqueness**: One file per provider. Re-onboarding overwrites.

### Chat UI State (Browser-side, Leptos Signals)

Reactive state managed by Leptos in the browser. Not persisted.

| Signal | Type | Description |
|--------|------|-------------|
| messages | `RwSignal<Vec<ChatMessage>>` | Conversation history displayed in the UI |
| input_text | `RwSignal<String>` | Current text in the message input field |
| is_streaming | `RwSignal<bool>` | Whether an LLM response is currently streaming |
| is_connected | `RwSignal<bool>` | WebSocket connection status |
| ws_connection | `Option<WebSocket>` | Active gloo-net WebSocket handle |
| auth_token | `RwSignal<Option<String>>` | Token entered by user (if auth required) |
| needs_auth | `RwSignal<bool>` | Whether to show the auth token prompt |

### ChatMessage (UI-side)

| Field | Type | Description |
|-------|------|-------------|
| role | enum | `User` or `Assistant` or `Error` |
| content | String | Raw text (user) or accumulated markdown (assistant) |
| is_complete | bool | Whether streaming has finished for this message |

**State transitions**:
1. User sends message → `ChatMessage { role: User, content: input, is_complete: true }` added
2. Stream starts → `ChatMessage { role: Assistant, content: "", is_complete: false }` added
3. Tokens arrive → `content` appended token-by-token
4. Stream ends → `is_complete = true`
5. Error occurs → `ChatMessage { role: Error, content: error_msg, is_complete: true }` added

## Relationships

```
Credential File ──reads──► config::resolve_api_key() ──provides──► AgentRunner
                                                                        │
Chat UI (Leptos) ──WebSocket──► /ws ──JSON-RPC──► gateway::protocol ────┘
    │                                                    │
    └── ChatMessage[]                                    └── Session (001-core-runtime)
```

- The Chat UI and Credential File are completely independent — the UI never reads credentials
- The Chat UI connects to the same `/ws` endpoint as any other WebSocket client
- Server-side session state (from 001-core-runtime) persists across UI page refreshes
- The UI's `messages` signal is ephemeral — lost on refresh

## Data Flow: Onboarding

```
User runs `exoclaw onboard`
    │
    ├─► prompt: provider selection (default: anthropic)
    ├─► prompt: API key (hidden input via rpassword)
    │
    ├─► validate: key non-empty
    │
    ├─► write: ~/.exoclaw/credentials/{provider}.key  (0600)
    ├─► write: ~/.exoclaw/config.toml                 (0600)
    │       └── contains: provider, model, NO api_key
    │
    └─► print: next steps ("cargo run -- gateway")
```

## Data Flow: Chat UI Message

```
User types message, presses Enter
    │
    ├─► [if !is_connected] open WebSocket to /ws
    │       └─► [if needs_auth] send {"token": "..."} as first message
    │
    ├─► set is_streaming = true, disable input
    ├─► add User message to messages[]
    ├─► send JSON-RPC: {"jsonrpc":"2.0","method":"chat.send","params":{"content":"..."}}
    │
    ├─► add empty Assistant message to messages[]
    │
    ├─► receive stream events:
    │       ├── {"event":"text","data":"token"} → append to assistant message content
    │       ├── {"event":"error","data":"..."} → add Error message
    │       └── {"event":"done"} → mark assistant message complete
    │
    └─► set is_streaming = false, re-enable input
```
