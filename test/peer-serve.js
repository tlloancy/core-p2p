'use strict';

/**
 * Standalone peer listener for cross-process relay tests.
 * Prints NODE_ID= and ENDPOINT= on stdout, then stays alive until SIGINT.
 */
const { create_peer, peer_endpoint, shutdown_peer } = require('../index.node');

async function main() {
  const id = await create_peer();
  const endpoint = peer_endpoint(id);
  console.log(`NODE_ID=${id}`);
  console.log(`ENDPOINT=${endpoint}`);

  const shutdown = async () => {
    await shutdown_peer(id);
    process.exit(0);
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);

  await new Promise(() => {});
}

main().catch((err) => {
  console.error('FAIL:', err);
  process.exit(1);
});
