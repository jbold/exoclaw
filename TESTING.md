# Testing and TDD

This repository uses a layered test stack:

- Backend/unit/integration: `cargo-nextest`
- Coverage gate: `cargo-llvm-cov`
- UI Rust-to-WASM tests: `wasm-pack` + `wasm-bindgen-test`
- Browser E2E: Playwright
- Dependency security: `cargo-audit` + `cargo-deny`

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install cargo-nextest cargo-llvm-cov wasm-pack trunk
npm install
npx playwright install chromium
```

## Test Commands

```bash
# Backend tests + line coverage gate (default: 70%)
./scripts/test-rust.sh

# WASM UI tests (runs ui/tests/*.rs in Node)
./scripts/test-wasm-ui.sh

# Browser E2E tests
./scripts/test-e2e.sh

# Full suite
./scripts/test-all.sh
```

Set a stricter coverage gate locally:

```bash
COVERAGE_MIN_LINES=75 ./scripts/test-rust.sh
```

## Red/Green Workflow

1. Write a failing test in the right layer first:
   - backend behavior: `tests/*.rs` or module `#[cfg(test)]`
   - UI parser/transform logic: `ui/tests/*.rs` with `#[wasm_bindgen_test]`
   - full user flow: `e2e/*.spec.ts`
2. Run the smallest relevant command.
3. Implement the behavior.
4. Re-run targeted tests.
5. Run `./scripts/test-all.sh` before merging.

## CI

GitHub Actions workflow: `.github/workflows/test-suite.yml`

Jobs:

- `rust-tests`: nextest + coverage gate
- `security`: cargo-audit + cargo-deny
- `wasm-ui-tests`: wasm-pack tests
- `e2e-tests`: Playwright E2E against the real gateway + embedded UI
