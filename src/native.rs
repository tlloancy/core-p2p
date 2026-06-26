//! Native Node.js addon — iroh QUIC peer mesh (not compiled for wasm32).

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use iroh::address_lookup::pkarr::PkarrResolver;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, PublicKey};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

const ALPN: &[u8] = b"sport-p2p/blob/0";
const MAX_BLOB: usize = 256 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(90);
const RELAY_ONLINE_TIMEOUT: Duration = Duration::from_secs(45);
const PKARR_PUBLISH_WAIT: Duration = Duration::from_secs(2);

type BlobStore = Arc<Mutex<HashMap<String, Vec<u8>>>>;

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

enum PeerCommand {
    ProbeConnect {
        remote: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SendBlob {
        to: String,
        data: Vec<u8>,
        reply: oneshot::Sender<Result<String, String>>,
    },
    FetchBlob {
        remote: String,
        hash: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

struct PeerHandle {
    bind_addr: String,
    store: BlobStore,
    cmd_tx: mpsc::Sender<PeerCommand>,
    _thread: JoinHandle<()>,
}

static PEERS: Lazy<Mutex<HashMap<String, Arc<PeerHandle>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn get_peer(node_id: &str) -> napi::Result<Arc<PeerHandle>> {
    PEERS
        .lock()
        .unwrap()
        .get(node_id)
        .cloned()
        .ok_or_else(|| napi::Error::from_reason(format!("unknown peer: {node_id}")))
}

fn shared_store(node_id: &str) -> napi::Result<BlobStore> {
    Ok(get_peer(node_id)?.store.clone())
}

fn resolve_remote_addr(remote: &str) -> Result<EndpointAddr, String> {
    if let Some(peer) = PEERS.lock().unwrap().get(remote) {
        return Ok(peer_endpoint_addr(&peer.bind_addr, remote)?);
    }
    let id = PublicKey::from_str(remote).map_err(|e| format!("invalid remote node id: {e}"))?;
    Ok(EndpointAddr::new(id))
}

fn peer_endpoint_addr(bind_addr: &str, fallback_id: &str) -> Result<EndpointAddr, String> {
    if let Some(start) = bind_addr.find("PublicKey(") {
        let rest = &bind_addr[start + 10..];
        if let Some(end) = rest.find(')') {
            let id = PublicKey::from_str(&rest[..end]).map_err(|e| e.to_string())?;
            return Ok(EndpointAddr::new(id));
        }
    }
    let id = PublicKey::from_str(fallback_id).map_err(|e| e.to_string())?;
    Ok(EndpointAddr::new(id))
}

async fn connect_with_timeout(from: &Endpoint, addr: EndpointAddr) -> Result<Connection, String> {
    tokio::time::timeout(CONNECT_TIMEOUT, from.connect(addr, ALPN))
        .await
        .map_err(|_| "connect timed out".to_string())?
        .map_err(|e| format!("connect failed: {e}"))
}

async fn transfer_blob(from: &Endpoint, to_addr: EndpointAddr, bytes: Vec<u8>) -> Result<String, String> {
    let hash = sha256_hex(&bytes);
    let conn = connect_with_timeout(from, to_addr).await?;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi failed: {e}"))?;

    send.write_all(&bytes)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    send.finish()
        .map_err(|e| format!("finish failed: {e}"))?;

    let ack = recv
        .read_to_end(128)
        .await
        .map_err(|e| format!("ack read failed: {e}"))?;
    conn.close(0u32.into(), b"done");

    let ack_hash = String::from_utf8_lossy(&ack).to_string();
    if ack_hash != hash {
        return Err(format!(
            "integrity mismatch: sent {hash} but receiver acked {ack_hash}"
        ));
    }
    Ok(hash)
}

async fn fetch_blob_from(
    from: &Endpoint,
    remote: &str,
    hash: &str,
) -> Result<Vec<u8>, String> {
    let target_addr = resolve_remote_addr(remote)?;
    let conn = connect_with_timeout(from, target_addr).await?;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi failed: {e}"))?;

    send.write_all(hash.as_bytes())
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    send.finish()
        .map_err(|e| format!("finish failed: {e}"))?;

    let bytes = recv
        .read_to_end(MAX_BLOB)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    conn.close(0u32.into(), b"done");

    if sha256_hex(&bytes) != hash {
        return Err(format!("fetch integrity mismatch for {hash}"));
    }
    Ok(bytes)
}

async fn run_peer(
    store: BlobStore,
    mut cmd_rx: mpsc::Receiver<PeerCommand>,
    ready_tx: oneshot::Sender<Result<(String, String), String>>,
) {
    let setup = async {
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .address_lookup(PkarrResolver::n0_dns())
            .bind_addr("0.0.0.0:0")
            .map_err(|e| format!("bind_addr failed: {e}"))?
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| format!("bind failed: {e}"))?;

        let node_id = endpoint.id().to_string();
        let acceptor = BlobAcceptor { store: store.clone() };
        let router = Router::builder(endpoint.clone())
            .accept(ALPN, acceptor)
            .spawn();

        tokio::time::timeout(RELAY_ONLINE_TIMEOUT, endpoint.online())
            .await
            .map_err(|_| "relay online timed out".to_string())?;
        tokio::time::sleep(PKARR_PUBLISH_WAIT).await;
        let bind_addr = format!("{:?}", endpoint.addr());
        Ok::<_, String>((endpoint, router, node_id, bind_addr))
    }
    .await;

    let (endpoint, router, node_id, bind_addr) = match setup {
        Ok(v) => v,
        Err(err) => {
            eprintln!("core-p2p peer thread error: {err}");
            let _ = ready_tx.send(Err(err));
            return;
        }
    };

    let _ = ready_tx.send(Ok((node_id.clone(), bind_addr.clone())));

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            PeerCommand::ProbeConnect { remote, reply } => {
                let result = async {
                    let target = resolve_remote_addr(&remote)?;
                    let conn = connect_with_timeout(&endpoint, target).await?;
                    conn.close(0u32.into(), b"probe");
                    Ok(())
                }
                .await;
                let _ = reply.send(result);
            }
            PeerCommand::SendBlob { to, data, reply } => {
                let result = async {
                    let hash = sha256_hex(&data);
                    store.lock().unwrap().insert(hash.clone(), data.clone());
                    let target = resolve_remote_addr(&to)?;
                    transfer_blob(&endpoint, target, data).await
                }
                .await;
                let _ = reply.send(result);
            }
            PeerCommand::FetchBlob { remote, hash, reply } => {
                let result = fetch_blob_from(&endpoint, &remote, &hash).await;
                let _ = reply.send(result);
            }
            PeerCommand::Shutdown { reply } => {
                let _ = reply.send(());
                let _ = router.shutdown().await;
                endpoint.close().await;
                break;
            }
        }
    }
}

