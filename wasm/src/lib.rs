/*!
# `rusty-kaspa WASM32 bindings`

[<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
[<img alt="crates.io" src="https://img.shields.io/crates/v/kaspa-wasm.svg?maxAge=2592000&style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kaspa-wasm)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-kaspa--wasm-56c2a5?maxAge=2592000&style=for-the-badge&logo=docs.rs" height="20">](https://docs.rs/kaspa-wasm)
<img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">

<br>

Rusty-Kaspa WASM32 bindings offer direct integration of Rust code and Rusty-Kaspa
codebase within JavaScript environments such as Node.js and Web Browsers.

The APIs are currently separated into the following groups:

- **Transaction API** — Bindings for primitives related to transactions.
This includes basic primitives related to consensus transactions, as well as
`MutableTransaction` and `VirtualTransaction` primitives usable for 
transaction creation.

- **Wallet API** — API for async core wallet processing tasks.

- **RPC API** — [RPC interface bindings](rpc) for the Kaspa node using WebSocket connections.
Compatible with Rusty Kaspa as well as with the Golang node (kaspad) via the `kaspa-wrpc-proxy` 
WebSocket / gRPC proxy (located in `rpc/wrpc/proxy`).

# Using RPC

**NODEJS:** To use WASM RPC client in the Node.js environment, you need to introduce a W3C WebSocket object 
before loading the WASM32 library. You can use any Node.js module that exposes a W3C-compatible 
WebSocket implementation. Two of such modules are [WebSocket](https://www.npmjs.com/package/websocket) 
(provides a custom implementation) and [isomorphic-ws](https://www.npmjs.com/package/isomorphic-ws) 
(built on top of the ws WebSocket module).

You can use the following shims:

```js
// WebSocket
globalThis.WebSocket = require('websocket').w3cwebsocket;
// isomorphic-ws
globalThis.WebSocket = require('isomorphic-ws');
```



*/

#![allow(unused_imports)]

pub use kaspa_addresses::{Address, Version as AddressVersion};
pub use kaspa_consensus_core::tx::{
    ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
pub use kaspa_consensus_core::wasm::keypair::{Keypair, PrivateKey};

pub mod rpc {
    //! Kaspa RPC interface
    //! 

    pub mod messages {
        //! Kaspa RPC messages
        pub use kaspa_rpc_core::model::message::*;
    }
    pub use kaspa_wrpc_client::wasm::RpcClient;
    pub use kaspa_rpc_core::api::rpc::RpcApi;
}

pub use kaspa_wallet_core::{
    account::Account,
    signer::{js_sign_transaction as sign_transaction, Signer},
    storage::Store,
    tx::{MutableTransaction, VirtualTransaction},
    utxo::{UtxoEntryReference, UtxoOrdering, UtxoSet},
    wallet::Wallet,
};
