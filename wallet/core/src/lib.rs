//!
//! Kaspa Wallet Core - Multi-platform Rust framework for Kaspa Wallet.
//!
//! This framework provides a series of APIs and primitives
//! to simplify building applications that interface with
//! the Kaspa p2p network.
//!
//! Included are low-level primitives
//! such as [`UtxoProcessor`](crate::utxo::UtxoProcessor)
//! and [`UtxoContext`](crate::utxo::UtxoContext) that provide
//! various levels of automation as well as higher-level
//! APIs such as [`Wallet`](crate::runtime::Wallet),
//! [`Account`](crate::runtime::Account) (managed via
//! [`WalletApi`](crate::api::WalletApi) trait)
//! that offer a fully-featured wallet implementation
//! backed by a multi-platform data storage layer capable of
//! storing wallet data on a local file-system as well as
//! within the browser environment.
//!
//! The wallet framework also includes transaction
//! [`Generator`](crate::tx::generator::Generator)
//! that can be used to generate transactions from a set of
//! UTXO entries. The generator can be used to create
//! simple transactions as well as batch transactions
//! comprised of multiple chained transactions.  Batch
//! transactions (also known as compound transactions)
//! are needed when the total number of inputs required
//! to satisfy the requested amount exceeds the maximum
//! allowed transaction mass.
//!
//! The framework can operate
//! within native Rust applications as well as within the NodeJS
//! and browser environments via WASM32.
//!

extern crate alloc;
extern crate self as kaspa_wallet_core;

pub mod account;
pub mod api;
pub mod derivation;
pub mod deterministic;
pub mod encryption;
pub mod error;
pub mod events;
pub mod factory;
mod imports;
pub mod message;
pub mod prelude;
pub mod result;
pub mod rpc;
pub mod secret;
pub mod settings;
pub mod storage;
pub mod tx;
pub mod utils;
pub mod utxo;
pub mod wallet;
pub mod wasm;

/// Returns the version of the Wallet framework.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
pub mod tests;
