# Kaspa WASM SDK

An integration wrapper around [`kaspa-wasm`](https://www.npmjs.com/package/kaspa-wasm) module that uses [`websocket`](https://www.npmjs.com/package/websocket) W3C adaptor for WebSocket communication.

This is a Node.js module that provides bindings to the Kaspa WASM SDK strictly for use in the Node.js environment. The web browser version of the SDK is available as part of official SDK releases at [https://github.com/kaspanet/rusty-kaspa/releases](https://github.com/kaspanet/rusty-kaspa/releases)

## Usage

Kaspa NPM module exports include all WASM32 bindings.
```javascript
const kaspa = require('kaspa');
console.log(kaspa.version());
```

## Documentation

Documentation is available at [https://kaspa.aspectron.org/docs/](https://kaspa.aspectron.org/docs/)


## Building from source & Examples

SDK examples as well as information on building the project from source can be found at [https://github.com/kaspanet/rusty-kaspa/tree/master/wasm](https://github.com/kaspanet/rusty-kaspa/tree/master/wasm)

## Releases

Official releases as well as releases for Web Browsers are available at [https://github.com/kaspanet/rusty-kaspa/releases](https://github.com/kaspanet/rusty-kaspa/releases).

Nightly / developer builds are available at: [https://aspectron.org/en/projects/kaspa-wasm.html](https://aspectron.org/en/projects/kaspa-wasm.html)

