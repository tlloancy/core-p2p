#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true
BIN=/root/.cache/core-p2p-target/release/examples/relay_connect
cargo build -q --release --example relay_connect
LOG=/tmp/rust-server.log
"$BIN" serve >"$LOG" 2>&1 &
PID=$!
trap "kill $PID 2>/dev/null || true" EXIT

for _ in $(seq 1 60); do
  grep -q '^NODE_ID=' "$LOG" && break
  sleep 1
done
cat "$LOG"
REMOTE=$(grep '^NODE_ID=' "$LOG" | head -1 | cut -d= -f2-)
sleep 10
node -e "
const { create_peer, probe_connect, shutdown_peer } = require('./index.node');
(async () => {
  const local = await create_peer();
  console.log('napi probe_connect to rust server', '$REMOTE');
  await probe_connect(local, '$REMOTE');
  console.log('PASS: napi client connected via node_id + relay');
  await shutdown_peer(local);
})().catch((e) => { console.error('FAIL:', e); process.exit(1); });
"
