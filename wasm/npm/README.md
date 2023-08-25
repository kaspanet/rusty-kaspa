# Kaspa WASM SDK

An integration wrapper around [`kaspa-wasm`](https://www.npmjs.com/package/kaspa-wasm) module that uses [`ws`](https://www.npmjs.com/package/ws) together with the  [`isomorphic-ws`](https://www.npmjs.com/package/isomorphic-ws) w3c adaptor for WebSocket communication.

## Usage

Kaspa module exports include all WASM32 bindings.
```javascript
const kaspa = require('kaspa');
```