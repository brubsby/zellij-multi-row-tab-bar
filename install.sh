#!/usr/bin/env bash
# Build the plugin to wasm and install it into zellij's plugin dir.
set -euo pipefail

PLUGIN_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/zellij/plugins"
TARGET="wasm32-wasip1"

cd "$(dirname "$0")"
rustup target add "$TARGET" >/dev/null 2>&1 || true
cargo build --release --target "$TARGET"

mkdir -p "$PLUGIN_DIR"
cp "target/$TARGET/release/multi-row-tab-bar.wasm" "$PLUGIN_DIR/"
echo "installed -> $PLUGIN_DIR/multi-row-tab-bar.wasm"
