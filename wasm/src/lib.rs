//!
//! # `rusty-kaspa WASM32 bindings`
//!
//! [<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/kaspa-wasm.svg?maxAge=2592000&style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kaspa-wasm)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-kaspa--wasm-56c2a5?maxAge=2592000&style=for-the-badge&logo=docs.rs" height="20">](https://docs.rs/kaspa-wasm)
//! <img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">
//!
//! <img src="https://img.shields.io/badge/platforms:-informational?style=for-the-badge&color=555555" height="20">
//! <img src="https://img.shields.io/badge/Rust native -informational?style=for-the-badge&color=3080c0" height="20">
//! <img src="https://img.shields.io/badge/wasm32 browser -informational?style=for-the-badge&color=3080c0" height="20">
//! <img src="https://img.shields.io/badge/wasm32 node.js -informational?style=for-the-badge&color=3080c0" height="20">
//!

#![allow(unused_imports)]

pub use kaspa_addresses::{Address, Version as AddressVersion};
pub use kaspa_consensus_core::tx::{
    ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
pub use kaspa_consensus_core::wasm::keypair::{Keypair, PrivateKey};

pub mod rpc {
    //! Kaspa RPC interface
    pub use kaspa_rpc_core::model::message::*;
    pub use kaspa_wrpc_client::wasm::RpcClient;
}

pub use kaspa_wallet_core::{
    account::Account,
    signer::{js_sign_transaction as sign_transaction, Signer},
    storage::Store,
    utxo::UtxoSet,
    wallet::Wallet,
};
