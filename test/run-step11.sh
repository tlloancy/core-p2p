#!/usr/bin/env bash
# Cross-process relay test: rust seeder (inbound relay OK) + napi leecher (node_id only).
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true
BIN=/root/.cache/core-p2p-target/release/examples/blob_relay
cargo build -q --release --example blob_relay

LOG=$(mktemp /tmp/core-p2p-step11.XXXXXX.log)
"$BIN" serve >"$LOG" 2>&1 &
SERVER_PID=$!

cleanup() {
  kill "$SERVER_PID" 2>/dev/null || true
  wait "$SERVER_PID" 2>/dev/null || true
  rm -f "$LOG"
}
trap cleanup EXIT

for _ in $(seq 1 60); do
  if grep -q '^NODE_ID=' "$LOG"; then
    break
  fi
  sleep 1
done

REMOTE_ID=$(grep '^NODE_ID=' "$LOG" | head -1 | cut -d= -f2-)
if [ -z "$REMOTE_ID" ]; then
  echo "FAIL: server did not publish node_id"
  cat "$LOG"
  exit 1
fi

echo "server node_id=$REMOTE_ID"
grep '^ENDPOINT=' "$LOG" | head -1 || true
echo "waiting 10s for pkarr propagation..."
sleep 10

node test/step11-relay.js "$REMOTE_ID"
