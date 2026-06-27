//!
//! # wRPC Client for Rusty Kaspa p2p Node
//!
//! This crate provides a WebSocket RPC client for Rusty Kaspa p2p node. It is based on the
//! [wRPC](https://docs.rs/workflow-rpc) crate that offers WebSocket RPC implementation
//! for Rust based on Borsh and Serde JSON serialization. wRPC is a lightweight RPC framework
//! meant to function as an IPC (Inter-Process Communication) mechanism for Rust applications.
//!
//! Rust examples on using wRPC client can be found in the
//! [examples](https://github.com/kaspanet/rusty-kaspa/tree/master/rpc/wrpc/examples) folder.
//!
//! WASM bindings for wRPC client can be found in the [`kaspa-wrpc-wasm`](https://docs.rs/kaspa-wrpc-wasm) crate.
//!
//! The main struct managing Kaspa RPC client connections is the [`KaspaRpcClient`].
//!

pub mod client;
pub mod error;
mod imports;
pub mod result;
pub use imports::{KaspaRpcClient, Resolver, WrpcEncoding};
pub mod node;
pub mod parse;
pub mod prelude;
pub mod resolver;

/// Ensures a process-level rustls [`CryptoProvider`](rustls::crypto::CryptoProvider)
/// is installed before the client establishes any TLS connection.
///
/// reqwest 0.13 (used by the [`Resolver`] over HTTPS) only auto-selects the
/// `aws-lc-rs` provider, which requires a C/cmake toolchain. To keep the build
/// pure-Rust (`ring`) while staying zero-config for SDK consumers, we install
/// the `ring` provider on first client/resolver construction. This is
/// idempotent and a no-op if a provider was already installed — so a consumer
/// can override it by installing their own provider before constructing a
/// client. wasm targets use the host's TLS and need no provider.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn ensure_crypto_provider() {}
