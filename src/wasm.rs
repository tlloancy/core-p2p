//! WASM build — browser transport stub (Layer 4). No iroh / mio on this target.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn core_p2p_wasm_version() -> String {
    "0.1.0-wasm".to_string()
}
