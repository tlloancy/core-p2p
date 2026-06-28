//! Browser WASM — iroh relay-only peer for sport-p2p blob fetch.

use std::sync::Mutex;

use futures_util::{AsyncReadExt, AsyncWriteExt};
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointId};
use js_sys::Uint8Array;
use once_cell::sync::OnceCell;
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

const ALPN: &[u8] = b"sport-p2p/blob/0";
const MAX_BLOB: usize = 256 * 1024 * 1024;

struct PeerState {
    endpoint: Endpoint,
}

static PEER: OnceCell<Mutex<Option<PeerState>>> = OnceCell::new();

fn peer_slot() -> &'static Mutex<Option<PeerState>> {
    PEER.get_or_init(|| Mutex::new(None))
}

fn js_err(message: impl AsRef<str>) -> JsError {
    JsError::new(message.as_ref())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn validate_hash(hash: &str) -> Result<(), JsError> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(js_err("invalid hash: expected 64 hex chars"));
    }
    Ok(())
}

#[wasm_bindgen(start)]
pub fn wasm_start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn core_p2p_wasm_version() -> String {
    "0.2.0-wasm".to_string()
}

/// Create a relay-only iroh endpoint (N0 preset). Returns local node id.
#[wasm_bindgen]
pub async fn create_peer_wasm() -> Result<String, JsError> {
    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| js_err(format!("bind failed: {e}")))?;

    endpoint
        .online()
        .await
        .map_err(|e| js_err(format!("relay online failed: {e}")))?;

    let id = endpoint.id().to_string();
    *peer_slot().lock().map_err(|_| js_err("peer lock poisoned"))? = Some(PeerState { endpoint });
    Ok(id)
}

/// Fetch a chunk from a remote peer by hash (sport-p2p/blob/0 protocol).
#[wasm_bindgen]
pub async fn fetch_chunk_wasm(remote_peer_id: String, hash: String) -> Result<Uint8Array, JsError> {
    validate_hash(&hash)?;

    let guard = peer_slot()
        .lock()
        .map_err(|_| js_err("peer lock poisoned"))?;
    let state = guard
        .as_ref()
        .ok_or_else(|| js_err("call create_peer_wasm() first"))?;

    let remote: EndpointId = remote_peer_id
        .parse()
        .map_err(|e| js_err(format!("invalid peer id: {e}")))?;

    let conn = state
        .endpoint
        .connect(remote, ALPN)
        .await
        .map_err(|e| js_err(format!("connect failed: {e}")))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| js_err(format!("open_bi failed: {e}")))?;

    send.write_all(hash.as_bytes())
        .await
        .map_err(|e| js_err(format!("write failed: {e}")))?;
    send.finish()
        .map_err(|e| js_err(format!("finish failed: {e}")))?;

    let bytes = recv
        .read_to_end(MAX_BLOB)
        .await
        .map_err(|e| js_err(format!("read failed: {e}")))?;

    conn.close(0u32.into(), b"done");

    if sha256_hex(&bytes) != hash {
        return Err(js_err("fetch integrity mismatch"));
    }

    Ok(Uint8Array::from(&bytes[..]))
}
