#!/usr/bin/env bash
# Safe cleanup of rebuildable artifacts (WSL + project on /mnt/c).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOADING="$(cd "$ROOT/.." && pwd)"

echo "==> WSL /tmp cargo targets"
rm -rf /tmp/core-p2p-target /tmp/iroh-wasm-default /tmp/iroh-wasm-target /tmp/core-p2p-wasm-target 2>/dev/null || true

echo "==> WSL cargo wasm build cache"
rm -rf "$HOME/.cache/core-p2p-wasm-target" "$HOME/core-p2p-wasm-build" 2>/dev/null || true

echo "==> Stray CARGO_TARGET_DIR under core-p2p (Windows path bug)"
rm -rf "$ROOT/C:"* 2>/dev/null || true

echo "==> Rust target/ (native rebuild via build.sh)"
rm -rf "$ROOT/target" 2>/dev/null || true

echo "==> WASM feasibility probes (optional test crates)"
rm -rf "$ROOT/test-wasm-iroh" "$ROOT/test-wasm-iroh-default" 2>/dev/null || true

echo "==> sport-app .next + test-results"
rm -rf "$LOADING/sport-app/.next" "$LOADING/sport-app/test-results" "$LOADING/sport-app/tsconfig.tsbuildinfo" 2>/dev/null || true

echo "==> Done"
df -h /tmp /mnt/c 2>/dev/null | tail -2
