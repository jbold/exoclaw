# Research: Onboarding & Built-in Chat UI

**Feature**: `002-onboard-chat-ui`
**Date**: 2026-02-08

## Decision 1: Frontend Framework

**Decision**: Leptos 0.7 CSR (client-side rendering) with trunk + rust-embed

**Rationale**:
- Fine-grained reactivity (SolidJS-style): only the text node updates when tokens stream in, no virtual DOM diffing
- CSR approach avoids SSR complexity — trunk compiles to WASM, rust-embed serves static assets from the server binary
- No `leptos_axum` dependency needed — axum serves pre-built WASM/HTML via a simple fallback route
- Leptos 0.7 is the stable release compatible with Rust edition 2024

**Alternatives considered**:
- **Leptos 0.8 with leptos_axum (SSR)**: Originally planned but pivoted away — SSR adds complexity for a chat UI that doesn't need server-side rendering. CSR is simpler and keeps `cargo build/test/clippy` working normally.
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

**Decision**: `trunk` for WASM builds + `rust-embed` for asset serving

**Rationale**:
- `trunk build` compiles the Leptos CSR app to WASM and produces `ui/dist/` (index.html + WASM + JS glue)
- `rust-embed` embeds `ui/dist/` at compile time into the server binary — no separate asset directory needed at runtime
- `cargo check`, `cargo clippy`, `cargo test`, `cargo fmt` still work independently for the server crate
- Simpler than `cargo-leptos` which is designed for SSR setups

**Impact on existing workflow**:
- UI changes: `trunk build` then `cargo build` to re-embed assets
- `cargo check` / `cargo test` / `cargo clippy` / `cargo fmt` → unchanged
- CI needs `trunk` installed: `cargo install trunk`

## Decision 4: WebSocket Client (Browser)

**Decision**: `gloo-net` for WebSocket from Leptos WASM to gateway

**Rationale**:
- Idiomatic Rust bindings over `web_sys::WebSocket`
- Works cleanly in `wasm32-unknown-unknown` target
- Supports async read/write via futures streams
- The gateway's existing `/ws` endpoint and JSON-RPC protocol stay unchanged
- The Leptos frontend connects as a regular WebSocket client

**Alternatives considered**:
- **Leptos server functions (WebSocket mode)**: Designed for Leptos-specific RPC, not generic JSON-RPC. Would require adapting the protocol.
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

**Decision**: rust-embed fallback route at `/`, existing WebSocket at `/ws`

**Rationale**:
- Register custom routes (`/ws`, `/health`, `/webhook/{channel}`) BEFORE the UI fallback
- The axum `.fallback(get(ui_handler))` catches `/` and all unmatched paths (SPA routing)
- `ui_handler` serves files from the embedded `ui/dist/` assets, falling back to `index.html`
- Custom handlers take precedence over the fallback

**Route order**:
```
Router::new()
    .route("/ws", get(ws_handler))            // Existing JSON-RPC WebSocket
    .route("/health", get(health))            // Existing health check
    .route("/webhook/{channel}", post(...))   // Existing webhook
    .fallback(get(ui_handler))                // Embedded UI assets at /
```

## Decision 7: Binary Size

**Decision**: Acceptable — well within 25MB constitution target

**Rationale**:
- Server binary overhead from rust-embed: minimal (embeds pre-built WASM assets)
- WASM bundle (browser-side): ~3 MB debug, smaller with wasm-opt
- Current release binary: ~21 MB (LTO + strip)
- With embedded UI: estimated ~22-23 MB server binary (WASM assets included)
- The 25MB constitution target applies to the server binary including embedded assets

**Optimization levers if needed**:
- `trunk build --release` with wasm-opt
- Explicit `[build] target` in Cargo.toml to prevent WASM opt-level from bleeding into server profile

## Decision 8: Hidden Password Input

**Decision**: `rpassword` crate (already in Cargo.toml)

**Rationale**:
- Already added as a dependency in the uncommitted onboarding code
- `rpassword::prompt_password()` handles cross-platform hidden input
- Lightweight (~3 dependencies), well-maintained
- No terminal UI framework needed (no `dialoguer`, no `inquire`)
