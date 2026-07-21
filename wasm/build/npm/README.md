<!-- This file is published as the README of the @kaspa/sdk-wasm npm package -->

## Kaspa WASM SDK

[<img alt="github" src="https://img.shields.io/badge/github-kaspanet/rusty--kaspa-8da0cb?style=for-the-badge&labelColor=555555&color=8da0cb&logo=github" height="20">](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)
<img alt="license" src="https://img.shields.io/crates/l/kaspa-wasm.svg?maxAge=2592000&color=6ac&style=for-the-badge&logoColor=fff" height="20">

WASM32 bindings for the [Rusty Kaspa](https://github.com/kaspanet/rusty-kaspa) SDK - direct
integration of the Rusty Kaspa codebase within JavaScript and TypeScript environments such as
Node.js and web browsers.

The SDK provides:

- **Key & Address Generation** - BIP-39 mnemonics, HD (BIP-32) key derivation, and address creation.
- **Transactions** - Primitives for building, signing, and submitting transactions.
- **RPC API** - RPC client for the Kaspa node using WebSocket (wRPC) connections.
- **Wallet SDK** - Bindings for async core wallet processing tasks.
- **Wallet API** - API for the fully-featured Rusty Kaspa Wallet framework.

## Installation

```bash
npm install @kaspa/sdk-wasm
```

## Initializing

In web browsers, initialize the WASM module before using the SDK:

```javascript
import init, { version } from '@kaspa/sdk-wasm';
await init();
console.log(version());
```

In server runtimes, load the WASM binary manually:

```javascript
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { initSync, version } from '@kaspa/sdk-wasm';

const require = createRequire(import.meta.url);
initSync({ module: readFileSync(require.resolve('@kaspa/sdk-wasm/kaspa_bg.wasm')) });
console.log(version());
```

The package is an ES module; CommonJS consumers can `require()` it on Node.js 22.12 or newer.
Node.js provides a global `WebSocket` (needed for the RPC client) since version 22; on older
Node.js versions, assign a W3C-compatible shim such as
[`ws`](https://www.npmjs.com/package/ws) to `globalThis.WebSocket` before connecting.

## Quickstart

Generate a mnemonic and derive a mainnet address:

```javascript
import { Mnemonic, XPrv, createAddress, NetworkType } from '@kaspa/sdk-wasm';

const mnemonic = Mnemonic.random();
const xprv = new XPrv(mnemonic.toSeed());
const publicKey = xprv.derivePath("m/44'/111111'/0'/0/0").toXPub().toPublicKey();
const address = createAddress(publicKey, NetworkType.Mainnet);
console.log(String(address));
```

Connect to a public node and query it over RPC:

```javascript
import { RpcClient, Resolver } from '@kaspa/sdk-wasm';

const rpc = new RpcClient({ resolver: new Resolver(), networkId: 'mainnet' });
await rpc.connect();
try {
    const { isSynced, serverVersion } = await rpc.getServerInfo();
    console.log(`connected to Kaspa node ${serverVersion}, synced: ${isSynced}`);
} finally {
    await rpc.disconnect();
}
```
## Documentation

- [**integrating with Kaspa** guide](https://kaspa.aspectron.org/)
- [**TypeScript** documentation](https://kaspa.aspectron.org/docs/)
- [**Rust** documentation](https://docs.rs/kaspa-wasm/latest/kaspa_wasm/index.html)

## License

ISC
