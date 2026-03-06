#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="$ROOT_DIR/ui"
UI_ZIP_PATH="$ROOT_DIR/target/ui-assets.zip"
RELEASE_DIR="$ROOT_DIR/target/release"
SERVER_BIN_PATH="$RELEASE_DIR/server"
COMBINED_BIN_PATH="$RELEASE_DIR/wideboard"

cd "$ROOT_DIR"

echo "[1/4] Building UI assets"
cd "$UI_DIR"
if [[ ! -d "$UI_DIR/node_modules" ]]; then
  echo "ui/node_modules missing; installing dependencies"
  CI=true ../tool-tool pnpm install --frozen-lockfile
fi
../tool-tool pnpm build

echo "[2/4] Packaging UI assets into $UI_ZIP_PATH"
mkdir -p "$(dirname "$UI_ZIP_PATH")"
rm -f "$UI_ZIP_PATH"
cd "$UI_DIR/dist"
if command -v zip >/dev/null 2>&1; then
  zip -r "$UI_ZIP_PATH" .
else
  bsdtar -a -cf "$UI_ZIP_PATH" .
fi

cd "$ROOT_DIR"
echo "[3/4] Building release server binary"
cargo build --release -p server

if [[ ! -f "$SERVER_BIN_PATH" ]]; then
  echo "Expected server binary not found at $SERVER_BIN_PATH" >&2
  exit 1
fi

echo "[4/4] Appending UI bundle to server binary"
cp "$SERVER_BIN_PATH" "$COMBINED_BIN_PATH"
cat "$UI_ZIP_PATH" >> "$COMBINED_BIN_PATH"
chmod +x "$COMBINED_BIN_PATH"

echo "Build complete"
echo "- UI zip: $UI_ZIP_PATH"
echo "- Combined binary: $COMBINED_BIN_PATH"
