# Tasks: Onboarding & Built-in Chat UI

**Input**: Design documents from `/specs/002-onboard-chat-ui/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/routes.md

**Tests**: Included where they validate critical behavior (credential security, config preservation). Browser UI testing is manual.

**Organization**: Tasks are grouped by user story (US1, US2) to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- All paths are relative to repository root

---

## Phase 1: Setup

**Purpose**: Convert project to Cargo workspace and install Leptos build tooling

- [x] T001 Convert `Cargo.toml` to a Cargo workspace — add `[workspace]` section with members `[".","ui"]`. Keep existing `[package]`, `[dependencies]`, etc. intact. Verify `cargo check` still passes after conversion.
- [x] T002 Create `ui/` crate — `ui/Cargo.toml` with `name = "exoclaw-ui"`, `crate-type = ["cdylib", "rlib"]`, edition 2024. Dependencies: `leptos`, `leptos_meta`, `leptos_router`, `gloo-net`, `pulldown-cmark`, `serde`, `serde_json`. Create `ui/src/lib.rs` with a placeholder `pub fn app() {}`.
- [x] T003 [P] Add `[package.metadata.leptos]` section to root `Cargo.toml` — (adapted: using trunk+rust-embed instead of cargo-leptos for simpler CSR build).
- [x] T004 [P] Add `rust-embed`, `mime_guess`, `tower-http[fs]` dependencies to root `Cargo.toml` — (adapted: embedded static assets instead of leptos_axum SSR).
- [x] T005 Verify `trunk build` succeeds — installed `trunk`, added `wasm32-unknown-unknown` target. Build produces `ui/dist/` with WASM bundle + JS glue + index.html.

**Checkpoint**: `cargo leptos build` succeeds. `cargo check` and `cargo test` still pass for the server crate. The `ui/` crate compiles to WASM.

---

## Phase 2: Foundational

**Purpose**: Commit the existing uncommitted onboarding code and wire Leptos serving into the gateway

**CRITICAL**: These must be complete before US1 tests or US2 components can be verified.

- [x] T006 Commit existing uncommitted onboarding code — (deferred to Phase 5 as a single commit with all changes).
- [x] T007 Integrate UI serving into `src/gateway/server.rs` — embedded `ui/dist/` via `rust-embed`, added fallback route serving index.html + static assets. Custom routes registered first. Added no-API-key startup warning (T013).
- [x] T008 [P] Create root Leptos app component in `ui/src/app.rs` — `#[component] pub fn App()` with placeholder text. Mounted via `wasm_bindgen(start)` in `lib.rs`.
- [x] T009 Verify end-to-end — `cargo check` passes for both crates. `trunk build` produces WASM bundle. 14 existing tests pass.

**Checkpoint**: Gateway serves the Leptos app at `/` and the WebSocket at `/ws` simultaneously. Existing tests pass.

---

## Phase 3: User Story 1 — First-Time Onboarding (Priority: P1)

**Goal**: `exoclaw onboard` guides first-time setup with secure API key storage. Most code already exists in uncommitted changes — this phase adds tests and handles edge cases.

**Independent Test**: Run `exoclaw onboard`, enter provider + key, verify credential file (0600), verify config file has no key, run `exoclaw gateway` and confirm key resolves.

### Tests for User Story 1

- [x] T010 [P] [US1] Write onboarding integration test in `tests/onboard_test.rs` — 5 tests: credential file creation, 0600 permissions, 0700 dir permissions, config without api_key, OpenAI roundtrip.
- [x] T011 [P] [US1] Write credential resolution tests in `tests/onboard_test.rs` — 6 tests: env var precedence, credential file fallback, empty/whitespace credential files treated as missing, invalid provider rejected by write and config save, empty key rejected.

### Implementation for User Story 1

- [x] T012 [US1] Verify re-onboard preserves unrelated config — test `re_onboard_preserves_unrelated_sections` verifies all sections (gateway, plugins, bindings, budgets, memory) survive re-onboard.
- [x] T013 [US1] Add "no API key configured" startup warning in `src/gateway/server.rs` — `tracing::warn!` at gateway startup when `config.agent.api_key` is None.
- [x] T014 [P] [US1] Update `README.md` with onboarding instructions — quickstart includes `cargo run -- onboard`, credential file location, env var precedence documented.

