'use strict';

const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const { execSync } = require('child_process');

const {
  create_peer,
  send_blob,
  recv_blob,
  shutdown_peer,
} = require('../index.node');

const FIXTURE = path.join(__dirname, 'fixtures', 'sample.mp4');

function sha256(buf) {
  return crypto.createHash('sha256').update(buf).digest('hex');
}

async function main() {
  execSync('bash scripts/gen_sample.sh', { cwd: path.join(__dirname, '..'), stdio: 'inherit' });

  const fileBytes = fs.readFileSync(FIXTURE);
  const expectedHash = sha256(fileBytes);
  const sizeMb = (fileBytes.length / (1024 * 1024)).toFixed(2);

  const peerA = await create_peer();
  const peerB = await create_peer();

  const start = Date.now();
  const hash = await send_blob(peerA, peerB, fileBytes);
  const received = await recv_blob(peerB, hash, 120_000);
  const elapsed = Date.now() - start;

  const receivedHash = sha256(Buffer.from(received));
  if (receivedHash !== expectedHash || hash !== expectedHash) {
    console.error('FAIL: SHA256 mismatch');
    console.error('  expected:', expectedHash);
    console.error('  got:     ', receivedHash);
    process.exit(1);
  }

  console.log('PASS: SHA256 match — file integrity verified');
  console.log(`Transfer time: ${elapsed}ms | Size: ${sizeMb}MB`);

  await shutdown_peer(peerA);
  await shutdown_peer(peerB);
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
