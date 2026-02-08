# Feature Specification: Onboarding & Built-in Chat UI

**Feature Branch**: `002-onboard-chat-ui`
**Created**: 2026-02-08
**Status**: Draft
**Input**: Secure API key onboarding CLI and built-in web chat interface — two UX essentials missing from the core runtime spec

## Clarifications

### Session 2026-02-08

- Q: Should the chat UI be inline HTML/JS or a Rust/WASM frontend framework? → A: Leptos (Rust/WASM, fine-grained reactivity, axum-native integration via leptos_axum). Renders markdown in LLM responses.
- Q: Should onboarding verify the API key works via a test API call? → A: No — non-empty validation only (Option A). Fast, offline, no network dependency. Bad keys surface on first message.
- Q: What happens when user sends a message while AI is still streaming? → A: Block input — disable send while streaming, re-enable when response finishes (Option B).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - First-Time Onboarding with Secure API Key Entry (Priority: P1)

A new user has just installed exoclaw and wants to start chatting with an AI agent. They run `exoclaw onboard`, choose their LLM provider (Anthropic or OpenAI), and enter their API key. The key is never shown on screen during entry. The system stores the key in a dedicated credential file with restricted file permissions, separate from the main config file. The config file itself never contains the API key — the runtime resolves it at startup from either the credential file or an environment variable.

This is the very first thing a user does after install. It must feel simple, safe, and obvious. A user who completes onboarding should be one command away from a working agent.

**Why this priority**: Without onboarding, a user has to manually create config files and figure out where to put their API key. The core runtime spec (001) assumed env vars, but most users expect a guided setup. This is the front door to the product.

**Independent Test**: Run `exoclaw onboard`, enter a provider and API key, verify the credential file exists with correct permissions, verify the config file is written without the key, then run `exoclaw gateway` and confirm it resolves the key from the credential file.

**Acceptance Scenarios**:

1. **Given** a fresh install with no config file, **When** the user runs `exoclaw onboard`, **Then** the system prompts for provider (defaulting to Anthropic) and API key, writes `~/.exoclaw/config.toml` and `~/.exoclaw/credentials/{provider}.key`, and prints next-step instructions.
2. **Given** onboarding is complete, **When** the user runs `exoclaw gateway`, **Then** the gateway resolves the API key from the credential file without the user setting any environment variable.
3. **Given** a user running onboarding, **When** they type their API key, **Then** the characters are not echoed to the terminal (hidden input).
4. **Given** a completed onboarding, **When** the user inspects `~/.exoclaw/config.toml`, **Then** the file contains provider and model settings but no API key value.
5. **Given** the credential file exists at `~/.exoclaw/credentials/anthropic.key`, **When** the file permissions are checked, **Then** the file is readable only by the owner (mode 0600) and the parent directory is mode 0700.
6. **Given** an existing config from a previous onboarding, **When** the user runs `exoclaw onboard` again, **Then** the system overwrites the credential file and updates the config, preserving any non-onboarding settings (plugins, bindings, budgets).
7. **Given** `ANTHROPIC_API_KEY` is set in the environment, **When** the gateway starts, **Then** the environment variable takes precedence over the credential file.
8. **Given** a user enters an empty or whitespace-only API key, **When** onboarding validates the input, **Then** the system rejects it with a clear error and does not write any files.

---

### User Story 2 - Built-in Web Chat Interface (Priority: P1)

After starting the gateway, the user opens their browser to the gateway's address (e.g., `http://localhost:7200`) and sees a chat interface built with Leptos — a Rust/WASM frontend framework with fine-grained reactivity. They type a message, press Enter, and see the AI response stream in token-by-token with rendered markdown (code blocks, bold, lists, headers). No npm, no JS build tools. The frontend is a Leptos crate compiled to WASM and served by the same axum gateway, built entirely with Rust tooling (`cargo-leptos`).

This is the simplest possible way to verify the system works and have a conversation. It replaces the need for `websocat` or hand-crafted JSON-RPC messages. A user who completes onboarding and starts the gateway can immediately chat through their browser.

**Why this priority**: The core runtime spec (001) defined everything as WebSocket + JSON-RPC, which requires a developer tool to even test. No normal user will hand-write `{"jsonrpc":"2.0","method":"chat.send","params":{"content":"hello"}}`. The built-in chat UI makes SC-006 from 001-core-runtime ("install to first message in under 10 minutes") actually achievable.

**Independent Test**: Start the gateway, open a browser to the gateway URL, type "hello" in the message box, press Enter, verify the response streams in character-by-character with markdown rendering, verify the conversation persists across messages in the same session.

**Acceptance Scenarios**:

1. **Given** a running gateway on localhost:7200, **When** the user navigates to `http://localhost:7200` in a browser, **Then** the system serves the Leptos chat application (HTML shell + WASM bundle), with no external CDN dependencies.
2. **Given** the chat page is loaded, **When** the user types a message and presses Enter (or clicks Send), **Then** the page communicates with the gateway via WebSocket using the existing JSON-RPC protocol (`chat.send`) and displays the streamed response tokens as they arrive.
3. **Given** the gateway requires an auth token (non-loopback bind), **When** the chat page loads, **Then** it prompts the user for the token before connecting and sends it as the first WebSocket message.
4. **Given** the gateway is in loopback mode (no auth), **When** the chat page loads, **Then** it connects immediately without prompting for a token.
5. **Given** an active conversation in the chat UI, **When** the user sends a follow-up message, **Then** the response reflects conversation context (the agent remembers prior messages in the session).
6. **Given** the LLM is streaming a response, **When** tokens arrive, **Then** each token is appended to the response area immediately via Leptos fine-grained reactivity (not buffered until complete), and the view auto-scrolls to show the latest text.
7. **Given** an LLM response containing markdown (code blocks, bold, lists), **When** rendered in the chat UI, **Then** the markdown is displayed with proper formatting (syntax-highlighted code blocks, styled bold/italic, indented lists).
8. **Given** the LLM provider is unreachable or returns an error, **When** the user sends a message, **Then** the chat UI displays the error clearly in the conversation thread (not a silent failure or browser console error).
9. **Given** the LLM is streaming a response, **When** the user attempts to type or send a new message, **Then** the input is disabled until the current response completes, indicating the agent is still responding.
10. **Given** the gateway is not running, **When** the user navigates to the URL, **Then** the browser shows its standard connection-refused behavior (no special handling needed).

---

### Edge Cases

- What happens when the user runs `exoclaw onboard` but the home directory is read-only or the disk is full? The system reports a clear filesystem error ("cannot write to ~/.exoclaw: permission denied") and does not leave partial files.
- What happens when the credential file exists but is empty or corrupted? The runtime treats it as missing and falls back to the environment variable. If neither is available, the gateway reports "no API key configured" at startup.
- What happens when two browser tabs open the chat UI simultaneously? Each tab gets its own WebSocket connection and session. Messages in one tab do not appear in the other.
- What happens when the user refreshes the chat page? The conversation history from the current session is lost in the UI (it's a stateless page), but the server-side session retains context so the agent still remembers prior messages.
- What happens when the WebSocket connection drops mid-stream (network blip, laptop sleep)? The chat UI shows a "connection lost" indicator and offers a reconnect action. The server-side session remains intact.
- What happens when a provider name other than "anthropic" or "openai" is entered during onboarding? The system rejects it with "invalid provider: expected anthropic or openai" and re-prompts.

## Requirements *(mandatory)*

### Functional Requirements

**Onboarding**

- **FR-001**: System MUST provide a CLI command (`exoclaw onboard`) that guides first-time setup: provider selection and API key entry.
- **FR-002**: System MUST accept API key input via hidden terminal entry (characters not echoed to screen).
- **FR-003**: System MUST store the API key in a dedicated credential file (`~/.exoclaw/credentials/{provider}.key`), separate from the main config file.
- **FR-004**: Credential files MUST be created with owner-only read/write permissions (mode 0600); the credentials directory MUST be owner-only (mode 0700).
- **FR-005**: The main config file (`~/.exoclaw/config.toml`) MUST NOT contain API key values. Keys are resolved at runtime.
- **FR-006**: System MUST resolve API keys in priority order: environment variable first, then credential file, then absent.
- **FR-007**: System MUST validate that the entered API key is non-empty before writing any files.
- **FR-008**: Re-running `exoclaw onboard` MUST update credentials and provider/model settings while preserving unrelated config sections (plugins, bindings, budgets, memory).

**Web Chat UI**

- **FR-009**: The gateway MUST serve a Leptos-based chat application at its root URL (`/`), compiled from Rust to WASM with fine-grained reactivity. No external CDN dependencies.
- **FR-010**: The chat page MUST establish a WebSocket connection to the gateway and communicate using the existing JSON-RPC protocol (`chat.send`).
- **FR-011**: The chat page MUST display streamed LLM response tokens as they arrive, appending each token immediately via Leptos fine-grained DOM updates (not buffering until completion).
- **FR-012**: The chat page MUST handle authentication: prompt for a token when the gateway requires one, skip the prompt in loopback mode.
- **FR-013**: The chat page MUST display errors from the gateway (provider unreachable, budget exceeded, no API key configured) as visible messages in the conversation thread.
- **FR-014**: The chat page MUST be built entirely with Rust tooling (`cargo-leptos`). No npm, no JS package managers, no JavaScript build steps.
- **FR-015**: The chat page MUST render markdown in LLM responses: code blocks with syntax highlighting, bold/italic text, lists, and headers.
- **FR-016**: The chat page MUST disable message input while an LLM response is streaming and re-enable it when the response completes or errors.

### Key Entities

- **Credential File**: A plaintext file containing a single API key for one provider. Located at `~/.exoclaw/credentials/{provider}.key`. Restricted to owner-only permissions. Read at gateway startup as a fallback when the corresponding environment variable is not set.
- **Chat Session (UI)**: A browser-side conversation state managed by Leptos reactive signals. Tied to a single WebSocket connection. Lost on page refresh, but the server-side session (from 001-core-runtime) retains context for the agent.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A new user can go from `cargo install exoclaw` to sending their first AI message through the browser chat UI in under 5 minutes, following only: `exoclaw onboard` then `exoclaw gateway` then open browser.
- **SC-002**: The onboarding flow completes in under 30 seconds for a user who has their API key ready.
- **SC-003**: The chat page loads in under 500ms including WASM initialization (Leptos hydration).
- **SC-004**: Streamed tokens appear in the chat UI within 50ms of the gateway receiving them from the LLM provider (fine-grained reactivity overhead).
- **SC-005**: Credential files are never readable by other users on the system (verified by file permission check).
- **SC-006**: The chat page works in all modern browsers (Chrome, Firefox, Safari, Edge) that support WebAssembly.