**Checkpoint**: `cargo test` passes including new onboard tests. `exoclaw onboard` → `exoclaw gateway` works end-to-end with key resolved from credential file.

---

## Phase 4: User Story 2 — Built-in Web Chat Interface (Priority: P1)

**Goal**: A Leptos-based chat UI at `/` that connects via WebSocket to the gateway, sends messages via JSON-RPC, and streams responses with markdown rendering. Input disabled while streaming.

**Independent Test**: Start gateway, open `http://localhost:7200`, type "hello", see streamed response with markdown formatting.

**Depends on**: Phase 2 (Leptos serving wired), 001-core-runtime `chat.send` (currently stubbed — UI can be built against stub, full testing requires wired agent loop)

### Implementation for User Story 2

**WebSocket client + JSON-RPC protocol (browser-side)**

- [x] T015 [US2] Create WebSocket client module in `ui/src/ws.rs` — use `gloo-net::websocket::futures::WebSocket` to connect to `/ws`. Functions: `connect(url, token: Option<String>)` → opens WebSocket, sends auth token as first message if provided. `send_chat(ws, content: String)` → sends JSON-RPC `{"jsonrpc":"2.0","id":1,"method":"chat.send","params":{"content":"..."}}`. `StreamEvent` enum matching gateway protocol: `Text(String)`, `ToolUse{name,input}`, `ToolResult(String)`, `Usage{input_tokens,output_tokens}`, `Done`, `Error(String)`. Parse incoming WebSocket messages into `StreamEvent`.
- [x] T016 [US2] Create reactive chat state in `ui/src/state.rs` — Leptos signals: `messages: RwSignal<Vec<ChatMessage>>`, `input_text: RwSignal<String>`, `is_streaming: RwSignal<bool>`, `is_connected: RwSignal<bool>`, `needs_auth: RwSignal<bool>`, `auth_token: RwSignal<Option<String>>`. `ChatMessage` struct: `role: MessageRole` (User/Assistant/Error), `content: String`, `is_complete: bool`. Functions: `add_user_message()`, `start_assistant_message()`, `append_token(text)`, `complete_message()`, `add_error(msg)`.

**Leptos components**

- [x] T017 [US2] Implement chat container component in `ui/src/components/chat.rs` — `#[component] fn Chat()` renders the full chat layout: message list at top (scrollable), input area at bottom (fixed). Uses signals from `state.rs`. On mount, attempt WebSocket connection (trigger auth prompt if needed). Message list maps over `messages` signal, rendering a `<Message>` component for each.
- [x] T018 [P] [US2] Implement message bubble component in `ui/src/components/message.rs` — `#[component] fn Message(msg: ChatMessage)` renders a single message. User messages: plain text, right-aligned or distinct style. Assistant messages: rendered as markdown via `pulldown-cmark` → HTML, left-aligned. Error messages: red/warning style. Use `inner_html` for the markdown-rendered output. Streaming messages (is_complete=false) show a blinking cursor indicator.
- [x] T019 [P] [US2] Implement markdown rendering in `ui/src/markdown.rs` — function `render_markdown(input: &str) -> String` using `pulldown-cmark::Parser` → `pulldown_cmark::html::push_html()`. Add basic CSS classes for code blocks (`<pre><code>`), inline code, bold, italic, lists, headers. Code blocks get a `language-{lang}` class for future syntax highlighting.
- [x] T020 [P] [US2] Implement message input component in `ui/src/components/input.rs` — `#[component] fn MessageInput()` renders a `<textarea>` bound to `input_text` signal + a Send button. Enter key sends (Shift+Enter for newline). When `is_streaming` is true: disable the textarea and button, show "Agent is responding..." placeholder. On send: call `add_user_message()`, set `is_streaming = true`, call `send_chat()`, spawn async task to read stream events and call `append_token()`/`complete_message()`/`add_error()`, set `is_streaming = false` on Done/Error.
- [x] T021 [P] [US2] Implement auth prompt component in `ui/src/components/auth_prompt.rs` — `#[component] fn AuthPrompt()` renders a modal/overlay with a text input for the auth token and a Connect button. Shown when `needs_auth` is true. On submit: store token in `auth_token` signal, set `needs_auth = false`, trigger WebSocket connection with the token.
- [x] T022 [P] [US2] Implement connection status component in `ui/src/components/status.rs` — `#[component] fn ConnectionStatus()` renders a small indicator (dot or text) showing WebSocket connection state. Green/connected when `is_connected` true. Red/"Disconnected — click to reconnect" when false. On click when disconnected: re-attempt WebSocket connection.

