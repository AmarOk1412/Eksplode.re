#!/bin/sh
# Builds the WebAssembly graphics module and copies it into the web server assets.
# Needs: rustup target add wasm32-unknown-unknown
set -eu

cd "$(dirname "$0")/.."
MAX_BYTES=307200

cargo build -p bomberust-web-render --target wasm32-unknown-unknown --profile wasm-release
src=target/wasm32-unknown-unknown/wasm-release/bomberust_web_render.wasm
dst=crates/web/assets/bomberust_render.wasm

mkdir -p "$(dirname "$dst")"
cp "$src" "$dst"
size=$(wc -c < "$dst")
echo "bomberust_render.wasm: $size bytes"
if [ "$size" -gt "$MAX_BYTES" ]; then
    echo "error: the module is larger than $MAX_BYTES bytes" >&2
    exit 1
fi
