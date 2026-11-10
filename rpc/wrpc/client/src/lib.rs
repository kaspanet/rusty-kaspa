//!
//! # wRPC Client for Rusty Aero p2p Node
//!
//! This crate provides a WebSocket RPC client for Rusty Aero p2p node. It is based on the
//! [wRPC](https://docs.rs/workflow-rpc) crate that offers WebSocket RPC implementation
//! for Rust based on Borsh and Serde JSON serialization. wRPC is a lightweight RPC framework
//! meant to function as an IPC (Inter-Process Communication) mechanism for Rust applications.
//!
//! Rust examples on using wRPC client can be found in the
//! [examples](https://github.com/arkunati-png/rusty-aero/tree/master/rpc/wrpc/examples) folder.
//!
//! WASM bindings for wRPC client can be found in the [`aero-wrpc-wasm`](https://docs.rs/kaspa-wrpc-wasm) crate.
//!
//! The main struct managing Aero RPC client connections is the [`AeroRpcClient`].
//!

pub mod client;
pub mod error;
mod imports;
pub mod result;
pub use imports::{AeroRpcClient, Resolver, WrpcEncoding};
pub mod node;
pub mod parse;
pub mod prelude;
pub mod resolver;