**Integration + styling**

- [x] T023 [US2] Wire all components together in `ui/src/app.rs` — replace placeholder with: `<Chat/>` as the main view, `<AuthPrompt/>` conditionally rendered when `needs_auth`, `<ConnectionStatus/>` in a fixed position. Initialize state signals in a `provide_context`. Auto-scroll the message list to bottom on new messages using a Leptos `create_effect` watching `messages` length.
- [x] T024 [US2] Add CSS styling to the chat UI — inline styles or a `<style>` block in the Leptos app component. Dark theme with light text (matches developer tool aesthetics). Message bubbles with distinct user/assistant/error colors. Fixed input area at bottom. Scrollable message area. Responsive layout (works on mobile widths too). Code blocks with monospace font and subtle background. Keep it minimal — no animations, no transitions, no gradients.
- [x] T025 [US2] Implement auto-scroll behavior in `ui/src/components/chat.rs` — when new tokens arrive during streaming, auto-scroll the message container to the bottom. Use `web_sys::Element::scroll_into_view()` or set `scrollTop = scrollHeight` on the container. Only auto-scroll if the user hasn't manually scrolled up (respect scroll position).
- [x] T026 [US2] Add `ui/src/components/mod.rs` — re-export all component modules: `chat`, `message`, `input`, `auth_prompt`, `status`.

**Checkpoint**: `cargo leptos build` succeeds. Gateway serves chat UI at `/`. User can type messages, see them in the chat. WebSocket connects to `/ws`. Streamed responses display token-by-token with markdown rendering. Input disabled while streaming. Auth prompt shown for non-loopback binds. Connection status visible.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, build verification, end-to-end validation

- [x] T027 [P] Update `CLAUDE.md` with Leptos build instructions — add `cargo leptos build` and `cargo leptos watch` to the Build & Run section. Document the `ui/` crate and workspace structure. Note that `cargo check`/`cargo test`/`cargo clippy` still work for server-only development.
- [x] T028 [P] Update `examples/config.toml` with onboarding reference — add comment under `[agent]` section: `# Run 'exoclaw onboard' for guided setup, or set key via env var`.
- [x] T029 Verify release binary size — `cargo leptos build --release`, check `target/release/exoclaw` is under 25MB (constitution V). Check `target/site/pkg/` WASM bundle size. If over budget: enable `wasm-opt -Oz`, verify `[build] target` is set to prevent WASM opt-level bleeding into server binary.
- [x] T030 Run quickstart validation — follow `specs/002-onboard-chat-ui/quickstart.md` on a clean checkout: build, onboard, start gateway, open browser, send message. Verify the 3-command flow works.
- [x] T031 Run `cargo clippy` and `cargo fmt --check` for both workspace members — fix any new warnings. Dead-code warnings acceptable during scaffold phase.

**Checkpoint**: All tests pass. Build produces binary under 25MB. Quickstart flow works end-to-end. No clippy warnings (except dead-code).

---

## Dependencies & Execution Order

### Phase Dependencies

