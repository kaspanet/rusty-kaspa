
## `kaspa-wasm` WASM32 bindings for Kaspa

[<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
[<img alt="crates.io" src="https://img.shields.io/crates/v/kaspa-wasm.svg?maxAge=2592000&style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kaspa-wasm)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-kaspa--wasm-56c2a5?maxAge=2592000&style=for-the-badge&logo=docs.rs" height="20">](https://docs.rs/kaspa-wasm)
<img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">

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

The APIs are currently separated into the following groups:

- **Transaction API** — Bindings for primitives related to transactions.
This includes basic primitives related to consensus transactions, as well as
`MutableTransaction` and `VirtualTransaction` primitives usable for 
transaction creation.

- **Wallet API** — API for async core wallet processing tasks.

- **RPC API** — [RPC interface bindings](https://docs.rs/kaspa-wasm/latest/kaspa-wasm/rpc) for the Kaspa node using WebSocket connections.
Compatible with Rusty Kaspa as well as with the Golang node (kaspad) via the `kaspa-wrpc-proxy` 
WebSocket / gRPC proxy (located in `rpc/wrpc/proxy`).

## Using RPC

There are multiple ways to use RPC:
- Control over WebSocket-framed JSON-RPC protocol (you have to manually handle serialization)
- Use `RpcClient` class that handles the connectivity automatically and provides RPC interfaces in a form of async function calls.

**NODEJS:** To use WASM RPC client in the Node.js environment, you need to introduce a W3C WebSocket object 
before loading the WASM32 library. You can use any Node.js module that exposes a W3C-compatible 
WebSocket implementation. Two of such modules are [WebSocket](https://www.npmjs.com/package/websocket) 
(provides a custom implementation) and [isomorphic-ws](https://www.npmjs.com/package/isomorphic-ws) 
(built on top of the ws WebSocket module).

You can use the following shims:

```js
// `websocket` module
globalThis.WebSocket = require('websocket').w3cwebsocket;
// `isomorphic-ws` module
globalThis.WebSocket = require('isomorphic-ws');
```

For more details, please follow the [**integrating with Kaspa**](https://kaspa-mdbook.aspectron.com/) guide.
