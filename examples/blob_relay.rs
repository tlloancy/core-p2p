use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use core_p2p::blob_disk;
use iroh::address_lookup::pkarr::PkarrResolver;
use iroh::endpoint::Connection;
use iroh::endpoint::presets;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, PublicKey};
use sha2::{Digest, Sha256};

const ALPN: &[u8] = b"sport-p2p/blob/0";
const MAX_BLOB: usize = 256 * 1024 * 1024;

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
        insert_blob(&self.store, hash.clone(), data.clone());
        let _ = send.write_all(hash.as_bytes()).await;
        let _ = send.finish();
        connection.closed().await;
        Ok(())
    }
}

fn insert_blob(store: &BlobStore, hash: String, data: Vec<u8>) {
    store.lock().unwrap().insert(hash.clone(), data.clone());
    if let Err(err) = blob_disk::persist_blob(&hash, &data) {
        eprintln!("warning: persist blob {hash}: {err}");
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    match mode.as_str() {
        "serve" => serve().await,
        "dial" => {
            let remote = std::env::args().nth(2).expect("usage: blob_relay dial <node_id>");
            dial(&remote).await
        }
        other => Err(format!("unknown mode {other}").into()),
    }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let initial = blob_disk::load_all_blobs().unwrap_or_else(|err| {
        eprintln!("warning: load blobs from disk: {err}");
        HashMap::new()
    });
    eprintln!(
        "blob_relay: loaded {} blob(s) from {}",
        initial.len(),
        blob_disk::blob_store_dir().display()
    );
    let store: BlobStore = Arc::new(Mutex::new(initial));
    let endpoint = bind().await?;
    let _router = Router::builder(endpoint.clone())
        .accept(ALPN, BlobAcceptor { store })
        .spawn();
    endpoint.online().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("NODE_ID={}", endpoint.id());
    println!("ENDPOINT={:?}", endpoint.addr());
    std::future::pending::<()>().await;
    Ok(())
}

async fn dial(remote_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = bind().await?;
    client.online().await;
    let remote = PublicKey::from_str(remote_id)?;
    let conn = tokio::time::timeout(
        Duration::from_secs(90),
        client.connect(EndpointAddr::new(remote), ALPN),
    )
    .await
    .map_err(|_| "connect timed out")??;
    println!("PASS connected remote={}", conn.remote_id());
    conn.close(0u32.into(), b"done");
    client.close().await;
    Ok(())
}

async fn bind() -> Result<Endpoint, Box<dyn std::error::Error>> {
    Ok(Endpoint::builder(presets::N0)
        .address_lookup(PkarrResolver::n0_dns())
        .bind_addr("0.0.0.0:0")?
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?)
}
