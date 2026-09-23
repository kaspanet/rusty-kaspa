## WASM32 bindings for Rusty Kaspa SDK

[<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
[<img alt="crates.io" src="https://img.shields.io/crates/v/kaspa-wasm.svg?maxAge=2592000&style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/kaspa-wasm)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-kaspa--wasm-56c2a5?maxAge=2592000&style=for-the-badge&logo=docs.rs" height="20">](https://docs.rs/kaspa-wasm)
<img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">

Rusty-Kaspa WASM32 bindings offer direct integration of Rust code and Rusty-Kaspa
codebase within JavaScript and TypeScript environments such as Node.js and Web Browsers.

## Documentation

- [**integrating with Kaspa** guide](https://kaspa.aspectron.org/)
- [**Rust** documentation](https://docs.rs/kaspa-wasm/latest/kaspa_wasm/index.html)
- [**TypeScript** documentation](https://kaspa.aspectron.org/docs/)

Please note that while WASM directly binds JavaScript and Rust resources, their names on JavaScript side
are different from their name in Rust as they conform to the 'camelCase' convention in JavaScript and
to the 'snake_case' convention in Rust.

The WASM32 bindings can be used in both TypeScript and JavaScript environments, where in JavaScript
types will not be constrained by TypeScript type definitions.

## Interfaces

The SDK is currently separated into the following top-level categories:

- **RPC API** — RPC API for the Kaspa node using WebSockets.
- **Wallet SDK** — Bindings for primitives related to key management and transactions.
- **Wallet API** — API for the Rusty Kaspa Wallet framework.

## WASM32 SDK release packages

The SDK is built as 4 packages for Web Browsers as follows:

- KeyGen - Key & Address Generation only
- RPC - RPC only
- Core - RPC + Key & Address Generation + Wallet SDK
- Full - Full SDK + Integrated Wallet For NodeJS, the SDK is built as a single package containing all features.

## SDK folder structure

The following is a brief overview of the SDK folder structure (as available in the release):

- `web/kaspa` - **full** Rusty Kaspa WASM32 SDK bindings for use in web browsers.
- `web/kaspa-rpc` - only the RPC bindings for use in web browsers (reduced WASM binary size).
- `nodejs/kaspa` - **full** Rusty Kaspa WASM32 SDK bindings for use with NodeJS.
- `docs` - Rusty Kaspa WASM32 SDK documentation.
- `examples` - runnable TypeScript examples (see its [`README.md`](./examples/README.md)).

Included documentation in the release can be accessed by loading the `docs/kaspa/index.html`
file in a web browser.

## Building from Source

To build the WASM32 SDK from source, you need to have the Rust environment installed. To do that,
follow instructions in the [Rusty Kaspa README](https://github.com/kaspanet/rusty-kaspa).

Once you have Rust installed, you can build the WASM32 SDK as follows:

- `./build-release` - build the release version of the WASM32 SDK + Docs. The release version also contains `debug` builds of the libraries.
- `./build-web` - build the web package (ES6 module)
- `./build-node` - build the NodeJS package (CommonJS module)
- `./build-docs` - runs `build-web` and then generates TypeDoc documentation from the resulting build.

Please note that to build from source, you need to have TypeDoc installed globally via `npm install -g typedoc`.

## Creating Documentation

Please note that to build documentation from source you need to have the Rust environment installed.
The build script will first build the WASM32 SDK and then generate typedoc documentation from it.

You can build documentation from source as follows:

```bash
npm install -g typedoc
./build-docs
```

The resulting documentation will be located in `docs/typedoc/`

## Running examples

Runnable examples for both NodeJS and the browser live in `examples/`. See
[`examples/README.md`](./examples/README.md) for how to run the TypeScript
recipes and serve the web examples.

## Loading in a Web App

```html
<html>
  <head>
    <script type="module">
      import init, { initBrowserPanicHook, version } from "./kaspa/kaspa-wasm.js";

      (async () => {
        await init("./kaspa/kaspa-wasm_bg.wasm");

        // enabling browser panic hooks creates a full-page DIV with panic details
        // useful for mobile devices where the console is not available
        initBrowserPanicHook();

        console.log(version());
      })();
    </script>
  </head>
  <body></body>
</html>
```

## Loading in a Node.js App

```javascript
//
// W3C WebSocket module shim.
// Only required if you use RPC && are running on Node < 22
import { WebSocket } from "ws";
// @ts-ignore
globalThis.WebSocket = WebSocket; // before rpc.connect()
//

import { initConsolePanicHook, version } from "./kaspa";

// enabling console panic hooks allows WASM to print panic details to console
initConsolePanicHook();

console.log(version());
```

## Using RPC

There are multiple ways to use RPC:

- Control over WebSocket-framed JSON-RPC protocol (you have to manually handle serialization)
- Use `RpcClient` class that handles the connectivity automatically and provides RPC interfaces in a form of async function calls.

```javascript
import { RpcClient, Encoding } from "./kaspa";

// if port is not specified, it will use the default port for the specified network
const rpc = new RpcClient({
  url: "127.0.0.1",
  encoding: Encoding.Borsh,
  network: "testnet-10",
});

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
