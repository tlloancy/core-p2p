#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/core-p2p-wasm-target}"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "Installing wasm-pack..."
  cargo install wasm-pack --locked
fi

rustup target add wasm32-unknown-unknown 2>/dev/null || true

if ! command -v clang >/dev/null 2>&1; then
  echo "ERROR: clang required for ring (iroh tls-ring on wasm). Install: sudo apt install clang"
  exit 1
fi

wasm-pack build \
  --target web \
  --out-dir pkg \
  --release \
  -- \
  --no-default-features

mkdir -p ../sport-app/public/core-p2p
cp -f pkg/core_p2p.js pkg/core_p2p_bg.wasm pkg/core_p2p.d.ts ../sport-app/public/core-p2p/ 2>/dev/null || true

echo "WASM_BUILD_OK -> pkg/ (+ sport-app/public/core-p2p when copy succeeded)"
