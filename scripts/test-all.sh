#!/usr/bin/env bash
set -euo pipefail

./scripts/test-rust.sh
./scripts/test-wasm-ui.sh
./scripts/test-e2e.sh

