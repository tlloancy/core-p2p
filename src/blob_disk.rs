//! On-disk blob persistence (`BLOB_STORE_DIR`, default `/var/lib/sport-p2p/blobs/`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn blob_store_dir() -> PathBuf {
    std::env::var("BLOB_STORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/sport-p2p/blobs"))
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn is_blob_hash(name: &str) -> bool {
    name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn persist_blob(hash: &str, data: &[u8]) -> std::io::Result<()> {
    let dir = blob_store_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(hash), data)
}

pub fn load_all_blobs() -> std::io::Result<HashMap<String, Vec<u8>>> {
    let dir = blob_store_dir();
    let mut map = HashMap::new();
    if !dir.is_dir() {
        return Ok(map);
    }

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_blob_hash(name) {
            continue;
        }
        let bytes = fs::read(&path)?;
        if sha256_hex(&bytes) == name {
            map.insert(name.to_string(), bytes);
        } else {
            eprintln!("warning: skipping blob with hash mismatch: {}", path.display());
        }
    }

    Ok(map)
}

pub fn load_blobs_from_dir(dir: &Path) -> std::io::Result<HashMap<String, Vec<u8>>> {
    let prev = std::env::var("BLOB_STORE_DIR").ok();
    std::env::set_var("BLOB_STORE_DIR", dir);
    let loaded = load_all_blobs();
    match prev {
        Some(p) => std::env::set_var("BLOB_STORE_DIR", p),
        None => std::env::remove_var("BLOB_STORE_DIR"),
    }
    loaded
}
