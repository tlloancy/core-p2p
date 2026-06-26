use std::str::FromStr;
use std::time::Duration;

use iroh::address_lookup::pkarr::PkarrResolver;
use iroh::endpoint::Connection;
use iroh::endpoint::presets;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, PublicKey};

const ALPN: &[u8] = b"sport-p2p/blob/0";

#[derive(Clone, Debug)]
struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        conn.closed().await;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    match mode.as_str() {
        "serve" => serve().await,
        "dial" => {
            let remote = std::env::args().nth(2).expect("usage: relay_connect dial <node_id>");
            dial(&remote).await
        }
        other => Err(format!("unknown mode {other}").into()),
    }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let server = bind().await?;
    let _router = Router::builder(server.clone())
        .accept(ALPN, Echo)
        .spawn();
    server.online().await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    println!("NODE_ID={}", server.id());
    println!("ENDPOINT={:?}", server.addr());
    std::future::pending::<()>().await;
    Ok(())
}

async fn dial(remote_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = bind().await?;
    client.online().await;
    let remote = PublicKey::from_str(remote_id)?;
    let mut addr = EndpointAddr::new(remote);
    if let Some(relay) = std::env::args().nth(3) {
        addr = addr.with_relay_url(relay.parse()?);
        println!("using explicit relay {relay}");
    }
    println!("dialing {remote_id}…");
    let conn = tokio::time::timeout(Duration::from_secs(90), client.connect(addr, ALPN))
        .await
        .map_err(|_| "connect timed out")??;
    println!("PASS connected remote={}", conn.remote_id());
    conn.close(0u32.into(), b"done");
    client.close().await;
    Ok(())
}

async fn bind() -> Result<Endpoint, Box<dyn std::error::Error>> {
    let ep = Endpoint::builder(presets::N0)
        .address_lookup(PkarrResolver::n0_dns())
        .bind_addr("0.0.0.0:0")?
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    Ok(ep)
}
