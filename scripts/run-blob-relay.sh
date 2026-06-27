#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true

BLOB_STORE_DIR="${BLOB_STORE_DIR:-/var/lib/sport-p2p/blobs}"
mkdir -p "$BLOB_STORE_DIR"
export BLOB_STORE_DIR

cargo build --release --example blob_relay
BIN="${CARGO_TARGET_DIR:-/root/.cache/core-p2p-target}/release/examples/blob_relay"
if [ ! -x "$BIN" ]; then
  BIN="target/release/examples/blob_relay"
fi
exec "$BIN" serve
