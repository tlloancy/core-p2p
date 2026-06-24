'use strict';

const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const { execSync } = require('child_process');

const {
  create_peer,
  send_blob,
  fetch_blob,
  shutdown_peer,
} = require('../index.node');

const ROOT = path.join(__dirname, '..');
const FIXTURE = path.join(__dirname, 'fixtures', 'sample.mp4');

function sha256(buf) {
  return crypto.createHash('sha256').update(buf).digest('hex');
}

async function main() {
  execSync('bash scripts/gen_sample.sh', { cwd: ROOT, stdio: 'pipe' });
  const fileBytes = fs.readFileSync(FIXTURE);
  const expectedHash = sha256(fileBytes);

  const peerA = await create_peer();
  const peerB = await create_peer();
  const peerC = await create_peer();

  await send_blob(peerA, peerB, fileBytes);
  await shutdown_peer(peerA);

  const received = await fetch_blob(peerC, peerB, expectedHash);
  const gotHash = sha256(Buffer.from(received));

  if (gotHash !== expectedHash) {
    console.error('FAIL: hash mismatch after relay fetch');
    process.exit(1);
  }

  console.log('PASS: peer_c received video — peer_a was offline');

  await shutdown_peer(peerB);
  await shutdown_peer(peerC);
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
