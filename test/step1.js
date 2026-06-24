'use strict';

const { create_peer, send_blob, recv_blob, shutdown_peer } = require('../index.node');

async function main() {
  const peerA = await create_peer();
  const peerB = await create_peer();

  const payload = 'Hello from peer_a — sport P2P step 1.1';
  const data = Buffer.from(payload, 'utf8');

  const start = Date.now();
  const hash = await send_blob(peerA, peerB, data);
  const received = await recv_blob(peerB, hash, 30_000);
  const latency = Date.now() - start;

  const receivedText = Buffer.from(received).toString('utf8');
  if (receivedText !== payload) {
    console.error('FAIL: payload mismatch');
    console.error('  expected:', payload);
    console.error('  got:     ', receivedText);
    process.exit(1);
  }

  console.log('PASS: peer_b received exactly what peer_a sent');
  console.log(`Latency: ${latency}ms`);

  await shutdown_peer(peerA);
  await shutdown_peer(peerB);
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
