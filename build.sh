#!/usr/bin/env bash
# Build the native Node addon (cdylib) and place it as index.node.
set -euo pipefail
cd "$(dirname "$0")"
source "$HOME/.cargo/env" 2>/dev/null || true

cargo build --release
# Linux cdylib output is lib<crate>.so → expose it as index.node for require().
SO="/root/.cache/core-p2p-target/release/libcore_p2p.so"
if [ ! -f "$SO" ]; then
  SO="target/release/libcore_p2p.so"
fi
cp "$SO" index.node
echo "BUILD_OK -> index.node"
