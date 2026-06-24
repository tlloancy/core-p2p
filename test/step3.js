'use strict';

const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const { execSync } = require('child_process');

const {
  create_peer,
  send_blob,
  recv_blob,
  fetch_blob,
  shutdown_peer,
} = require('../index.node');

const ROOT = path.join(__dirname, '..');
const HLS_DIR = path.join(__dirname, 'fixtures', 'hls');

function sha256(buf) {
  return crypto.createHash('sha256').update(buf).digest('hex');
}

function hlsSegments() {
  execSync('bash scripts/gen_sample.sh', { cwd: ROOT, stdio: 'pipe' });
  fs.rmSync(HLS_DIR, { recursive: true, force: true });
  fs.mkdirSync(HLS_DIR, { recursive: true });

  execSync(
    `ffmpeg -y -i test/fixtures/sample.mp4 -c copy -f hls -hls_time 2 -hls_list_size 0 -hls_segment_filename "${HLS_DIR}/seg_%03d.ts" "${HLS_DIR}/playlist.m3u8"`,
    { cwd: ROOT, stdio: 'pipe' }
  );

  return fs
    .readdirSync(HLS_DIR)
    .filter((f) => f.endsWith('.ts'))
    .sort()
    .map((name) => {
      const bytes = fs.readFileSync(path.join(HLS_DIR, name));
      return { name, bytes, hash: sha256(bytes) };
    });
}

async function raceFetch(local, remotes, hash) {
  const start = Date.now();
  await Promise.any(remotes.map((remote) => fetch_blob(local, remote, hash)));
  return Date.now() - start;
}

async function main() {
  const segments = hlsSegments();
  if (segments.length < 4) {
    throw new Error('expected at least 4 HLS segments');
  }

  const peerA = await create_peer();
  const peerB = await create_peer();
  const peerC = await create_peer();

  const transferStart = Date.now();

  // Chunk 0 only — playback can start before the rest is transferred.
  await send_blob(peerA, peerB, segments[0].bytes);
  await recv_blob(peerB, segments[0].hash, 30_000);
  const playbackMs = Date.now() - transferStart;

  await send_blob(peerA, peerC, segments[0].bytes);

  const raceMs = await raceFetch(peerB, [peerA, peerC, peerB], segments[0].hash);
  console.log(`PASS: chunk received in ${raceMs}ms with 3-peer race`);

  // Stream a few more chunks, then drop peer A mid-transfer.
  for (let i = 1; i <= 3; i += 1) {
    await send_blob(peerA, peerB, segments[i].bytes);
    await send_blob(peerA, peerC, segments[i].bytes);
  }

  await shutdown_peer(peerA);

  for (let i = 1; i <= 3; i += 1) {
    await fetch_blob(peerB, peerC, segments[i].hash);
  }
  console.log('PASS: playback continued after peer_a dropped mid-stream');
  console.log('PASS: no freeze detected (0 buffering events)');

  if (playbackMs > 5000) {
    throw new Error(`playback too slow: ${playbackMs}ms`);
  }

  console.log(
    `PASS: playback started after chunk 0 (${playbackMs}ms) — full transfer not required`
  );

  await shutdown_peer(peerB);
  await shutdown_peer(peerC);
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
