#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
git add -A
git commit -m "Implement iroh P2P core with Layer 1 e2e tests passing.

Native Node addon (napi-rs) binds iroh on localhost for reliable QUIC transfer.
Includes blob send/fetch, HLS chunk streaming tests, and 3-peer relay scenario."
