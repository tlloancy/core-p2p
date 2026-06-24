//! core-p2p — P2P video distribution engine built on iroh, exposed to Node.js.
//!
//! iroh uses QUIC/UDP and is compiled as a native Node addon (napi-rs). Browser
//! transport is handled separately in Layer 4 using the same chunk/hash model.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

const ALPN: &[u8] = b"sport-p2p/blob/0";
const MAX_BLOB: usize = 256 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

type BlobStore = Arc<Mutex<HashMap<String, Vec<u8>>>>;

struct Inner {
    endpoint: Endpoint,
    _router: Router,
}

static PEERS: Lazy<Mutex<HashMap<String, Arc<Inner>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static SHARED_STORES: Lazy<Mutex<HashMap<String, BlobStore>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
struct BlobAcceptor {
    store: BlobStore,
}

impl ProtocolHandler for BlobAcceptor {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let Ok((mut send, mut recv)) = connection.accept_bi().await else {
            return Ok(());
        };
        let Ok(data) = recv.read_to_end(MAX_BLOB).await else {
            return Ok(());
        };

        // 64-char hex payload = fetch-by-hash request.
        if data.len() == 64 && data.iter().all(|b| b.is_ascii_hexdigit()) {
            let hash = String::from_utf8_lossy(&data).to_string();
            let cached = self.store.lock().unwrap().get(&hash).cloned();
            if let Some(bytes) = cached {
                let _ = send.write_all(&bytes).await;
                let _ = send.finish();
                connection.closed().await;
                return Ok(());
            }
        }

        let hash = sha256_hex(&data);
        self.store.lock().unwrap().insert(hash.clone(), data.clone());
        let _ = send.write_all(hash.as_bytes()).await;
        let _ = send.finish();
        connection.closed().await;
        Ok(())
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn get_peer(node_id: &str) -> napi::Result<Arc<Inner>> {
    PEERS
        .lock()
        .unwrap()
        .get(node_id)
        .cloned()
        .ok_or_else(|| napi::Error::from_reason(format!("unknown peer: {node_id}")))
}

fn shared_store(node_id: &str) -> napi::Result<BlobStore> {
    SHARED_STORES
        .lock()
        .unwrap()
        .get(node_id)
        .cloned()
        .ok_or_else(|| napi::Error::from_reason(format!("no store for peer: {node_id}")))
}

async fn connect_with_timeout(
    from: &Endpoint,
    addr: EndpointAddr,
) -> napi::Result<Connection> {
    tokio::time::timeout(CONNECT_TIMEOUT, from.connect(addr, ALPN))
        .await
        .map_err(|_| napi::Error::from_reason("connect timed out"))?
        .map_err(|e| napi::Error::from_reason(format!("connect failed: {e}")))
}

async fn transfer_blob(
    from: &Endpoint,
    to_addr: EndpointAddr,
    bytes: Vec<u8>,
) -> napi::Result<String> {
    let hash = sha256_hex(&bytes);
    let conn = connect_with_timeout(from, to_addr).await?;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| napi::Error::from_reason(format!("open_bi failed: {e}")))?;

    send.write_all(&bytes)
        .await
        .map_err(|e| napi::Error::from_reason(format!("write failed: {e}")))?;
    send.finish()
        .map_err(|e| napi::Error::from_reason(format!("finish failed: {e}")))?;

    let ack = recv
        .read_to_end(128)
        .await
        .map_err(|e| napi::Error::from_reason(format!("ack read failed: {e}")))?;
    conn.close(0u32.into(), b"done");

    let ack_hash = String::from_utf8_lossy(&ack).to_string();
    if ack_hash != hash {
        return Err(napi::Error::from_reason(format!(
            "integrity mismatch: sent {hash} but receiver acked {ack_hash}"
        )));
    }
    Ok(hash)
}

