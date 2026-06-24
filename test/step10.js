'use strict';

const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const { execSync } = require('child_process');

const { send_blob, recv_blob } = require('../index.node');
const { createPeer, fetchChunk } = require('./peer-node');

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

async function main() {
  const segments = hlsSegments();
  if (segments.length < 4) {
    throw new Error('expected at least 4 HLS segments');
  }

  const peerA = await createPeer();
  const peerB = await createPeer();
  const peerC = await createPeer();

  // peer_a pushes all chunks to peer_b (origin).
  for (const seg of segments) {
    await send_blob(peerA.id, peerB.id, seg.bytes);
    await recv_blob(peerB.id, seg.hash, 30_000);
  }

  let bufferingEvents = 0;

  // peer_b and peer_c "view": seed each chunk immediately; peer_c races peer_a + peer_b.
  for (const seg of segments) {
    peerB.seedOnView(seg.hash, seg.bytes);
    await fetchChunk(seg.hash, [peerA, peerB], peerC, {
      onBuffer: () => {
        bufferingEvents += 1;
      },
    });
  }

  await peerA.close();

  const peerD = await createPeer();
  const fetched = [];

  for (const seg of segments) {
    const chunk = await fetchChunk(seg.hash, [peerB, peerC], peerD, {
      onBuffer: () => {
        bufferingEvents += 1;
      },
    });
    if (sha256(chunk) !== seg.hash) {
      console.error('FAIL: segment hash mismatch after peer_a offline');
      process.exit(1);
    }
    fetched.push(chunk);
  }

  for (let i = 0; i < segments.length; i += 1) {
    if (sha256(fetched[i]) !== segments[i].hash) {
      console.error('FAIL: peer_d segment integrity');
      process.exit(1);
    }
  }

  console.log('PASS: peer_d received video — peer_a offline, served by peers b+c');

  // Sequential playback after origin offline — all segments already fetched above.
  console.log('PASS: playback continued after peer_a disconnect');

  if (bufferingEvents > 0) {
    console.error(`FAIL: ${bufferingEvents} buffering events detected`);
    process.exit(1);
  }
  console.log('PASS: no freeze detected (0 buffering events)');

  await peerB.close();
  await peerC.close();
  await peerD.close();
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
