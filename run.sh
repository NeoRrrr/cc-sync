#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
  # macOS GUI/dev shells may not have Rust on PATH yet.
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "[cc-sync] cargo not found. Install Rust first: https://rustup.rs/" >&2
  exit 1
fi

if [ ! -d node_modules ]; then
  echo "[cc-sync] node_modules not found, running npm install..."
  npm install
fi

echo "[cc-sync] starting tauri dev..."
npm run tauri:dev
