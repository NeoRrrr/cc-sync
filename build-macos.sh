#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  echo "[cc-sync] cargo not found. Install Rust first: https://rustup.rs/" >&2
  exit 1
fi

if [ ! -d node_modules ]; then
  echo "[cc-sync] node_modules not found, running npm ci..."
  npm ci
fi

echo "[cc-sync] building macOS app and DMG..."
npm run tauri:build:mac

VERSION="$(node -p "require('./package.json').version")"
HOST_ARCH="$(uname -m)"
case "$HOST_ARCH" in
  arm64) TARGET_ARCH="aarch64" ;;
  x86_64) TARGET_ARCH="x64" ;;
  *) TARGET_ARCH="$HOST_ARCH" ;;
esac

RELEASE_DIR="$ROOT_DIR/release-assets"
APP_PATH="$ROOT_DIR/src-tauri/target/release/bundle/macos/CC Sync.app"
mkdir -p "$RELEASE_DIR"

if [ ! -d "$APP_PATH" ]; then
  echo "[cc-sync] app bundle not found: $APP_PATH" >&2
  exit 1
fi

DMG_OUT="$RELEASE_DIR/CC.Sync_${VERSION}_${TARGET_ARCH}.dmg"
APP_ZIP="$RELEASE_DIR/CC.Sync_${VERSION}_${TARGET_ARCH}.app.zip"

echo "[cc-sync] ad-hoc signing app bundle..."
codesign --force --deep --sign - "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

echo "[cc-sync] creating normalized DMG..."
hdiutil create -volname "CC Sync" -srcfolder "$APP_PATH" -ov -format UDZO "$DMG_OUT"
ditto -c -k --sequesterRsrc --keepParent "$APP_PATH" "$APP_ZIP"

(
  cd "$RELEASE_DIR"
  shasum -a 256 *.dmg *.zip > "SHA256SUMS-macos-${VERSION}.txt"
)

echo "[cc-sync] macOS release assets ready:"
find "$RELEASE_DIR" -maxdepth 1 -type f -print | sort
