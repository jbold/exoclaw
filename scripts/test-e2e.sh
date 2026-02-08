#!/usr/bin/env bash
set -euo pipefail

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required. Install Node.js/npm first." >&2
  exit 1
fi

if [[ ! -d node_modules/@playwright/test ]]; then
  echo "Playwright dependencies missing. Run: npm install" >&2
  exit 1
fi

npx playwright test

