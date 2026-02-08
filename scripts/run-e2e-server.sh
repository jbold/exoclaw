#!/usr/bin/env bash
set -euo pipefail

if ! command -v trunk >/dev/null 2>&1; then
  echo "trunk is required. Install with: cargo install trunk" >&2
  exit 1
fi

port="${EXOCLAW_E2E_PORT:-7210}"
token="${EXOCLAW_E2E_TOKEN:-e2e-test-token}"

# Playwright sets NO_COLOR=1 in some environments. Trunk's clap-based parser
# rejects that value for its no-color bool env handling.
unset NO_COLOR

(
  cd ui
  trunk build
)

exec cargo run --quiet -- gateway --bind 0.0.0.0 --port "${port}" --token "${token}"
