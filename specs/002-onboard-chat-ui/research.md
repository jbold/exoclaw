# Research: Onboarding & Built-in Chat UI

**Feature**: `002-onboard-chat-ui`
**Date**: 2026-02-08

## Decision 1: Frontend Framework

**Decision**: Leptos 0.8 with `leptos_axum` integration

**Rationale**:
- Native axum 0.8 integration (exoclaw already runs on axum 0.8)
- Fine-grained reactivity (SolidJS-style): only the text node updates when tokens stream in, no virtual DOM diffing
- Built-in WebSocket support via server functions (added in 0.8.0)
- WASM code splitting / lazy loading (0.8.5+)
- 1.9x more popular than Dioxus for web-specific use cases
- 17.7k GitHub stars, very active development (0.8.15 latest)

**Alternatives considered**:
- **Dioxus 0.7**: Cross-platform (web + desktop + mobile) but virtual DOM overhead, not axum-native. Better for desktop apps.
- **Yew**: Mature but momentum has slowed. Virtual DOM.
- **Inline HTML/JS**: Zero build complexity (~150 lines JS) but no Rust type safety, poor markdown rendering, doesn't align with project's Rust-first philosophy.

## Decision 2: Project Structure

**Decision**: Cargo workspace with separate UI crate

**Rationale**:
- The existing `Cargo.toml` has heavy server-only dependencies (extism, async-nats, reqwest/rustls) that must NOT compile into the WASM bundle
- A separate `ui/` crate cleanly separates browser code from server code without fragile `#[cfg(feature)]` gating on dozens of dependencies
- `cargo-leptos` handles building both the server binary and the WASM frontend
- The server crate imports the UI crate and serves it via `leptos_axum`

**Alternatives considered**:
- **Single-crate with features**: Simpler Cargo.toml but requires feature-gating every server-only dependency. Error-prone with 20+ deps.
- **Full start-axum-workspace** (app + server + frontend): Overkill — exoclaw already has a well-structured server crate.

**Workspace layout**:
```
Cargo.toml              # workspace root
├── src/                 # existing exoclaw server (binary crate, workspace member)
├── ui/                  # new Leptos frontend (lib crate, workspace member)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── app.rs       # Root Leptos app component
│       └── components/
│           ├── chat.rs
│           ├── message.rs
│           └── auth_prompt.rs
└── examples/echo-plugin/ # existing (stays independent)
```

## Decision 3: Build Tooling

**Decision**: `cargo-leptos` replaces `cargo build` for development and release

**Rationale**:
- `cargo leptos build` orchestrates two cargo builds: native binary + WASM frontend
- `cargo leptos watch` provides hot-reload during development
- `cargo check`, `cargo clippy`, `cargo test`, `cargo fmt` still work independently for the server crate
- Release build: `cargo leptos build --release` produces server binary + `target/site/` assets

**Impact on existing workflow**:
- `cargo build` → `cargo leptos build`
- `cargo build --release` → `cargo leptos build --release`
- `cargo check` / `cargo test` / `cargo clippy` / `cargo fmt` → unchanged
- CI needs `cargo-leptos` installed: `cargo install cargo-leptos`

## Decision 4: WebSocket Client (Browser)

**Decision**: `gloo-net` for WebSocket from Leptos WASM to gateway

**Rationale**:
- Idiomatic Rust bindings over `web_sys::WebSocket`
- Works cleanly in `wasm32-unknown-unknown` target
- Supports async read/write via futures streams
- The gateway's existing `/ws` endpoint and JSON-RPC protocol stay unchanged
- The Leptos frontend connects as a regular WebSocket client

**Alternatives considered**:
- **Leptos server functions (WebSocket mode)**: Built into Leptos 0.8 but designed for Leptos-specific RPC, not generic JSON-RPC. Would require adapting the protocol.
- **Raw web-sys**: More verbose, no async ergonomics.

## Decision 5: Markdown Rendering

**Decision**: `pulldown-cmark` compiled to WASM

**Rationale**:
- Lightweight WASM footprint (smallest of the Rust markdown parsers)
- CommonMark compliant — handles code blocks, bold, italic, lists, headers
- Pure Rust, no C dependencies — compiles cleanly to `wasm32-unknown-unknown`
- Widely used (21M+ downloads)

**Alternatives considered**:
- **comrak**: Full GitHub Flavored Markdown but ~200KB+ additional WASM size. Overkill for chat.
- **markdown-rs**: Newer, good WASM support, but smaller community.

**Syntax highlighting**: Use a lightweight WASM-compatible highlighter or CSS-only approach (e.g., `class="language-rust"` + a minimal CSS theme). Avoid heavy highlighters like `syntect` in WASM.

## Decision 6: Serving Strategy

**Decision**: Leptos routes at `/`, existing WebSocket at `/ws`

**Rationale**:
- Register custom routes (`/ws`, `/health`, `/webhook/{channel}`) BEFORE `.leptos_routes()`
- Leptos catches `/` and serves the chat app
- Custom handlers take precedence over Leptos catch-all
- The WASM bundle and assets are served from `target/site/pkg/` via `tower_http::services::ServeDir`

**Route order**:
```
Router::new()
    .route("/ws", get(ws_handler))            // Existing JSON-RPC WebSocket
    .route("/health", get(health))            // Existing health check
    .route("/webhook/{channel}", post(...))   // Existing webhook
    .leptos_routes(state, routes!(App))       // Leptos chat UI at /
    .fallback(not_found)
```

## Decision 7: Binary Size

**Decision**: Acceptable — well within 25MB constitution target

**Rationale**:
- Server binary overhead from leptos_axum: ~1-2 MB additional
- WASM bundle (browser-side): ~50-100 KB gzipped for a minimal app
- Current release binary: ~21 MB (LTO + strip)
- With Leptos: estimated ~22-23 MB server binary + separate WASM assets
- The 25MB constitution target applies to the server binary; WASM is a separate download

**Optimization levers if needed**:
- `wasm-opt -Oz` (default in cargo-leptos)
- Islands mode for selective hydration
- Explicit `[build] target` in Cargo.toml to prevent WASM opt-level from bleeding into server profile

## Decision 8: Hidden Password Input

**Decision**: `rpassword` crate (already in Cargo.toml)

**Rationale**:
- Already added as a dependency in the uncommitted onboarding code
- `rpassword::prompt_password()` handles cross-platform hidden input
- Lightweight (~3 dependencies), well-maintained
- No terminal UI framework needed (no `dialoguer`, no `inquire`)
