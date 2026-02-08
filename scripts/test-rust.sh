#!/usr/bin/env bash
set -euo pipefail

coverage_min_lines="${COVERAGE_MIN_LINES:-70}"

if ! command -v cargo-nextest >/dev/null 2>&1; then
  echo "cargo-nextest is required. Install with: cargo install cargo-nextest" >&2
  exit 1
fi

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "cargo-llvm-cov is required. Install with: cargo install cargo-llvm-cov" >&2
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  if ! rustup component list --installed | grep -q '^llvm-tools'; then
    rustup component add llvm-tools-preview
  fi
fi

# Integration tests load fixture WASM plugins from examples/*/target/...;
# build them explicitly for clean CI runners.
if [[ -f examples/mock-channel/Cargo.toml || -f examples/echo-plugin/Cargo.toml ]]; then
  if command -v rustup >/dev/null 2>&1; then
    rustup target add wasm32-unknown-unknown
  fi
  if [[ -f examples/mock-channel/Cargo.toml ]]; then
    cargo build \
      --manifest-path examples/mock-channel/Cargo.toml \
      --release \
      --target wasm32-unknown-unknown
  fi
  if [[ -f examples/echo-plugin/Cargo.toml ]]; then
    cargo build \
      --manifest-path examples/echo-plugin/Cargo.toml \
      --release \
      --target wasm32-unknown-unknown
  fi
fi

# Backend tests compile rust-embed assets from ui/dist. Keep a minimal
# placeholder so CI can run Rust tests without building the full UI bundle.
mkdir -p ui/dist
if [[ ! -f ui/dist/index.html ]]; then
  cat > ui/dist/index.html <<'EOF'
<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>exoclaw</title></head>
<body>UI placeholder for Rust test compile</body>
</html>
EOF
fi

cargo nextest run --workspace --exclude exoclaw-ui
cargo llvm-cov \
  --workspace \
  --exclude exoclaw-ui \
  --ignore-filename-regex 'src/main.rs$' \
  --summary-only \
  --fail-under-lines "${coverage_min_lines}"
