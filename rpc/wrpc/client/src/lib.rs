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
//! ## TLS crypto provider (native)
//!
//! On native targets the client opens secure (`wss://`) connections and, when a
//! [`Resolver`] is used, fetches the public node list over HTTPS — both go
//! through [`rustls`](https://docs.rs/rustls), which (since 0.23) requires a
//! process-level crypto provider to be installed.
//!
//! For zero-config use, the default `rustls-tls-webpki-roots` feature installs
//! the pure-Rust `ring` provider automatically the first time a
//! [`KaspaRpcClient`] or [`Resolver`] is constructed (see
//! [`ensure_crypto_provider`]). The install is idempotent and a no-op if a
//! provider has already been installed, so to use a different provider (e.g.
//! `aws-lc-rs`) install it yourself *before* constructing a client:
//!
//! ```ignore
//! rustls::crypto::aws_lc_rs::default_provider().install_default().unwrap();
//! ```
//!
//! Building this crate with `default-features = false` (or selecting a
//! `native-tls*` backend) drops the bundled `ring` provider; if you then use the
//! [`Resolver`] (HTTPS) you must install a rustls [`CryptoProvider`] yourself —
//! [`ensure_crypto_provider`] becomes a no-op in that configuration.
//!
//! On `wasm32` the host environment handles TLS, so no provider is required.
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
///
/// When built without a `rustls-tls-*` backend (e.g. `default-features = false`
/// or a `native-tls*` backend) the `ring` provider is not compiled in and this
/// function is a no-op; such consumers must install their own provider.
#[cfg(all(not(target_arch = "wasm32"), feature = "rustls-ring"))]
pub fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "rustls-ring")))]
pub fn ensure_crypto_provider() {}

#[cfg(target_arch = "wasm32")]
pub fn ensure_crypto_provider() {}