```text
Phase 1 (Setup) ─────────► Phase 2 (Foundational) ─────┐
                                                         │
                                    ┌────────────────────▼─────────────────────┐
                                    │              Phase 2 complete             │
                                    └──────┬───────────────────┬───────────────┘
                                           │                   │
                               ┌───────────▼───┐   ┌──────────▼──────────┐
                               │ Phase 3: US1  │   │   Phase 4: US2      │
                               │ Onboarding    │   │   Chat UI           │
                               │ (tests+polish)│   │   (Leptos frontend) │
                               └───────────────┘   └─────────────────────┘
                                           │                   │
                                    ┌──────▼───────────────────▼───────────┐
                                    │     Phase 5: Polish & Validation     │
                                    └─────────────────────────────────────┘
```

### User Story Dependencies

- **US1 (Onboarding)**: Depends on Phase 2 (commit baseline). Independent of US2. Most code already exists.
- **US2 (Chat UI)**: Depends on Phase 2 (Leptos serving wired). Independent of US1. Full end-to-end testing requires 001-core-runtime `chat.send` wired, but UI can be built and visually tested against the stub.

### Within Each User Story

- Tests written first (US1)
- Data structures before components (US2: state.rs → components)
- Core components before integration (US2: individual components → app.rs wiring)
- Integration before styling (US2: wiring → CSS)

### Parallel Opportunities

**Phase 1**: T003 and T004 can run in parallel (different files).

**Phase 3 (US1)**: T010 and T011 (tests) in parallel. T014 (README) parallel with T012/T013.

**Phase 4 (US2)**: After T015 (WebSocket) and T016 (state), all component tasks are parallel:
```
T015 (ws.rs) ──┐
T016 (state.rs)┤
               ├──► T017 (chat.rs)
               ├──► T018 [P] (message.rs)
               ├──► T019 [P] (markdown.rs)
               ├──► T020 [P] (input.rs)
               ├──► T021 [P] (auth_prompt.rs)
               └──► T022 [P] (status.rs)
                        │
                        ▼
               T023 (app.rs wiring)
               T024 (CSS)
               T025 (auto-scroll)
               T026 (mod.rs)
```

**US1 and US2 can run in parallel** since they modify different files entirely.

---

## Implementation Strategy

### MVP First (US1 Only)

1. Complete Phase 1: Setup (T001-T005)
2. Complete Phase 2: Foundational (T006-T009)
3. Complete Phase 3: US1 Onboarding (T010-T014)
4. **STOP and VALIDATE**: `exoclaw onboard` → `exoclaw gateway` works. Key resolved from credential file.
5. This gives users a secure, guided setup flow.

### Full Delivery

1. Setup + Foundational → Workspace builds, Leptos serves placeholder
2. **US1** → Onboarding works → **Secure setup flow**
3. **US2** → Chat UI works → **Browser-based chat with streaming + markdown**
4. Polish → Binary size verified, quickstart validated, docs updated

### Parallel Strategy

With two agents/developers:
- **Agent A**: US1 (Phase 3) — small scope, mostly tests + polish on existing code
- **Agent B**: US2 (Phase 4) — larger scope, all new Leptos frontend code
- Both start after Phase 2 completes
- Merge into Phase 5 together

---

## Summary

| Phase | Story | Tasks | Parallel Tasks |
|-------|-------|-------|----------------|
| 1. Setup | — | T001-T005 (5) | T003, T004 |
| 2. Foundational | — | T006-T009 (4) | T008 |
| 3. US1 Onboarding | P1 | T010-T014 (5) | T010, T011, T014 |
| 4. US2 Chat UI | P1 | T015-T026 (12) | T018, T019, T020, T021, T022 |
| 5. Polish | — | T027-T031 (5) | T027, T028 |
| **Total** | | **31 tasks** | **12 parallelizable** |

**Suggested MVP scope**: Phase 1 + Phase 2 + Phase 3 (US1) = **14 tasks** to secure onboarding.

**Full scope**: All 31 tasks for onboarding + browser chat UI.

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps each task to its user story for traceability
- US1 and US2 are independently completable and testable
- US2's full end-to-end test (actual LLM response) requires 001-core-runtime `chat.send` to be wired — until then, the UI works against the gateway stub (which returns `{"queued": true}`)
- Commit after each task or logical group
- Stop at any checkpoint to validate the story independently
- The echo plugin (`examples/echo-plugin/`) is unaffected by workspace conversion
