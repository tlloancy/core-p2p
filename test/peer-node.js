'use strict';

const crypto = require('crypto');
const {
  create_peer,
  fetch_blob,
  peer_endpoint,
  seed_blob,
  shutdown_peer,
} = require('../index.node');

function sha256(buf) {
  return crypto.createHash('sha256').update(buf).digest('hex');
}

/** Diversity key: first 3 IP octets; on localhost, port stands in for subnet. */
function subnetKey(endpoint) {
  const trimmed = endpoint.replace(/^\[/, '').replace(/\]$/, '');
  const lastColon = trimmed.lastIndexOf(':');
  const host = lastColon >= 0 ? trimmed.slice(0, lastColon) : trimmed;
  const port = lastColon >= 0 ? trimmed.slice(lastColon + 1) : '0';
  const octets = host.split('.');
  if (octets.length >= 3 && octets[0] === '127' && octets[1] === '0' && octets[2] === '0') {
    return `127.0.0.${port}`;
  }
  return octets.slice(0, 3).join('.');
}

class Peer {
  constructor(id, endpoint) {
    this.id = id;
    this.endpoint = endpoint;
    this._closed = false;
  }

  get subnet() {
    return subnetKey(this.endpoint);
  }

  /** Seed a chunk as soon as it is received (viewer becomes a seeder). */
  seedOnView(chunkHash, chunkData) {
    const data = Buffer.isBuffer(chunkData) ? chunkData : Buffer.from(chunkData);
    const hash = sha256(data);
    if (hash !== chunkHash) {
      throw new Error(`seedOnView hash mismatch: expected ${chunkHash}, got ${hash}`);
    }
    seed_blob(this.id, chunkHash, data);
  }

  close() {
    this._closed = true;
    return shutdown_peer(this.id);
  }

  get closed() {
    return this._closed;
  }
}

async function createPeer() {
  const id = await create_peer();
  const endpoint = peer_endpoint(id);
  return new Peer(id, endpoint);
}

/** Pick up to `n` peers on distinct subnets (port proxy on localhost). */
function selectDiversePeers(peers, n) {
  const alive = peers.filter((p) => p && !p.closed);
  const chosen = [];
  const seen = new Set();

  for (const peer of alive) {
    if (chosen.length >= n) break;
    const key = peer.subnet;
    if (seen.has(key)) continue;
    seen.add(key);
    chosen.push(peer);
  }

  for (const peer of alive) {
    if (chosen.length >= n) break;
    if (!chosen.includes(peer)) chosen.push(peer);
  }

  return chosen.slice(0, n);
}

/**
 * Race up to 3 diverse peers for a chunk; seed locally on first response.
 * @param {string} chunkHash
 * @param {Peer[]} peers candidate sources (excludes viewer)
 * @param {Peer} viewer receiving peer
 * @param {{ onBuffer?: () => void }} [opts]
 */
async function fetchChunk(chunkHash, peers, viewer, opts = {}) {
  const local = viewer.id;
  try {
    const cached = await fetch_blob(local, local, chunkHash);
    return Buffer.from(cached);
  } catch {
    // not cached locally
  }

  const candidates = selectDiversePeers(
    peers.filter((p) => p.id !== local),
    3
  );
  if (candidates.length === 0) {
    throw new Error('fetchChunk: no peer candidates');
  }

  let chunk;
  try {
    chunk = await Promise.any(
      candidates.map((peer) =>
        fetch_blob(local, peer.id, chunkHash).then((buf) => Buffer.from(buf))
      )
    );
  } catch (err) {
    if (opts.onBuffer) opts.onBuffer();
    throw err;
  }

  viewer.seedOnView(chunkHash, chunk);
  return chunk;
}

module.exports = {
  Peer,
  createPeer,
  selectDiversePeers,
  fetchChunk,
  seedOnView: (peer, chunkHash, chunkData) => peer.seedOnView(chunkHash, chunkData),
  subnetKey,
};