/// Create a new peer bound on localhost for direct QUIC connectivity in tests.
#[napi(js_name = "create_peer")]
pub async fn create_peer() -> napi::Result<String> {
    let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .map_err(|e| napi::Error::from_reason(format!("bind_addr failed: {e}")))?
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| napi::Error::from_reason(format!("bind failed: {e}")))?;

    let node_id = endpoint.id().to_string();
    let store: BlobStore = Arc::new(Mutex::new(HashMap::new()));
    let acceptor = BlobAcceptor { store: store.clone() };
    let router = Router::builder(endpoint.clone())
        .accept(ALPN, acceptor)
        .spawn();

    SHARED_STORES
        .lock()
        .unwrap()
        .insert(node_id.clone(), store);
    PEERS.lock().unwrap().insert(
        node_id.clone(),
        Arc::new(Inner {
            endpoint,
            _router: router,
        }),
    );

    // Let the router bind before callers dial.
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(node_id)
}

/// Send `data` from peer `from` to peer `to`. Returns sha256 hex of the payload.
#[napi(js_name = "send_blob")]
pub async fn send_blob(from: String, to: String, data: Buffer) -> napi::Result<String> {
    let from_inner = get_peer(&from)?;
    let to_inner = get_peer(&to)?;
    let bytes: Vec<u8> = data.to_vec();
    let hash = sha256_hex(&bytes);

    shared_store(&from)?
        .lock()
        .unwrap()
        .insert(hash.clone(), bytes.clone());

    let target_addr = to_inner.endpoint.addr();
    transfer_blob(&from_inner.endpoint, target_addr, bytes).await?;
    Ok(hash)
}

/// Fetch a blob by hash from a remote peer into the local store.
#[napi(js_name = "fetch_blob")]
pub async fn fetch_blob(local: String, remote: String, hash: String) -> napi::Result<Buffer> {
    if shared_store(&local)?.lock().unwrap().contains_key(&hash) {
        let bytes = shared_store(&local)?
            .lock()
            .unwrap()
            .get(&hash)
            .cloned()
            .unwrap();
        return Ok(Buffer::from(bytes));
    }

    let local_inner = get_peer(&local)?;
    let remote_inner = get_peer(&remote)?;
    let target_addr = remote_inner.endpoint.addr();

    // Request by sending the hash as a lookup key; remote re-sends stored bytes.
    let conn = connect_with_timeout(&local_inner.endpoint, target_addr).await?;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| napi::Error::from_reason(format!("open_bi failed: {e}")))?;

    send.write_all(hash.as_bytes())
        .await
        .map_err(|e| napi::Error::from_reason(format!("write failed: {e}")))?;
    send.finish()
        .map_err(|e| napi::Error::from_reason(format!("finish failed: {e}")))?;

    let bytes = recv
        .read_to_end(MAX_BLOB)
        .await
        .map_err(|e| napi::Error::from_reason(format!("read failed: {e}")))?;
    conn.close(0u32.into(), b"done");

    if sha256_hex(&bytes) != hash {
        return Err(napi::Error::from_reason(format!(
            "fetch integrity mismatch for {hash}"
        )));
    }

    shared_store(&local)?
        .lock()
        .unwrap()
        .insert(hash, bytes.clone());
    Ok(Buffer::from(bytes))
}

/// Wait until `peer` has a blob with `hash`, then return it.
#[napi(js_name = "recv_blob")]
pub async fn recv_blob(
    peer: String,
    hash: String,
    timeout_ms: Option<u32>,
) -> napi::Result<Buffer> {
    let store = shared_store(&peer)?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.unwrap_or(30_000) as u64);
    loop {
        if let Some(bytes) = store.lock().unwrap().get(&hash) {
            return Ok(Buffer::from(bytes.clone()));
        }
        if Instant::now() >= deadline {
            return Err(napi::Error::from_reason(format!(
                "timeout waiting for blob {hash} on peer {peer}"
            )));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[napi(js_name = "received_hashes")]
pub fn received_hashes(peer: String) -> napi::Result<Vec<String>> {
    let store = shared_store(&peer)?;
    let keys: Vec<String> = store.lock().unwrap().keys().cloned().collect();
    Ok(keys)
}

#[napi(js_name = "shutdown_peer")]
pub async fn shutdown_peer(peer: String) -> napi::Result<()> {
    let inner = PEERS.lock().unwrap().remove(&peer);
    SHARED_STORES.lock().unwrap().remove(&peer);
    if let Some(inner) = inner {
        inner.endpoint.close().await;
    }
    Ok(())
}
