/*!
# `rusty-kaspa WASM32 bindings`

[<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
[<img alt="crates.io" src="https://img.shields.io/crates/v/kaspa-wasm.svg?maxAge=2592000&style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kaspa-wasm)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-kaspa--wasm-56c2a5?maxAge=2592000&style=for-the-badge&logo=docs.rs" height="20">](https://docs.rs/kaspa-wasm)
<img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">

<br>

Rusty-Kaspa WASM32 bindings offer direct integration of Rust code and Rusty-Kaspa
codebase within JavaScript environments such as Node.js and Web Browsers.

## Documentation

- [**integrating with Kaspa** guide](https://kaspa-mdbook.aspectron.com/)
- [**Rustdoc** documentation](https://docs.rs/kaspa-wasm/latest/kaspa-wasm)
- [**JSDoc** documentation](https://aspectron.com/docs/kaspa-wasm/)

Please note that while WASM directly binds JacaScript and Rust resources, their names on JavaScript side
are different from their name in Rust as they conform to the 'camelCase' convention in JavaScript and
to the 'snake_case' convention in Rust.

## Interfaces

The APIs are currently separated into the following groups (this will be expanded in the future):

- **Transaction API** — Bindings for primitives related to transactions.
This includes basic primitives related to consensus transactions, as well as
`MutableTransaction` and `VirtualTransaction` primitives usable for
transaction creation.

- **Wallet API** — API for async core wallet processing tasks.

- **RPC API** — [RPC interface bindings](rpc) for the Kaspa node using WebSocket connections.
Compatible with Rusty Kaspa as well as with the Golang node (kaspad) via the `kaspa-wrpc-proxy`
WebSocket / gRPC proxy (located in `rpc/wrpc/proxy`).

## Using RPC

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

## Loading in a Web App

```html
<html>
    <head>
        <script type="module">
            import * as kaspa_wasm from './kaspa/kaspa-wasm.js';
            (async () => {
                const kaspa = await kaspa_wasm.default('./kaspa/kaspa-wasm_bg.wasm');
            })();
        </script>
    </head>
    <body></body>
</html>
```

## Loading in a Node.js App

```javascript
// W3C WebSocket module shim
globalThis.WebSocket = require('websocket').w3cwebsocket;

let {RpcClient,Encoding,init_console_panic_hook,defer} = require('./kaspa-rpc');
// init_console_panic_hook();

let rpc = new RpcClient(Encoding.Borsh,"ws://127.0.0.1:17110");

(async () => {
    await rpc.connect();

    let info = await rpc.getInfo();
    console.log(info);

    await rpc.disconnect();
})();
```

For more details, please follow the [**integrating with Kaspa**](https://kaspa-mdbook.aspectron.com/) guide.

*/

#![allow(unused_imports)]

pub mod utils;
pub use crate::utils::*;

pub use kaspa_addresses::{Address, Version as AddressVersion};
pub use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
pub use kaspa_pow::wasm::*;

pub mod rpc {
    //! Kaspa RPC interface
    //!

    pub mod messages {
        //! Kaspa RPC messages
        pub use kaspa_rpc_core::model::message::*;
    }
    pub use kaspa_rpc_core::api::rpc::RpcApi;
    pub use kaspa_wrpc_client::wasm::RpcClient;
}

pub use kaspa_wallet_core::{
    keypair::{Keypair, PrivateKey},
    runtime::account::Account,
    runtime::wallet::Wallet,
    signer::{js_sign_transaction as sign_transaction, Signer},
    storage::local::Store,
    tx::{MutableTransaction, VirtualTransaction},
    utxo::{UtxoEntry, UtxoEntryReference, UtxoOrdering, UtxoSet},
};
