'use strict';

/**
 * Dial a remote peer by node_id only (no IP/port) — uses iroh N0 preset:
 * pkarr DNS (iroh.link) + n0.computer DERP relays.
 *
 * Usage: node test/step11-relay.js <remote_node_id>
 */
const {
  create_peer,
  send_blob,
  fetch_blob,
  peer_endpoint,
  shutdown_peer,
} = require('../index.node');

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function withRetries(label, fn, { attempts = 4, delayMs = 10_000 } = {}) {
  let lastErr;
  for (let i = 1; i <= attempts; i += 1) {
    try {
      return await fn();
    } catch (err) {
      lastErr = err;
      if (i < attempts) {
        console.log(`${label} attempt ${i}/${attempts} failed: ${err.message}; retry in ${delayMs}ms`);
        await sleep(delayMs);
      }
    }
  }
  throw lastErr;
}

async function main() {
  const remoteId = process.argv[2];
  if (!remoteId) {
    console.error('usage: node test/step11-relay.js <remote_node_id>');
    process.exit(1);
  }

  const local = await create_peer();
  const localEndpoint = peer_endpoint(local);
  console.log(`local node_id=${local}`);
  console.log(`local endpoint=${localEndpoint}`);
  console.log(`dialing remote node_id=${remoteId} (no IP/port)`);

  const payload = Buffer.from('NAT traversal via n0 relay — step 11', 'utf8');
  const start = Date.now();
  const hash = await withRetries('send_blob', () => send_blob(local, remoteId, payload));
  console.log(`send_blob ok hash=${hash} (${Date.now() - start}ms)`);

  const fetched = Buffer.from(
    await withRetries('fetch_blob', () => fetch_blob(local, remoteId, hash))
  );
  if (!fetched.equals(payload)) {
    console.error('FAIL: fetch integrity mismatch');
    console.error('  expected:', payload.toString('utf8'));
    console.error('  got:     ', fetched.toString('utf8'));
    process.exit(1);
  }

  console.log(`PASS: node_id-only relay round-trip in ${Date.now() - start}ms`);
  await shutdown_peer(local);
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