async fn start_peer() -> napi::Result<(String, Arc<PeerHandle>)> {
    let store: BlobStore = Arc::new(Mutex::new(HashMap::new()));
    let (ready_tx, ready_rx) = oneshot::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel(32);

    let store_for_thread = store.clone();
    let join = thread::Builder::new()
        .name("core-p2p-peer".into())
        .spawn(move || {
            let rt = Runtime::new().expect("peer runtime");
            rt.block_on(run_peer(store_for_thread, cmd_rx, ready_tx));
        })
        .map_err(|e| napi::Error::from_reason(format!("spawn peer thread failed: {e}")))?;

    let (node_id, bind_addr) = ready_rx
        .await
        .map_err(|_| napi::Error::from_reason("peer thread exited before ready"))?
        .map_err(|e| napi::Error::from_reason(e))?;

    let handle = Arc::new(PeerHandle {
        bind_addr,
        store,
        cmd_tx,
        _thread: join,
    });
    PEERS.lock().unwrap().insert(node_id.clone(), handle.clone());
    Ok((node_id, handle))
}

async fn peer_request<T>(
    peer: &PeerHandle,
    build: impl FnOnce(oneshot::Sender<T>) -> PeerCommand,
) -> napi::Result<T> {
    let (reply_tx, reply_rx) = oneshot::channel();
    peer.cmd_tx
        .send(build(reply_tx))
        .await
        .map_err(|_| napi::Error::from_reason("peer thread stopped"))?;
    reply_rx
        .await
        .map_err(|_| napi::Error::from_reason("peer thread dropped response"))
}

#[napi(js_name = "probe_connect")]
pub async fn probe_connect(local: String, remote: String) -> napi::Result<()> {
    let peer = get_peer(&local)?;
    peer_request(&peer, |reply| PeerCommand::ProbeConnect { remote, reply })
        .await?
        .map_err(|e| napi::Error::from_reason(e))
}

#[napi(js_name = "create_peer")]
pub async fn create_peer() -> napi::Result<String> {
    let (node_id, _) = start_peer().await?;
    Ok(node_id)
}

#[napi(js_name = "send_blob")]
pub async fn send_blob(from: String, to: String, data: Buffer) -> napi::Result<String> {
    let peer = get_peer(&from)?;
    let bytes: Vec<u8> = data.to_vec();
    let result = peer_request(&peer, |reply| PeerCommand::SendBlob {
        to,
        data: bytes,
        reply,
    })
    .await?;
    result.map_err(|e| napi::Error::from_reason(e))
}

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

    let peer = get_peer(&local)?;
    let hash_for_store = hash.clone();
    let bytes = peer_request(&peer, |reply| PeerCommand::FetchBlob { remote, hash, reply })
        .await?
        .map_err(|e| napi::Error::from_reason(e))?;

    shared_store(&local)?
        .lock()
        .unwrap()
        .insert(hash_for_store, bytes.clone());
    Ok(Buffer::from(bytes))
}

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

#[napi(js_name = "peer_endpoint")]
pub fn peer_endpoint(peer: String) -> napi::Result<String> {
    Ok(get_peer(&peer)?.bind_addr.clone())
}

#[napi(js_name = "seed_blob")]
pub fn seed_blob(peer: String, hash: String, data: Buffer) -> napi::Result<()> {
    let bytes: Vec<u8> = data.to_vec();
    if sha256_hex(&bytes) != hash {
        return Err(napi::Error::from_reason(format!(
            "seed_blob hash mismatch: expected {hash}"
        )));
    }
    shared_store(&peer)?
        .lock()
        .unwrap()
        .insert(hash, bytes);
    Ok(())
}

#[napi(js_name = "shutdown_peer")]
pub async fn shutdown_peer(peer: String) -> napi::Result<()> {
    let handle = PEERS.lock().unwrap().remove(&peer);
    if let Some(handle) = handle {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = handle
            .cmd_tx
            .send(PeerCommand::Shutdown { reply: reply_tx })
            .await;
        let _ = reply_rx.await;
    }
    Ok(())
}
