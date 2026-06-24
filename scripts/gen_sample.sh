#!/usr/bin/env bash
# Generate a 30-second test video (H.264/AAC) for P2P transfer tests.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p test/fixtures
OUT="test/fixtures/sample.mp4"
if [ -f "$OUT" ]; then
  echo "exists: $OUT"
  exit 0
fi
ffmpeg -y -f lavfi -i testsrc=duration=30:size=640x360:rate=30 \
  -f lavfi -i sine=frequency=440:duration=30 \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$OUT"
echo "generated: $OUT ($(du -h "$OUT" | cut -f1))"
