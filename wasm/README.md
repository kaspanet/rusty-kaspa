
## `kaspa-wasm` WASM32 bindings for Kaspa

[<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
[<img alt="crates.io" src="https://img.shields.io/crates/v/kaspa-wasm.svg?maxAge=2592000&style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kaspa-wasm)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-kaspa--wasm-56c2a5?maxAge=2592000&style=for-the-badge&logo=docs.rs" height="20">](https://docs.rs/kaspa-wasm)
<img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">

Rusty-Kaspa WASM32 bindings offer direct integration of Rust code and Rusty-Kaspa
codebase within JavaScript environments such as Node.js and Web Browsers.

## Documentation

- [**integrating with Kaspa** guide](https://kaspa.aspectron.org/)
- [**Rustdoc** documentation](https://docs.rs/kaspa-wasm/latest/kaspa-wasm)
- [**JSDoc** documentation](https://kaspa.aspectron.org/jsdoc/)

Please note that while WASM directly binds JavaScript and Rust resources, their names on JavaScript side
are different from their name in Rust as they conform to the 'camelCase' convention in JavaScript and 
to the 'snake_case' convention in Rust. 

## Interfaces

The APIs are currently separated into the following groups:

- **Transaction API** — Bindings for primitives related to transactions.
- **RPC API** — [RPC interface bindings](https://docs.rs/kaspa-wasm/latest/kaspa-wasm/rpc) for the Kaspa node using WebSocket (wRPC) connections.
- **Wallet API** — API for async core wallet processing tasks.

## Using RPC

There are multiple ways to use RPC:
- Control over WebSocket-framed JSON-RPC protocol (you have to manually handle serialization)
- Use `RpcClient` class that handles the connectivity automatically and provides RPC interfaces in a form of async function calls.

**NODEJS:** To use WASM RPC client in the Node.js environment, you need to introduce a W3C WebSocket object 
before loading the WASM32 library. You can use any Node.js module that exposes a W3C-compatible 
WebSocket implementation. Two of such modules are [WebSocket](https://www.npmjs.com/package/websocket) 
(provides a custom implementation) and [isomorphic-ws](https://www.npmjs.com/package/isomorphic-ws) 
(built on top of the ws WebSocket module).


## Loading in a Web App

```html
<html>
    <head>
        <script type="module">
            import * as kaspa_wasm from './kaspa/kaspa-wasm.js';
            (async () => {
                const kaspa = await kaspa_wasm.default('./kaspa/kaspa-wasm_bg.wasm');
                // ...
            })();
        </script>
    </head>
    <body></body>
</html>
```

## Loading in a Node.js App

```javascript
// W3C WebSocket module shim
// this is provided by NPM `kaspa` module and is only needed
// if you are building WASM libraries for NodeJS from source
// globalThis.WebSocket = require('websocket').w3cwebsocket;

let {RpcClient,Encoding,initConsolePanicHook} = require('./kaspa-rpc');

// enabling console panic hooks allows WASM to print panic details to console
// initConsolePanicHook();
// enabling browser panic hooks will create a full-page DIV with panic details
// this is useful for mobile devices where console is not available
// initBrowserPanicHook();

// if port is not specified, it will use the default port for the specified network
const rpc = new RpcClient("127.0.0.1", Encoding.Borsh, "testnet-10");

(async () => {
    try {
        await rpc.connect();
        let info = await rpc.getInfo();
        console.log(info);
    } finally {
        await rpc.disconnect();
    }
})();
```

For more details, please follow the [**integrating with Kaspa**](https://kaspa.aspectron.org/) guide.
