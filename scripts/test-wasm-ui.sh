#!/usr/bin/env bash
set -euo pipefail

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack is required. Install with: cargo install wasm-pack" >&2
  exit 1
fi

(
  cd ui
  wasm-pack test --node
)

