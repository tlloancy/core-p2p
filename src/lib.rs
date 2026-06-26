//! core-p2p — P2P video distribution engine.
//!
//! Native: iroh QUIC exposed via napi-rs (`index.node`).
//! WASM: wasm-bindgen stub for browser transport (Layer 4).

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
