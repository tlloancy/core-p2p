#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true
BIN=/root/.cache/core-p2p-target/release/examples/relay_connect
cargo build -q --release --example relay_connect
LOG=/tmp/relay-connect.log
"$BIN" serve >"$LOG" 2>&1 &
PID=$!
trap "kill $PID 2>/dev/null || true" EXIT

for _ in $(seq 1 90); do
  grep -q '^NODE_ID=' "$LOG" && break
  sleep 1
done
cat "$LOG"
REMOTE=$(grep '^NODE_ID=' "$LOG" | head -1 | cut -d= -f2-)
sleep 10
"$BIN" dial "$REMOTE"
